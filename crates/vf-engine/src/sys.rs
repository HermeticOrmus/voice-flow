//! Small helpers over the operating system: the environment as `${V:-default}` reads it, PATH
//! lookup, temp files and a clock in milliseconds.

use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use vf_core::conf::Conf;
use vf_core::dirs::Dirs;

/// `${KEY:-}`: a variable that is set and not empty.
pub fn var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

pub fn dirs() -> Dirs {
    Dirs::from_env(&|k| std::env::var(k).ok())
}

/// The conf file's keys: `$VOICE_FLOW_CONF`, else `voice-flow.conf` in the config dir. No file is no keys.
pub fn load_conf(dirs: &Dirs) -> Conf {
    let path = var("VOICE_FLOW_CONF")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs.conf_file());
    Conf::parse(&std::fs::read_to_string(path).unwrap_or_default())
}

/// `${KEY:-$(conf_value KEY)}`: the environment first, then the conf; empty counts as unset.
pub fn env_or_conf(conf: &Conf, key: &str) -> Option<String> {
    var(key).or_else(|| conf.get(key).filter(|v| !v.is_empty()).map(str::to_string))
}

/// `command -v`: the first executable of that name on PATH.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| {
            p.metadata()
                .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        })
}

pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Twelve hex digits from the kernel's random source.
fn random_hex() -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = [0u8; 6];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// `mktemp -t PREFIX-XXXXXX.SUFFIX`: a new empty file in `$TMPDIR` (or /tmp), mode 0600, created
/// exclusively (`create_new` refuses an existing path, a planted symlink included).
pub fn mktemp(prefix: &str, suffix: &str) -> std::io::Result<(PathBuf, File)> {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = std::env::temp_dir();
    loop {
        let path = dir.join(format!("{prefix}-{}{suffix}", random_hex()?));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

/// Removes a path when dropped: the temp files of a take go away on every way out.
pub struct Cleanup(pub Vec<PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for p in &self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mktemp_makes_distinct_files_and_cleanup_removes_them() {
        let (a, _) = mktemp("vf-test", ".wav").unwrap();
        let (b, _) = mktemp("vf-test", ".wav").unwrap();
        assert_ne!(a, b);
        assert!(a.exists());
        drop(Cleanup(vec![a.clone(), b.clone()]));
        assert!(!a.exists() && !b.exists());
    }

    #[test]
    fn a_temp_file_is_private_and_named_from_random_bytes() {
        use std::os::unix::fs::PermissionsExt;
        let (a, _) = mktemp("vf-mode", ".ogg").unwrap();
        let (b, _) = mktemp("vf-mode", ".ogg").unwrap();
        for p in [&a, &b] {
            assert_eq!(
                p.metadata().unwrap().permissions().mode() & 0o777,
                0o600,
                "{p:?}"
            );
        }
        let name = |p: &PathBuf| p.file_name().unwrap().to_string_lossy().into_owned();
        // vf-mode-<12 hex>.ogg: no pid, no clock.
        let n = name(&a);
        let hex = n
            .strip_prefix("vf-mode-")
            .and_then(|r| r.strip_suffix(".ogg"))
            .unwrap();
        assert!(
            hex.len() == 12 && hex.chars().all(|c| c.is_ascii_hexdigit()),
            "{n}"
        );
        assert_ne!(name(&a), name(&b));
        drop(Cleanup(vec![a, b]));
    }

    #[test]
    fn a_name_that_exists_is_never_opened() {
        // create_new refuses an existing path, a planted symlink included.
        let (a, _) = mktemp("vf-excl", ".wav").unwrap();
        let link = a.with_extension("lnk");
        std::os::unix::fs::symlink(&a, &link).unwrap();
        let err = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&link)
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        drop(Cleanup(vec![a, link]));
    }

    #[test]
    fn env_or_conf_prefers_the_environment_and_skips_empty() {
        let conf = Conf::parse("VF_TEST_KEY_ONLY_CONF=from-conf\nVF_TEST_EMPTY=\n");
        assert_eq!(
            env_or_conf(&conf, "VF_TEST_KEY_ONLY_CONF").as_deref(),
            Some("from-conf")
        );
        assert_eq!(env_or_conf(&conf, "VF_TEST_EMPTY"), None);
        assert_eq!(env_or_conf(&conf, "VF_TEST_NOWHERE"), None);
    }
}
