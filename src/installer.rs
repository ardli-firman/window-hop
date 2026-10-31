//! Installer logic and packaging validation for Window Hop.

use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Kesalahan yang dapat terjadi pada proses instalasi atau validasi sistem.
#[derive(Debug, Error)]
pub enum InstallError {
    #[error("Platform {0} tidak didukung. Installer Window Hop memerlukan Windows.")]
    UnsupportedPlatform(String),

    #[error("Arsitektur OS {found} tidak didukung. Window Hop memerlukan Windows 64-bit (x64).")]
    UnsupportedArchitecture { found: String },

    #[error(
        "Versi OS Windows {found} tidak didukung. Window Hop memerlukan minimal {minimum} (Windows 10 build 19041+ atau Windows 11 x64)."
    )]
    UnsupportedOsVersion { found: String, minimum: String },

    #[error("Kesalahan I/O instalasi: {0}")]
    Io(#[from] std::io::Error),

    #[error("Payload executable instalasi tidak ditemukan pada: {0}")]
    MissingPayload(PathBuf),

    #[error("Verifikasi instalasi gagal: {0}")]
    VerificationFailed(String),

    #[error("Format manifest instalasi rusak: {0}")]
    CorruptManifest(String),
}

/// Arsitektur prosesor / OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X64,
    X86,
    Arm64,
    Other,
}

impl Arch {
    pub fn as_str(&self) -> &'static str {
        match self {
            Arch::X64 => "x64 (64-bit)",
            Arch::X86 => "x86 (32-bit)",
            Arch::Arm64 => "ARM64",
            Arch::Other => "Other",
        }
    }
}

/// Informasi versi dan arsitektur sistem operasi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsVersion {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
    pub arch: Arch,
}

/// Batas minimum build Windows yang divalidasi untuk Window Hop (Windows 10 2004 / Build 19041).
pub(crate) const MINIMUM_WINDOWS_BUILD: u32 = 19041;
pub const MINIMUM_WINDOWS_VERSION_STR: &str = "Windows 10 build 19041 (10.0.19041)";
pub const APPLICATION_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Memvalidasi kesesuaian sistem operasi terhadap baseline yang telah diverifikasi.
pub fn validate_os_baseline(os: &OsVersion) -> Result<(), InstallError> {
    if os.arch != Arch::X64 {
        return Err(InstallError::UnsupportedArchitecture {
            found: os.arch.as_str().to_string(),
        });
    }

    if os.major < 10 || (os.major == 10 && os.build < MINIMUM_WINDOWS_BUILD) {
        return Err(InstallError::UnsupportedOsVersion {
            found: format!("{}.{}.{}", os.major, os.minor, os.build),
            minimum: MINIMUM_WINDOWS_VERSION_STR.to_string(),
        });
    }

    Ok(())
}

/// Mengembalikan lokasi direktori instalasi konsisten default untuk Window Hop.
///
/// Pada platform Windows, ini mengarah ke `%LOCALAPPDATA%\Programs\Window Hop`.
/// Lokasi ini tidak memerlukan hak akses administrator, konsisten antar komputer perawat,
/// dan merupakan standar instalasi per-pengguna Windows modern.
pub fn default_install_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(local_appdata)
                .join("Programs")
                .join("Window Hop");
        }
        if let Ok(prog_files) = std::env::var("ProgramFiles") {
            return PathBuf::from(prog_files).join("Window Hop");
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(data_home).join("programs").join("window-hop");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("programs")
                .join("window-hop");
        }
    }

    PathBuf::from("Window Hop")
}

/// Mengembalikan lokasi direktori Start Menu untuk Window Hop.
pub fn start_menu_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return Some(
                PathBuf::from(appdata)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Start Menu")
                    .join("Programs")
                    .join("Window Hop"),
            );
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
            return Some(PathBuf::from(data_home).join("applications"));
        }
        if let Ok(home) = std::env::var("HOME") {
            return Some(
                PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join("applications"),
            );
        }
    }

    None
}

