//! HTTP through the `curl` binary, with the same arguments the shell scripts pass. Slice 1 keeps
//! curl so the test suite's curl stub stays the contract (its log records every argument).

use std::process::{Command, Stdio};

pub struct Reply {
    /// curl's exit code; 0 is an answer, 7 is no connection, 28 is a timeout. 127: no curl.
    pub code: i32,
    pub stdout: String,
}

impl Reply {
    pub fn ok(&self) -> bool {
        self.code == 0
    }
}

pub fn curl(args: &[String]) -> Reply {
    curl_stdin(args, None)
}

/// curl with a request body on stdin (`--data-binary @-`): the body stays out of argv, where any
/// local user can read it in /proc/*/cmdline and where it hits the size limit.
pub fn curl_stdin(args: &[String], body: Option<&[u8]>) -> Reply {
    use std::io::Write;
    let stdin = if body.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    };
    let Ok(mut child) = Command::new("curl")
        .args(args)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return Reply {
            code: 127,
            stdout: String::new(),
        };
    };
    if let (Some(body), Some(mut pipe)) = (body, child.stdin.take()) {
        let body = body.to_vec();
        // A thread feeds the pipe so a large body cannot deadlock against curl's output.
        std::thread::spawn(move || {
            let _ = pipe.write_all(&body);
        });
    }
    match child.wait_with_output() {
        Ok(o) => Reply {
            code: o.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        },
        Err(_) => Reply {
            code: 127,
            stdout: String::new(),
        },
    }
}
