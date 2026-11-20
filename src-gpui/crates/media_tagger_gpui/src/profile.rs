//! Application profiles of the GPUI port (decision D5, ADR 0036).
//!
//! The port has two isolated profiles and no release identifier before epic E22, so
//! every other identifier is refused, in debug and release builds alike. The Tauri
//! profiles (`com.example.mediatagger`, `.dev`, `.e2e`) are deliberately not on the
//! list: the two applications never share `media.db`, `thumbs/` or `instance.lock`.

use std::{
    ffi::OsStr,
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
};

/// The identifier baked in by `build.rs` (`MEDIATAGGER_GPUI_IDENTIFIER`, default: dev).
pub const BUILT_IDENTIFIER: &str = env!("MEDIATAGGER_GPUI_IDENTIFIER");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    /// Names the app-data directory and the desktop application id.
    pub identifier: &'static str,
    pub window_title: &'static str,
}

pub const DEV: Profile = Profile {
    identifier: "com.example.mediatagger.gpui.dev",
    window_title: "Tagrove GPUI Dev",
};

pub const E2E: Profile = Profile {
    identifier: "com.example.mediatagger.gpui.e2e",
    window_title: "Tagrove GPUI E2E",
};

const PROFILES: [Profile; 2] = [DEV, E2E];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    /// The identifier is not one of the isolated GPUI profiles.
    NotIsolated(String),
    /// Neither `XDG_DATA_HOME` nor `HOME` names an absolute directory.
    NoDataHome,
}

impl Display for ProfileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotIsolated(identifier) => write!(
                f,
                "Refusing to start with identifier {identifier}. The GPUI port runs only on the \
                 isolated profiles {} and {}.",
                DEV.identifier, E2E.identifier
            ),
            Self::NoDataHome => write!(
                f,
                "cannot resolve the app-data directory: set XDG_DATA_HOME or HOME to an absolute path"
            ),
        }
    }
}

impl std::error::Error for ProfileError {}

/// The profile of an identifier, or a refusal for anything off the allowlist.
pub fn resolve(identifier: &str) -> Result<Profile, ProfileError> {
    PROFILES
        .into_iter()
        .find(|profile| profile.identifier == identifier)
        .ok_or_else(|| ProfileError::NotIsolated(identifier.to_owned()))
}

/// `${XDG_DATA_HOME:-$HOME/.local/share}/<identifier>`, the directory layout of the
/// Tauri profiles. A relative or empty `XDG_DATA_HOME` is ignored (XDG base directory
/// specification).
pub fn data_dir(
    profile: Profile,
    xdg_data_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Result<PathBuf, ProfileError> {
    let absolute = |value: Option<&OsStr>| {
        value
            .map(Path::new)
            .filter(|path| path.is_absolute())
            .map(Path::to_path_buf)
    };
    let data_home = absolute(xdg_data_home)
        .or_else(|| absolute(home).map(|home| home.join(".local").join("share")))
        .ok_or(ProfileError::NoDataHome)?;
    Ok(data_home.join(profile.identifier))
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsStr, path::PathBuf};

    use super::{data_dir, resolve, ProfileError, BUILT_IDENTIFIER, DEV, E2E};

    #[test]
    fn permits_only_the_isolated_gpui_profiles() {
        assert_eq!(resolve("com.example.mediatagger.gpui.dev"), Ok(DEV));
        assert_eq!(resolve("com.example.mediatagger.gpui.e2e"), Ok(E2E));
        for refused in [
            "com.example.mediatagger",
            "com.example.mediatagger.dev",
            "com.example.mediatagger.e2e",
            "com.example.mediatagger.gpui",
            "com.example.mediatagger.gpui.dev.extra",
            "io.github.publisher.Tagrove",
            "",
        ] {
            assert_eq!(
                resolve(refused),
                Err(ProfileError::NotIsolated(refused.to_owned())),
                "{refused:?}"
            );
        }
    }

    #[test]
    fn refusal_names_the_identifier_and_the_allowed_profiles() {
        let message = resolve("com.example.mediatagger").unwrap_err().to_string();

        assert!(
            message.starts_with("Refusing to start with identifier com.example.mediatagger."),
            "{message}"
        );
        assert!(message.contains(DEV.identifier), "{message}");
        assert!(message.contains(E2E.identifier), "{message}");
    }

    #[test]
    fn profiles_have_their_own_titles() {
        assert_eq!(DEV.window_title, "Tagrove GPUI Dev");
        assert_eq!(E2E.window_title, "Tagrove GPUI E2E");
    }

    #[test]
    fn the_built_identifier_is_an_isolated_profile() {
        assert!(resolve(BUILT_IDENTIFIER).is_ok(), "{BUILT_IDENTIFIER}");
    }

    #[test]
    fn data_dir_prefers_an_absolute_xdg_data_home() {
        let dir = data_dir(
            DEV,
            Some(OsStr::new("/synthetic/data")),
            Some(OsStr::new("/synthetic/user")),
        );

        assert_eq!(
            dir,
            Ok(PathBuf::from(
                "/synthetic/data/com.example.mediatagger.gpui.dev"
            ))
        );
    }

    #[test]
    fn data_dir_falls_back_to_the_home_directory() {
        let expected =
            PathBuf::from("/synthetic/user/.local/share/com.example.mediatagger.gpui.e2e");

        for xdg_data_home in [
            None,
            Some(OsStr::new("")),
            Some(OsStr::new("relative/data")),
        ] {
            assert_eq!(
                data_dir(E2E, xdg_data_home, Some(OsStr::new("/synthetic/user"))),
                Ok(expected.clone()),
                "{xdg_data_home:?}"
            );
        }
    }

    #[test]
    fn data_dir_needs_an_absolute_base() {
        for home in [
            None,
            Some(OsStr::new("")),
            Some(OsStr::new("relative/home")),
        ] {
            assert_eq!(
                data_dir(DEV, None, home),
                Err(ProfileError::NoDataHome),
                "{home:?}"
            );
        }
    }
}