/// Mengembalikan lokasi direktori Desktop pengguna.
pub fn desktop_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(onedrive) = std::env::var_os("OneDrive") {
            let desktop = PathBuf::from(onedrive).join("Desktop");
            if desktop.exists() {
                return Some(desktop);
            }
        }
        if let Some(userprofile) = std::env::var_os("USERPROFILE") {
            let desktop = PathBuf::from(userprofile).join("Desktop");
            if desktop.exists() {
                return Some(desktop);
            }
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME") {
            let desktop = PathBuf::from(home).join("Desktop");
            if desktop.exists() {
                return Some(desktop);
            }
        }
    }

    None
}

/// Memeriksa apakah path yang diberikan berada di dalam direktori build pengembang (e.g. target/debug atau target/release).
pub fn is_dev_build_path(path: &Path) -> bool {
    let s = path.to_string_lossy().to_lowercase();
    s.contains("target/debug")
        || s.contains("target/release")
        || s.contains("target\\debug")
        || s.contains("target\\release")
}

/// Menghasilkan representasi string stempel waktu UTC ISO-8601 saat ini.
pub(crate) fn current_timestamp_iso8601() -> String {
    let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => return "1970-01-01T00:00:00Z".to_string(),
    };

    let days = now / 86400;
    let rem_secs = now % 86400;
    let hours = rem_secs / 3600;
    let mins = (rem_secs % 3600) / 60;
    let secs = rem_secs % 60;

    let z = (days as i64) + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1024 + doe / 1461 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{mins:02}:{secs:02}Z")
}

/// Mengambil path executable aplikasi yang terpasang atau sedang berjalan.
///
/// Aturan resolusi:
/// 1. Jika aplikasi sedang berjalan dari folder yang bukan folder build pengembang
///    (e.g. `%LOCALAPPDATA%\Programs\Window Hop` atau direktori instalasi kustom `--dir`),
///    gunakan executable tersebut secara langsung.
/// 2. Jika aplikasi sedang dipanggil dari folder build pengembang (`target/debug` atau `target/release`),
///    alihkan ke executable pada direktori instalasi resmi (default atau kustom jika diberikan)
///    agar shortcut dan pendaftaran startup tidak menunjuk ke folder build dev.
pub fn resolve_installed_app_executable(
    current_exe: Option<&Path>,
    install_dir: Option<&Path>,
) -> PathBuf {
    let current = current_exe
        .map(|p| p.to_path_buf())
        .or_else(|| std::env::current_exe().ok());

    if let Some(curr) = current {
        if !is_dev_build_path(&curr) {
            return curr;
        }
    }

    let target_dir = match install_dir {
        Some(d) => d.to_path_buf(),
        None => default_install_dir(),
    };
    target_dir.join("window-hop.exe")
}

/// Konfigurasi parameter eksekusi instalasi.
#[derive(Debug, Clone)]
pub struct InstallOptions {
    /// Direktori tujuan instalasi (default: `default_install_dir()`).
    pub install_dir: PathBuf,
    /// Path executable aplikasi sumber yang akan dipasang jika tidak menggunakan payload tersemat.
    pub source_exe: PathBuf,
    /// Payload biner aplikasi yang tersemat langsung dalam biner installer.
    pub embedded_payload: Option<&'static [u8]>,
    /// Apakah menyertakan dokumen panduan instalasi dan deskripsi dependensi GPUI.
    pub bundle_resources: bool,
    /// Apakah membuat shortcut Start Menu untuk Window Hop.
    pub create_start_menu_shortcut: bool,
    /// Apakah membuat shortcut Desktop untuk Window Hop.
    pub create_desktop_shortcut: bool,
    /// Apakah mendaftarkan uninstaller ke Windows Add/Remove Programs (Apps & Features).
    pub register_uninstaller: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            install_dir: default_install_dir(),
            source_exe: PathBuf::from("window-hop.exe"),
            embedded_payload: None,
            bundle_resources: true,
            create_start_menu_shortcut: false,
            create_desktop_shortcut: false,
            register_uninstaller: false,
        }
    }
}

/// Tanda bukti keberhasilan instalasi Window Hop.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstallReceipt {
    pub install_dir: PathBuf,
    pub installed_exe: PathBuf,
    pub installed_files: Vec<PathBuf>,
    pub manifest_path: PathBuf,
    pub version: String,
    #[serde(default)]
    pub shortcuts_created: Vec<PathBuf>,
}

