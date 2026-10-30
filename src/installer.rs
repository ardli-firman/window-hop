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
    /// Path executable aplikasi sumber yang akan dipasang.
    pub source_exe: PathBuf,
    /// Apakah menyertakan dokumen panduan instalasi dan deskripsi dependensi GPUI.
    pub bundle_resources: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            install_dir: default_install_dir(),
            source_exe: PathBuf::from("window-hop.exe"),
            bundle_resources: true,
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
}

/// Manifest informasi berkas instalasi.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InstallManifest {
    pub app_name: String,
    pub version: String,
    pub installed_exe: PathBuf,
    pub installed_files: Vec<PathBuf>,
    pub installed_at: String,
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

/// Menjalankan instalasi aplikasi Window Hop ke lokasi yang konsisten.
pub fn install(options: &InstallOptions) -> Result<InstallReceipt, InstallError> {
    if !options.source_exe.exists() {
        return Err(InstallError::MissingPayload(options.source_exe.clone()));
    }

    fs::create_dir_all(&options.install_dir)?;

    let installed_exe = options.install_dir.join("window-hop.exe");
    fs::copy(&options.source_exe, &installed_exe)?;

    let mut installed_files = vec![installed_exe.clone()];

    if options.bundle_resources {
        let guide_path = options.install_dir.join("INSTALL-GUIDE.txt");
        fs::write(&guide_path, INSTALL_GUIDE_CONTENT)?;
        installed_files.push(guide_path);
    }

    let manifest_path = options.install_dir.join("install-manifest.json");
    let manifest = InstallManifest {
        app_name: "Window Hop".to_string(),
        version: APPLICATION_VERSION.to_string(),
        installed_exe: installed_exe.clone(),
        installed_files: installed_files.clone(),
        installed_at: current_timestamp_iso8601(),
    };

    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| InstallError::VerificationFailed(e.to_string()))?;
    fs::write(&manifest_path, manifest_json)?;
    installed_files.push(manifest_path.clone());

    verify_installation(&options.install_dir)
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
    })
}
