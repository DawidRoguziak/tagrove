use std::process::ExitCode;

use media_tagger_gpui::cli::{self, Command};

fn main() -> ExitCode {
    let outcome = match cli::parse(std::env::args().skip(1)) {
        // Build scripts use this to prove which profile a binary opens before anything
        // launches it.
        Ok(Command::PrintIdentifier) => {
            println!("{}", media_tagger_gpui::profile::BUILT_IDENTIFIER);
            return ExitCode::SUCCESS;
        }
        Ok(Command::Run { scenario }) => media_tagger_gpui::run(scenario.as_deref()),
        Err(error) => Err(error.into()),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Tagrove GPUI cannot start: {error}");
            ExitCode::FAILURE
        }
    }
}
