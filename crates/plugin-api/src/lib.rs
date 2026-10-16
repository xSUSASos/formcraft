//! Plugin API v1 (ADR 0003): the WIT world (`wit/formcraft.wit`) and the manifest format shared
//! by the host and plugin authors.
//!
//! ```toml
//! id = "gear"
//! name = "Spur Gear"
//! version = "1.0.0"
//! author = "FormCraft examples"
//! license = "MIT"
//! api = "1.0"
//! wasm = "gear.wasm"
//!
//! [permissions]
//! files = []          # folders the plugin may read/write (the user confirms on install)
//! network = false
//! ```
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use serde::{Deserialize, Serialize};

/// The WIT world, embedded for tools and docs.
pub const WIT: &str = include_str!("../wit/formcraft.wit");

/// API version implemented by this host (major, minor).
pub const API_VERSION: (u32, u32) = (1, 0);

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Permissions {
    /// Folders (absolute, or relative to the plugin folder) the plugin may read and write.
    pub files: Vec<String>,
    /// Network access: not available in API v1; a manifest asking for it is refused.
    pub network: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub description: String,
    /// Plugin API version the plugin was built for, "1.0".
    pub api: String,
    /// Component file next to the manifest.
    pub wasm: String,
    #[serde(default)]
    pub permissions: Permissions,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("plugin.toml: {0}")]
    Parse(String),
    #[error("plugin.toml: {0}")]
    Invalid(String),
    #[error("plugin `{id}` is built for API {wanted}, this FormCraft supports API {major}.x")]
    Incompatible { id: String, wanted: String, major: u32 },
}

/// Plugin ids: lower-case ASCII letters, digits, `_` and `-`, 1..=32 bytes, starting with a
/// letter. They prefix command ids (`gear.spur`).
pub fn valid_id(id: &str) -> bool {
    let mut c = id.chars();
    matches!(c.next(), Some(ch) if ch.is_ascii_lowercase())
        && c.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
        && id.len() <= 32
}

fn parse_version(v: &str) -> Option<(u32, u32)> {
    let mut it = v.trim().split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().map_or(Some(0), |m| m.parse().ok())?;
    Some((major, minor))
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        if text.len() > 64 * 1024 {
            return Err(ManifestError::Invalid("the manifest is too large".into()));
        }
        let m: Manifest = toml::from_str(text).map_err(|e| ManifestError::Parse(e.to_string()))?;
        m.validate()?;
        Ok(m)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        let bad = |m: String| Err(ManifestError::Invalid(m));
        if !valid_id(&self.id) {
            return bad(format!("bad id `{}` (lower-case letters, digits, _ and -, ≤ 32)", self.id));
        }
        if self.name.trim().is_empty() || self.name.len() > 80 {
            return bad("`name` must be 1..80 characters".into());
        }
        let wasm = std::path::Path::new(&self.wasm);
        let plain = wasm.components().count() == 1 && wasm.extension().is_some_and(|e| e == "wasm");
        if !plain {
            return bad(format!("`wasm` must be a .wasm file next to the manifest, not `{}`", self.wasm));
        }
        if self.permissions.network {
            return bad("network access is not available in Plugin API v1".into());
        }
        let Some((major, minor)) = parse_version(&self.api) else {
            return bad(format!("bad api version `{}`", self.api));
        };
        if major != API_VERSION.0 || minor > API_VERSION.1 {
            return Err(ManifestError::Incompatible {
                id: self.id.clone(),
                wanted: self.api.clone(),
                major: API_VERSION.0,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
id = "gear"
name = "Spur Gear"
version = "1.0.0"
api = "1.0"
wasm = "gear.wasm"
[permissions]
files = ["exports"]
"#;

    #[test]
    fn parses_and_validates() {
        let m = Manifest::parse(GOOD).expect("valid");
        assert_eq!(m.id, "gear");
        assert_eq!(m.permissions.files, vec!["exports"]);
        assert!(!m.permissions.network);
    }

    #[test]
    fn hostile_manifests_are_errors_not_aborts() {
        for (text, what) in [
            ("", "empty"),
            ("id = 5", "wrong type"),
            (&GOOD.replace("gear\"\nname", "Gear Plugin\"\nname"), "bad id"),
            (&GOOD.replace("gear.wasm", "../../evil.wasm"), "path escape"),
            (&GOOD.replace("gear.wasm", "gear.dll"), "not wasm"),
            (&GOOD.replace("api = \"1.0\"", "api = \"2.0\""), "future major"),
            (&GOOD.replace("api = \"1.0\"", "api = \"1.7\""), "future minor"),
            (&GOOD.replace("api = \"1.0\"", "api = \"one\""), "bad version"),
            (&format!("{GOOD}network = true\n"), "network"),
            (&format!("{GOOD}\nunknown = 1\n"), "unknown key"),
        ] {
            assert!(Manifest::parse(text).is_err(), "{what} must be refused");
        }
        let e = Manifest::parse(&GOOD.replace("api = \"1.0\"", "api = \"2.0\"")).unwrap_err();
        assert!(e.to_string().contains("built for API 2.0"), "{e}");
        assert!(Manifest::parse(&"x".repeat(70_000)).is_err());
    }

    #[test]
    fn wit_is_embedded() {
        assert!(WIT.contains("package formcraft:plugin@1.0.0"));
    }
}
