//! Bakes the application identifier into the binary, the way a Tauri build bakes the
//! identifier of its `--config` file. `bun run gpui:dev` builds the dev profile;
//! `bun run gpui:build:e2e` sets `MEDIATAGGER_GPUI_IDENTIFIER` for the E2E profile.
//! The binary checks the baked value against its allowlist at startup (`src/profile.rs`).

const VARIABLE: &str = "MEDIATAGGER_GPUI_IDENTIFIER";
const DEV_IDENTIFIER: &str = "com.example.mediatagger.gpui.dev";

fn main() {
    println!("cargo:rerun-if-env-changed={VARIABLE}");
    let identifier = std::env::var(VARIABLE).unwrap_or_else(|_| DEV_IDENTIFIER.to_owned());
    let well_formed = !identifier.is_empty()
        && identifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    assert!(
        well_formed,
        "{VARIABLE} must be a reverse-DNS identifier, got {identifier:?}"
    );
    println!("cargo:rustc-env={VARIABLE}={identifier}");
}
