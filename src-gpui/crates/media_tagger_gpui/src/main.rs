use std::process::ExitCode;

/// Prints the identifier baked into this binary and exits; build scripts use it to
/// prove which profile a binary opens before anything launches it.
const IDENTIFIER_FLAG: &str = "--identifier";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some(IDENTIFIER_FLAG) {
        println!("{}", media_tagger_gpui::profile::BUILT_IDENTIFIER);
        return ExitCode::SUCCESS;
    }
    match media_tagger_gpui::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Tagrove GPUI cannot start: {error}");
            ExitCode::FAILURE
        }
    }
}
