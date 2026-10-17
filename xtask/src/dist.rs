//! `cargo xtask dist`: release packages (PLAN Phase 7).
//!
//! - every platform: a portable archive `dist/FormCraft-<version>-<os>-x64.zip` with the app,
//!   the CLI, the example add-ins (built components), scripts, the Python client and docs;
//! - Windows: an `.msi` when the WiX toolset (`wix`, `dotnet tool install --global wix`) is
//!   installed — per-user install into `%LOCALAPPDATA%\Programs\FormCraft`, Start-menu entry;
//! - Linux: an AppImage via `packaging/linux/build-appimage.sh` (needs `linuxdeploy`).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{cargo_step, root, run};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn exe(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

/// (archive path, source path) of everything that ships.
fn payload(release: &Path) -> Vec<(String, PathBuf)> {
    let r = root();
    let mut files = vec![
        (exe("formcraft"), release.join(exe("formcraft"))),
        (exe("formcraft-cli"), release.join(exe("formcraft-cli"))),
        ("README.md".into(), r.join("README.md")),
        ("ATTRIBUTION.md".into(), r.join("ATTRIBUTION.md")),
        ("docs/plugins/README.md".into(), r.join("docs/plugins/README.md")),
        ("docs/shortcuts.md".into(), r.join("docs/shortcuts.md")),
        ("python/formcraft/__init__.py".into(), r.join("python/formcraft/__init__.py")),
        ("python/pyproject.toml".into(), r.join("python/pyproject.toml")),
        ("examples/scripts/export_stl.py".into(), r.join("examples/scripts/export_stl.py")),
        ("examples/bracket.fcs".into(), r.join("examples/bracket.fcs")),
    ];
    for plugin in ["gear", "material"] {
        for f in ["plugin.toml", &format!("{plugin}.wasm")] {
            files.push((format!("plugins/{plugin}/{f}"), r.join("examples/plugins").join(plugin).join(f)));
        }
    }
    for m in std::fs::read_dir(r.join("models")).into_iter().flatten().flatten() {
        let p = m.path();
        if p.extension().is_some_and(|e| e == "Modelfile") {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            files.push((format!("models/{name}"), p));
        }
    }
    files
}