/// Manifest informasi berkas instalasi.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstallManifest {
    pub app_name: String,
    pub version: String,
    pub installed_exe: PathBuf,
    pub installed_files: Vec<PathBuf>,
    pub installed_at: String,
    #[serde(default)]
    pub shortcuts_created: Vec<PathBuf>,
}

/// Tanda bukti keberhasilan pencopotan (uninstall) Window Hop.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UninstallReceipt {
    pub uninstalled_files: Vec<PathBuf>,
    pub cleaned_shortcuts: Vec<PathBuf>,
    pub success: bool,
}

/// Petunjuk pemasangan dan batas baseline OS/runtime yang diverifikasi.
pub(crate) const INSTALL_GUIDE_CONTENT: &str = r#"================================================================================
Window Hop - Petunjuk Pemasangan dan Batasan Sistem Terverifikasi
================================================================================

1. LINGKUNGAN SISTEM OPERASI TERVERIFIKASI
- Target OS: Windows 10 64-bit (Build 19041 / Version 2004 atau lebih baru) dan Windows 11 64-bit.
- Arsitektur OS: x64 (64-bit). OS 32-bit (x86) tidak didukung (ADR 0002).
- Hak akses: Instalasi per-pengguna (%LOCALAPPDATA%\Programs\Window Hop) tidak memerlukan hak administrator (UAC).

2. PERSYARATAN RUNTIME GPUI & GRAFIS
- Driver grafis dengan dukungan DirectX 11 / Direct3D 11 Feature Level 11_0 atau lebih baru.
- Layanan font sistem DirectWrite Windows aktif.
- Visual C++ Redistributable 2015-2022 x64 (biasanya sudah tersedia di Windows 10/11).
- Resource/asset UI dan font disertakan langsung (ter-embed via rust-embed) di dalam window-hop.exe.

3. BROWSER TERVERIFIKASI
Window Hop mendukung browser poliklinik 32-bit maupun 64-bit pada OS 64-bit:
- Microsoft Edge (Chromium)
- Google Chrome
- Mozilla Firefox
- Brave Browser
Setiap profil menjalankan sesi browser terisolasi (--user-data-dir atau -profile) tanpa mengganggu browser kerja perawat.

4. SHORTCUT & STARTUP
- Shortcut profil dibuat melalui UI Window Hop dan menunjuk ke direktori instalasi aplikasi resmi, bukan direktori build.
- Opsi startup otomatis mendaftarkan aplikasi di HKCU:\Software\Microsoft\Windows\CurrentVersion\Run dengan flag --startup.
"#;

