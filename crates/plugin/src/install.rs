//! Installing a plugin from a `.zip` (ADR 0003): the archive holds `plugin.toml` and the
//! component, either at the root or in one top folder. Paths are checked (no `..`, no absolute
//! paths, size limits); the plugin lands in `<plugins>/<id>/`.

use std::io::Read;
use std::path::{Component, Path, PathBuf};

use formcraft_plugin_api::Manifest;

const MAX_FILES: usize = 1000;
const MAX_TOTAL: u64 = 256 << 20;

/// Extract `zip` into `plugins/<id>` (replacing an older version). Returns the plugin folder.
pub fn from_zip(zip: &Path, plugins: &Path) -> Result<(Manifest, PathBuf), String> {
    let file = std::fs::File::open(zip).map_err(|e| format!("{}: {e}", zip.display()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("{}: not a zip: {e}", zip.display()))?;
    if archive.len() > MAX_FILES {
        return Err("too many files in the archive".into());
    }
    // find plugin.toml (root or one folder deep) → the prefix to strip
    let mut prefix = None;
    for i in 0..archive.len() {
        let f = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = f.name().map_err(|e| e.to_string())?.replace('\\', "/");
        let parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty()).collect();
        match parts.as_slice() {
            ["plugin.toml"] => prefix = Some(String::new()),
            [dir, "plugin.toml"] if prefix.is_none() => prefix = Some(format!("{dir}/")),
            _ => {}
        }
    }
    let prefix = prefix.ok_or("no plugin.toml in the archive")?;
    let mut text = String::new();
    archive
        .by_name(&format!("{prefix}plugin.toml"))
        .map_err(|e| e.to_string())?
        .take(64 * 1024)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    let manifest = Manifest::parse(&text).map_err(|e| e.to_string())?;
    let dest = plugins.join(&manifest.id);
    let staging = plugins.join(format!(".{}.installing", manifest.id));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = f.name().map_err(|e| e.to_string())?.replace('\\', "/");
        let Some(rel) = name.strip_prefix(&prefix) else { continue };
        if rel.is_empty() || f.is_dir() {
            continue;
        }
        let rel_path = Path::new(rel);
        if rel_path.components().any(|c| !matches!(c, Component::Normal(_))) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(format!("unsafe path in the archive: {name}"));
        }
        total += f.size();
        if total > MAX_TOTAL {
            let _ = std::fs::remove_dir_all(&staging);
            return Err("the archive is too large".into());
        }
        let out = staging.join(rel_path);
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let mut w = std::fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
        std::io::copy(&mut (&mut f).take(MAX_TOTAL), &mut w).map_err(|e| e.to_string())?;
    }
    if !staging.join(&manifest.wasm).is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!("`{}` (named in plugin.toml) is missing", manifest.wasm));
    }
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok((manifest, dest))
}

/// `<config>/plugins` (FORMCRAFT_PLUGINS overrides): `%APPDATA%/FormCraft/plugins` on Windows,
/// `$XDG_CONFIG_HOME/formcraft/plugins` or `~/.config/formcraft/plugins` elsewhere.
pub fn default_dir() -> Option<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(d) = var("FORMCRAFT_PLUGINS") {
        return Some(d);
    }
    if let Some(d) = var("APPDATA") {
        return Some(d.join("FormCraft").join("plugins"));
    }
    let cfg = var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config")))?;
    Some(cfg.join("formcraft").join("plugins"))
}