fn zip_dir(files: &[(String, PathBuf)], out: &Path) -> Result<(), String> {
    let f = std::fs::File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, src) in files {
        let data = std::fs::read(src).map_err(|e| format!("{}: {e}", src.display()))?;
        z.start_file(format!("FormCraft/{name}"), opts).map_err(|e| e.to_string())?;
        z.write_all(&data).map_err(|e| e.to_string())?;
    }
    z.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn wix_source(files: &[(String, PathBuf)]) -> String {
    // nested <Directory> elements mirror the archive layout (add-ins need their folders)
    let mut dirs: Vec<String> = files
        .iter()
        .filter_map(|(n, _)| n.rsplit_once('/').map(|(d, _)| d.to_string()))
        .flat_map(|d| {
            let parts: Vec<&str> = d.split('/').collect();
            (1..=parts.len()).map(move |i| parts[..i].join("/")).collect::<Vec<_>>()
        })
        .collect();
    dirs.sort();
    dirs.dedup();
    let id = |d: &str| {
        if d.is_empty() { "INSTALLFOLDER".to_string() } else { format!("d_{}", d.replace(['/', '-', '.'], "_")) }
    };
    fn tree(parent: &str, dirs: &[String], id: &dyn Fn(&str) -> String, depth: usize, out: &mut String) {
        for d in dirs.iter().filter(|d| d.rsplit_once('/').map_or("", |(p, _)| p) == parent) {
            let name = d.rsplit('/').next().unwrap_or(d);
            out.push_str(&format!("{}<Directory Id=\"{}\" Name=\"{name}\">\n", "  ".repeat(depth), id(d)));
            tree(d, dirs, id, depth + 1, out);
            out.push_str(&format!("{}</Directory>\n", "  ".repeat(depth)));
        }
    }
    let mut dir_xml = String::new();
    tree("", &dirs, &id, 5, &mut dir_xml);
    let mut comps = String::new();
    for (i, (name, src)) in files.iter().enumerate() {
        let (dir, file) = name.rsplit_once('/').unwrap_or(("", name));
        comps.push_str(&format!(
            "    <Component Id=\"c{i}\" Directory=\"{}\" Guid=\"*\"><File Id=\"f{i}\" Source=\"{}\" Name=\"{file}\" /></Component>\n",
            id(dir),
            src.display()
        ));
    }
    format!(
        r#"<Wix xmlns="http://wixtoolset.org/schemas/v4/wxs">
  <Package Name="FormCraft" Manufacturer="FormCraft" Version="{VERSION}" UpgradeCode="6C2D9E4B-7A31-4F0E-9B6C-2F5A8E1D3C70" Scope="perUser">
    <MajorUpgrade DowngradeErrorMessage="A newer FormCraft is installed." />
    <MediaTemplate EmbedCab="yes" />
    <StandardDirectory Id="LocalAppDataFolder">
      <Directory Id="ProgramsDir" Name="Programs">
        <Directory Id="INSTALLFOLDER" Name="FormCraft">
{dir_xml}        </Directory>
      </Directory>
    </StandardDirectory>
    <StandardDirectory Id="ProgramMenuFolder">
      <Component Id="StartMenu" Guid="*">
        <Shortcut Id="FormCraftShortcut" Name="FormCraft" Target="[INSTALLFOLDER]formcraft.exe" WorkingDirectory="INSTALLFOLDER" />
        <RegistryValue Root="HKCU" Key="Software\FormCraft" Name="installed" Type="integer" Value="1" KeyPath="yes" />
      </Component>
    </StandardDirectory>
    <ComponentGroup Id="Files">
{comps}    </ComponentGroup>
    <Feature Id="Main">
      <ComponentGroupRef Id="Files" />
      <ComponentRef Id="StartMenu" />
    </Feature>
  </Package>
</Wix>
"#
    )
}

pub fn dist(args: &[String]) -> Result<(), String> {
    let skip_build = args.iter().any(|a| a == "--no-build");
    if !skip_build {
        cargo_step(&["build", "--release", "-p", "formcraft", "-p", "formcraft-cli"])?;
    }
    let release = root().join("target").join("release");
    let out = root().join("dist");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let files = payload(&release);
    for (name, src) in &files {
        if !src.is_file() {
            return Err(format!("missing {name}: {} (build the example add-ins first?)", src.display()));
        }
    }
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let archive = out.join(format!("FormCraft-{VERSION}-{os}-x64.zip"));
    zip_dir(&files, &archive)?;
    eprintln!("portable: {}", archive.display());
    if cfg!(windows) {
        let wxs = out.join("FormCraft.wxs");
        std::fs::write(&wxs, wix_source(&files)).map_err(|e| e.to_string())?;
        let msi = out.join(format!("FormCraft-{VERSION}-x64.msi"));
        let mut wix = Command::new("wix");
        wix.arg("build").arg(&wxs).arg("-o").arg(&msi);
        match run(wix, "wix build") {
            Ok(()) => eprintln!("msi: {}", msi.display()),
            Err(e) => eprintln!("msi skipped ({e}); install WiX: dotnet tool install --global wix"),
        }
    } else if cfg!(target_os = "linux") {
        let mut sh = Command::new("bash");
        sh.arg(root().join("packaging/linux/build-appimage.sh")).arg(&release).arg(&out).arg(VERSION);
        match run(sh, "build-appimage.sh") {
            Ok(()) => eprintln!("AppImage in {}", out.display()),
            Err(e) => eprintln!("AppImage skipped ({e}); needs linuxdeploy in PATH"),
        }
    }
    Ok(())
}
