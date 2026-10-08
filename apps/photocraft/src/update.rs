//! GitHub Releases updater for native desktop installs.

use photocraft_update::{newer_than, sha256, verify_manifest};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const REPO: &str = "x-Pepsy/photocraft";
const PUBLIC_KEY: &str = match option_env!("PHOTOCRAFT_UPDATE_PUBLIC_KEY") {
    Some(key) => key,
    None => "",
};

pub fn installer() -> Box<dyn FnMut() -> Result<String, String>> {
    Box::new(|| install_latest().map_err(|e| e.to_string()))
}

fn install_latest() -> Result<String, UpdateError> {
    let key = if PUBLIC_KEY.is_empty() { return Err(UpdateError::NotConfigured) } else { PUBLIC_KEY };
    let temp = temp_dir()?;
    let manifest_path = temp.join("update-manifest.json");
    let signature_path = temp.join("update-manifest.json.sig");
    let manifest_url = format!("https://github.com/{REPO}/releases/latest/download/update-manifest.json");
    download(&manifest_url, &manifest_path)?;
    let release_url = format!("https://github.com/{REPO}/releases/latest/download/update-manifest.json.sig");
    download(&release_url, &signature_path)?;
    let manifest_bytes = fs::read(&manifest_path)?;
    let signature = fs::read_to_string(&signature_path)?;
    let manifest = verify_manifest(&manifest_bytes, signature.trim(), key).map_err(|e| UpdateError::Manifest(e.to_string()))?;
    let current = photocraft_engine::build_info::VERSION;
    if !newer_than(&manifest.version, current).map_err(|e| UpdateError::Manifest(e.to_string()))? {
        return Ok(format!("PhotoCraft {current} is up to date"));
    }
    let (platform, arch, kind) = target_asset()?;
    let asset = manifest.asset(platform, arch, kind).map_err(|e| UpdateError::Manifest(e.to_string()))?;
    let asset_path = temp.join(&asset.file);
    download(&format!("https://github.com/{REPO}/releases/latest/download/{}", asset.file), &asset_path)?;
    let bytes = fs::read(&asset_path)?;
    if sha256(&bytes) != asset.sha256.to_ascii_lowercase() {
        return Err(UpdateError::Checksum);
    }
    launch_or_replace(&asset_path, kind)?;
    Ok(format!("PhotoCraft {} downloaded; installation will continue after closing the app", manifest.version))
}

fn download(url: &str, destination: &Path) -> Result<(), UpdateError> {
    let status = Command::new("curl")
        .args(["--fail", "--location", "--silent", "--show-error", "--retry", "2", "--user-agent", "PhotoCraft-updater", "--output"])
        .arg(destination)
        .arg(url)
        .status()?;
    if status.success() { Ok(()) } else { Err(UpdateError::Download(url.to_string())) }
}

fn temp_dir() -> Result<PathBuf, UpdateError> {
    let dir = std::env::temp_dir().join(format!("photocraft-update-{}", std::process::id()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn target_asset() -> Result<(&'static str, &'static str, &'static str), UpdateError> {
    #[cfg(target_os = "windows")]
    return Ok((
        "windows",
        if cfg!(target_arch = "x86_64") {
            "x86_64"
        } else if cfg!(target_arch = "x86") {
            "x86"
        } else {
            "aarch64"
        },
        if portable_install() { "portable" } else { "msi" },
    ));
    #[cfg(target_os = "linux")]
    return Ok(("linux", if cfg!(target_arch = "x86_64") { "x86_64" } else { "aarch64" }, "appimage"));
    #[cfg(target_os = "macos")]
    return Ok(("macos", "universal", "dmg"));
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    Err(UpdateError::Manifest("automatic updates are unsupported on this platform".into()))
}

#[cfg(target_os = "windows")]
fn portable_install() -> bool {
    std::env::current_exe().ok().and_then(|p| p.parent().map(crate::app_dirs::portable_marker)).flatten().is_some()
}

fn launch_or_replace(asset: &Path, kind: &str) -> Result<(), UpdateError> {
    if kind == "msi" || kind == "dmg" {
        let path = asset.to_string_lossy().into_owned();
        if kind == "msi" {
            Command::new("cmd").args(["/C", "start", "", &path]).spawn()?;
        } else {
            Command::new("open").arg(path).spawn()?;
        }
        std::process::exit(0);
    }
    let exe = std::env::current_exe()?;
    let asset_arg = asset.to_string_lossy().into_owned();
    let exe_arg = exe.to_string_lossy().into_owned();
    if cfg!(target_os = "windows") {
        let helper = temp_dir()?.join("photocraft-update-helper.ps1");
        fs::write(&helper, WINDOWS_HELPER.as_bytes())?;
        Command::new("powershell").args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(helper).args([&asset_arg, &exe_arg]).spawn()?;
    } else {
        let helper = temp_dir()?.join("photocraft-update-helper.sh");
        fs::write(&helper, HELPER.as_bytes())?;
        Command::new("sh").arg(helper).args([&asset_arg, &exe_arg]).spawn()?;
    }
    std::process::exit(0)
}

const HELPER: &str =
    "#!/bin/sh\nset -eu\nasset=$1\nexe=$2\nsleep 1\ncase \"$asset\" in *.AppImage) cp \"$asset\" \"$exe\"; chmod +x \"$exe\";; esac\nexec \"$exe\"\n";
const WINDOWS_HELPER: &str = "$asset = $args[0]\n$exe = $args[1]\nStart-Sleep -Seconds 2\n$tmp = Join-Path ([IO.Path]::GetTempPath()) ('photocraft-update-' + [Guid]::NewGuid())\nExpand-Archive -LiteralPath $asset -DestinationPath $tmp -Force\nCopy-Item (Join-Path $tmp 'photocraft.exe') $exe -Force\nStart-Process $exe\n";

#[derive(Debug)]
enum UpdateError {
    NotConfigured,
    Download(String),
    Manifest(String),
    Checksum,
    Io(std::io::Error),
}
impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "OTA signing public key is not configured in this build"),
            Self::Download(url) => write!(f, "download failed: {url}"),
            Self::Manifest(e) => write!(f, "invalid update metadata: {e}"),
            Self::Checksum => write!(f, "download checksum mismatch"),
            Self::Io(e) => write!(f, "update I/O failed: {e}"),
        }
    }
}
impl std::error::Error for UpdateError {}
impl From<std::io::Error> for UpdateError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
