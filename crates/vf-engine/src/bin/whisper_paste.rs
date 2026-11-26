//! whisper-paste: the take. Same command line, environment and files as bin/whisper-paste.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(vf_engine::whisper_paste::main_with(&args));
}