#[cfg(windows)]
fn create_windows_shortcut(
    shortcut_path: &Path,
    target_exe: &Path,
    args: &str,
) -> Result<(), InstallError> {
    if let Some(parent) = shortcut_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let workdir = target_exe.parent().unwrap_or(shortcut_path);
    let ps_script = "& { param($lnk, $target, $arg, $dir) $ws = New-Object -ComObject WScript.Shell; $s = $ws.CreateShortcut($lnk); $s.TargetPath = $target; $s.Arguments = $arg; $s.WorkingDirectory = $dir; $s.Save() }";

    let output = std::process::Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(ps_script)
        .arg(shortcut_path.to_string_lossy().as_ref())
        .arg(target_exe.to_string_lossy().as_ref())
        .arg(args)
        .arg(workdir.to_string_lossy().as_ref())
        .output()
        .map_err(|e| InstallError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(InstallError::VerificationFailed(format!(
            "Gagal membuat shortcut: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

#[cfg(windows)]
fn register_windows_uninstaller(
    install_dir: &Path,
    installed_exe: &Path,
) -> Result<(), InstallError> {
    let uninstaller_exe = install_dir.join("uninstall.exe");
    let uninstall_cmd = if uninstaller_exe.exists() {
        format!("\"{}\"", uninstaller_exe.to_string_lossy())
    } else {
        format!("\"{}\" --uninstall", installed_exe.to_string_lossy())
    };
    let quiet_cmd = format!("{} --silent", uninstall_cmd);
    let display_icon = format!("\"{}\",0", installed_exe.to_string_lossy());
    let version = APPLICATION_VERSION;
    let install_dir_str = install_dir.to_string_lossy();

    let ps_script = r#"& {
        param($dir, $icon, $cmd, $qcmd, $ver)
        $regPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop'
        if (-not (Test-Path $regPath)) { New-Item -Path $regPath -Force | Out-Null }
        Set-ItemProperty -Path $regPath -Name 'DisplayName' -Value 'Window Hop'
        Set-ItemProperty -Path $regPath -Name 'DisplayVersion' -Value $ver
        Set-ItemProperty -Path $regPath -Name 'Publisher' -Value 'Window Hop Contributors'
        Set-ItemProperty -Path $regPath -Name 'InstallLocation' -Value $dir
        Set-ItemProperty -Path $regPath -Name 'DisplayIcon' -Value $icon
        Set-ItemProperty -Path $regPath -Name 'UninstallString' -Value $cmd
        Set-ItemProperty -Path $regPath -Name 'QuietUninstallString' -Value $qcmd
        Set-ItemProperty -Path $regPath -Name 'NoModify' -Value 1 -Type DWord
        Set-ItemProperty -Path $regPath -Name 'NoRepair' -Value 1 -Type DWord
    }"#;

    let output = std::process::Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(ps_script)
        .arg(install_dir_str.as_ref())
        .arg(&display_icon)
        .arg(&uninstall_cmd)
        .arg(&quiet_cmd)
        .arg(version)
        .output()
        .map_err(|e| InstallError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(InstallError::VerificationFailed(format!(
            "Gagal mendaftarkan uninstaller Windows: {}",
            stderr.trim()
        )));
    }

    Ok(())
}

#[cfg(windows)]
fn unregister_windows_uninstaller() -> Result<(), InstallError> {
    let ps_script = r#"& {
        $regPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop'
        if (Test-Path $regPath) { Remove-Item -Path $regPath -Recurse -Force -ErrorAction SilentlyContinue }
    }"#;
    let _ = std::process::Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(ps_script)
        .output();
    Ok(())
}

#[cfg(windows)]
fn unregister_windows_startup() -> Result<(), InstallError> {
    let ps_script = r#"& {
        $regPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
        Remove-ItemProperty -Path $regPath -Name 'WindowHop' -ErrorAction SilentlyContinue
    }"#;
    let _ = std::process::Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(ps_script)
        .output();
    Ok(())
}

/// Menjalankan instalasi aplikasi Window Hop ke lokasi yang konsisten.
pub fn install(options: &InstallOptions) -> Result<InstallReceipt, InstallError> {
    fs::create_dir_all(&options.install_dir)?;

    let installed_exe = options.install_dir.join("window-hop.exe");

    if let Some(payload_bytes) = options.embedded_payload {
        fs::write(&installed_exe, payload_bytes)?;
    } else if options.source_exe.exists() {
        fs::copy(&options.source_exe, &installed_exe)?;
    } else {
        return Err(InstallError::MissingPayload(options.source_exe.clone()));
    }

    let mut installed_files = vec![installed_exe.clone()];

    if options.bundle_resources {
        let guide_path = options.install_dir.join("INSTALL-GUIDE.txt");
        fs::write(&guide_path, INSTALL_GUIDE_CONTENT)?;
        installed_files.push(guide_path);
    }

    #[allow(unused_mut)]
    let mut shortcuts_created = Vec::new();

    #[cfg(windows)]
    {
        if options.create_start_menu_shortcut {
            if let Some(start_dir) = start_menu_dir() {
                let shortcut_path = start_dir.join("Window Hop.lnk");
                if let Ok(()) = create_windows_shortcut(&shortcut_path, &installed_exe, "") {
                    shortcuts_created.push(shortcut_path);
                }
            }
        }

        if options.create_desktop_shortcut {
            if let Some(desktop) = desktop_dir() {
                let shortcut_path = desktop.join("Window Hop.lnk");
                if let Ok(()) = create_windows_shortcut(&shortcut_path, &installed_exe, "") {
                    shortcuts_created.push(shortcut_path);
                }
            }
        }

        if options.register_uninstaller {
            let _ = register_windows_uninstaller(&options.install_dir, &installed_exe);
        }
    }

    let manifest_path = options.install_dir.join("install-manifest.json");
    let manifest = InstallManifest {
        app_name: "Window Hop".to_string(),
        version: APPLICATION_VERSION.to_string(),
        installed_exe: installed_exe.clone(),
        installed_files: installed_files.clone(),
        installed_at: current_timestamp_iso8601(),
        shortcuts_created: shortcuts_created.clone(),
    };

    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| InstallError::VerificationFailed(e.to_string()))?;
    fs::write(&manifest_path, manifest_json)?;
    installed_files.push(manifest_path.clone());

    let mut receipt = verify_installation(&options.install_dir)?;
    receipt.shortcuts_created = shortcuts_created;
    Ok(receipt)
}

/// Memverifikasi integritas berkas instalasi Window Hop pada direktori yang ditentukan.
pub fn verify_installation(install_dir: &Path) -> Result<InstallReceipt, InstallError> {
    let installed_exe = install_dir.join("window-hop.exe");
    if !installed_exe.exists() {
        return Err(InstallError::VerificationFailed(
            "Executable tidak ditemukan pada direktori instalasi".to_string(),
        ));
    }

    let manifest_path = install_dir.join("install-manifest.json");
    if !manifest_path.exists() {
        return Err(InstallError::VerificationFailed(
            "Manifest instalasi install-manifest.json tidak ditemukan".to_string(),
        ));
    }

    let manifest_content = fs::read_to_string(&manifest_path)?;
    let manifest: InstallManifest = serde_json::from_str(&manifest_content)
        .map_err(|e| InstallError::CorruptManifest(e.to_string()))?;

    Ok(InstallReceipt {
        install_dir: install_dir.to_path_buf(),
        installed_exe,
        installed_files: manifest.installed_files,
        manifest_path,
        version: manifest.version,
        shortcuts_created: manifest.shortcuts_created,
    })
}

/// Menghapus dan mencopot instalasi Window Hop beserta shortcut dan pendaftaran sistemnya.
pub fn uninstall(install_dir: &Path) -> Result<UninstallReceipt, InstallError> {
    let mut uninstalled_files = Vec::new();
    let mut cleaned_shortcuts = Vec::new();

    // 1. Unregister Windows Add/Remove Programs dan Run startup key jika di platform Windows
    #[cfg(windows)]
    {
        let _ = unregister_windows_uninstaller();
        let _ = unregister_windows_startup();
    }

    // 2. Baca install-manifest.json jika ada untuk membersihkan file dan shortcut yang tercatat
    let manifest_path = install_dir.join("install-manifest.json");
    if manifest_path.exists() {
        if let Ok(content) = fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<InstallManifest>(&content) {
                for sc in manifest.shortcuts_created {
                    if sc.exists() {
                        let _ = fs::remove_file(&sc);
                        cleaned_shortcuts.push(sc);
                    }
                }
                for f in manifest.installed_files {
                    if f.exists() {
                        let _ = fs::remove_file(&f);
                        uninstalled_files.push(f);
                    }
                }
            }
        }
    }

    // 3. Bersihkan shortcut Start Menu & Desktop default jika masih tersisa
    #[cfg(windows)]
    {
        if let Some(start_dir) = start_menu_dir() {
            let sc = start_dir.join("Window Hop.lnk");
            if sc.exists() {
                let _ = fs::remove_file(&sc);
                cleaned_shortcuts.push(sc);
            }
            let _ = fs::remove_dir(&start_dir);
        }
        if let Some(desktop) = desktop_dir() {
            let sc = desktop.join("Window Hop.lnk");
            if sc.exists() {
                let _ = fs::remove_file(&sc);
                cleaned_shortcuts.push(sc);
            }
        }
    }

    // 4. Bersihkan file-file standar di folder instalasi jika belum terhapus
    let known_files = [
        "window-hop.exe",
        "INSTALL-GUIDE.txt",
        "install-manifest.json",
        "uninstall.exe",
        "window-hop-installer.exe",
    ];

    for name in known_files {
        let p = install_dir.join(name);
        if p.exists() {
            let _ = fs::remove_file(&p);
            uninstalled_files.push(p);
        }
    }

    // 5. Hapus folder instalasi jika kosong
    let _ = fs::remove_dir(install_dir);

    Ok(UninstallReceipt {
        uninstalled_files,
        cleaned_shortcuts,
        success: true,
    })
}
