//! Browser capabilities, isolation verification, and launch planning.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Kesalahan terkait browser dan parameter peluncuran.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum BrowserError {
    #[error("Argumen berpetik tidak ditutup dengan benar: {0}")]
    UnterminatedQuote(String),

    #[error("Parameter '{arg}' ditolak karena berbenturan dengan isolasi sesi browser profil ({reason})")]
    ConflictingIsolationParameter { arg: String, reason: String },

    #[error("Browser '{0}' belum diverifikasi untuk isolasi sesi atau kepemilikan proses")]
    UnverifiedBrowser(String),
}

/// Parsing string parameter baris perintah kustom mengikuti aturan quoting Windows (CommandLineToArgvW semantics).
pub fn parse_windows_command_line(input: &str) -> Result<Vec<String>, BrowserError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let mut args = Vec::new();
    let mut current_arg = String::new();
    let mut in_quotes = false;
    let mut backslash_count = 0;
    let mut has_token = false;

    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        if ch == '\\' {
            backslash_count += 1;
            has_token = true;
            i += 1;
            continue;
        }

        if ch == '"' {
            let half = backslash_count / 2;
            for _ in 0..half {
                current_arg.push('\\');
            }

            if backslash_count % 2 == 1 {
                current_arg.push('"');
            } else {
                in_quotes = !in_quotes;
            }
            backslash_count = 0;
            has_token = true;
            i += 1;
            continue;
        }

        if backslash_count > 0 {
            for _ in 0..backslash_count {
                current_arg.push('\\');
            }
            backslash_count = 0;
        }

        if ch.is_whitespace() && !in_quotes {
            if has_token {
                args.push(current_arg);
                current_arg = String::new();
                has_token = false;
            }
        } else {
            current_arg.push(ch);
            has_token = true;
        }

        i += 1;
    }

    if backslash_count > 0 {
        for _ in 0..backslash_count {
            current_arg.push('\\');
        }
    }

    if in_quotes {
        return Err(BrowserError::UnterminatedQuote(input.to_string()));
    }

    if has_token {
        args.push(current_arg);
    }

    Ok(args)
}

/// Catatan retensi login/sesi berdasarkan mode browser yang dipilih.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginRetentionNote {
    /// Sesi tersimpan terisolasi di direktori data profil.
    SessionIsolated { explanation: String },
    /// Mode Kiosk atau InPrivate tidak mempertahankan sesi login persisten.
    KioskOrInPrivateNoRetention { explanation: String },
}

impl LoginRetentionNote {
    /// Mengambil teks penjelasan mengenai retensi login.
    pub fn explanation(&self) -> &str {
        match self {
            Self::SessionIsolated { explanation } => explanation,
            Self::KioskOrInPrivateNoRetention { explanation } => explanation,
        }
    }
}

/// Rencana peluncuran browser dengan argumen terstruktur dan isolasi sesi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub user_data_dir: PathBuf,
    pub login_retention: LoginRetentionNote,
}

/// Adapter kemampuan browser untuk Microsoft Edge (Chromium).
///
/// Mengelola parameter isolasi sesi tersendiri (`--user-data-dir`) dan memvalidasi
/// bahwa parameter kustom tidak berbenturan dengan isolasi profil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EdgeAdapter;

impl EdgeAdapter {
    const CONFLICTING_PREFIXES: &'static [(&'static str, &'static str)] = &[
        (
            "--user-data-dir",
            "Window Hop secara otomatis mengelola direktori data terisolasi untuk setiap profil",
        ),
        (
            "--profile-directory",
            "Window Hop menggunakan profil sesi browser tersendiri; parameter profil internal dilarang",
        ),
    ];

    pub fn new() -> Self {
        Self
    }

    pub fn browser_name(&self) -> &'static str {
        "Microsoft Edge"
    }

    /// Memvalidasi parameter kustom terhadap parameter isolasi sesi yang dicadangkan.
    pub fn validate_custom_parameters(
        &self,
        custom_parameters: &str,
    ) -> Result<Vec<String>, BrowserError> {
        let parsed = parse_windows_command_line(custom_parameters)?;
        self.validate_parsed_arguments(&parsed)?;
        Ok(parsed)
    }

    /// Memvalidasi daftar argumen yang telah diparsing.
    pub fn validate_parsed_arguments<S: AsRef<str>>(&self, args: &[S]) -> Result<(), BrowserError> {
        for arg in args {
            let trimmed = arg.as_ref().trim();
            let lower = trimmed.to_ascii_lowercase();
            let normalized = lower
                .strip_prefix("--")
                .or_else(|| lower.strip_prefix('-'))
                .or_else(|| lower.strip_prefix('/'));
            if let Some(arg_body) = normalized {
                for &(prefix, reason) in Self::CONFLICTING_PREFIXES {
                    let clean_prefix = prefix.trim_start_matches('-');
                    if arg_body == clean_prefix
                        || arg_body.starts_with(&format!("{}=", clean_prefix))
                        || arg_body.starts_with(&format!("{}:", clean_prefix))
                    {
                        return Err(BrowserError::ConflictingIsolationParameter {
                            arg: trimmed.to_string(),
                            reason: reason.to_string(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Menentukan catatan retensi sesi login berdasarkan argumen yang digunakan.
    pub fn evaluate_login_retention(&self, parsed_args: &[String]) -> LoginRetentionNote {
        let is_kiosk_or_inprivate = parsed_args.iter().any(|arg| {
            let lower = arg.to_ascii_lowercase();
            lower == "--kiosk"
                || lower.starts_with("--kiosk=")
                || lower == "--inprivate"
                || lower == "-inprivate"
        });

        if is_kiosk_or_inprivate {
            LoginRetentionNote::KioskOrInPrivateNoRetention {
                explanation: "Mode kiosk atau InPrivate pada Microsoft Edge tidak mempertahankan login persisten setelah jendela ditutup.".to_string(),
            }
        } else {
            LoginRetentionNote::SessionIsolated {
                explanation: "Login dan cookie disimpan tersendiri dalam sesi browser profil, terpisah dari browser kerja perawat.".to_string(),
            }
        }
    }

    /// Membangun rencana peluncuran browser dengan batas isolasi direktori data profil.
    pub fn build_launch_plan(
        &self,
        executable: &Path,
        profile_data_dir: &Path,
        custom_parameters: &str,
    ) -> Result<LaunchPlan, BrowserError> {
        let parsed_custom = self.validate_custom_parameters(custom_parameters)?;
        let login_retention = self.evaluate_login_retention(&parsed_custom);

        let mut arguments = Vec::new();
        // Parameter isolasi sesi Chromium / Edge
        arguments.push(format!("--user-data-dir={}", profile_data_dir.display()));
        // Tambahkan parameter kustom pengguna
        arguments.extend(parsed_custom);

        Ok(LaunchPlan {
            executable: executable.to_path_buf(),
            arguments,
            user_data_dir: profile_data_dir.to_path_buf(),
            login_retention,
        })
    }
}

/// Adapter kemampuan browser untuk Google Chrome (Chromium).
///
/// Mengelola parameter isolasi sesi tersendiri (`--user-data-dir`) dan memvalidasi
/// bahwa parameter kustom tidak berbenturan dengan isolasi profil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChromeAdapter;

impl ChromeAdapter {
    const CONFLICTING_PREFIXES: &'static [(&'static str, &'static str)] = &[
        (
            "--user-data-dir",
            "Window Hop secara otomatis mengelola direktori data terisolasi untuk setiap profil",
        ),
        (
            "--profile-directory",
            "Window Hop menggunakan profil sesi browser tersendiri; parameter profil internal dilarang",
        ),
    ];

    pub fn new() -> Self {
        Self
    }

    pub fn browser_name(&self) -> &'static str {
        "Google Chrome"
    }

    /// Memvalidasi parameter kustom terhadap parameter isolasi sesi yang dicadangkan.
    pub fn validate_custom_parameters(
        &self,
        custom_parameters: &str,
    ) -> Result<Vec<String>, BrowserError> {
        let parsed = parse_windows_command_line(custom_parameters)?;
        self.validate_parsed_arguments(&parsed)?;
        Ok(parsed)
    }

    /// Memvalidasi daftar argumen yang telah diparsing.
    pub fn validate_parsed_arguments<S: AsRef<str>>(&self, args: &[S]) -> Result<(), BrowserError> {
        for arg in args {
            let trimmed = arg.as_ref().trim();
            let lower = trimmed.to_ascii_lowercase();
            let normalized = lower
                .strip_prefix("--")
                .or_else(|| lower.strip_prefix('-'))
                .or_else(|| lower.strip_prefix('/'));
            if let Some(arg_body) = normalized {
                for &(prefix, reason) in Self::CONFLICTING_PREFIXES {
                    let clean_prefix = prefix.trim_start_matches('-');
                    if arg_body == clean_prefix
                        || arg_body.starts_with(&format!("{}=", clean_prefix))
                        || arg_body.starts_with(&format!("{}:", clean_prefix))
                    {
                        return Err(BrowserError::ConflictingIsolationParameter {
                            arg: trimmed.to_string(),
                            reason: reason.to_string(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Menentukan catatan retensi sesi login berdasarkan argumen yang digunakan.
    pub fn evaluate_login_retention(&self, parsed_args: &[String]) -> LoginRetentionNote {
        let is_kiosk_or_incognito = parsed_args.iter().any(|arg| {
            let lower = arg.to_ascii_lowercase();
            lower == "--kiosk"
                || lower.starts_with("--kiosk=")
                || lower == "--incognito"
                || lower == "-incognito"
                || lower == "/incognito"
        });

        if is_kiosk_or_incognito {
            LoginRetentionNote::KioskOrInPrivateNoRetention {
                explanation: "Mode kiosk atau incognito pada Google Chrome tidak mempertahankan login persisten setelah jendela ditutup.".to_string(),
            }
        } else {
            LoginRetentionNote::SessionIsolated {
                explanation: "Login dan cookie disimpan tersendiri dalam sesi browser profil, terpisah dari browser kerja perawat.".to_string(),
            }
        }
    }

    /// Membangun rencana peluncuran browser dengan batas isolasi direktori data profil.
    pub fn build_launch_plan(
        &self,
        executable: &Path,
        profile_data_dir: &Path,
        custom_parameters: &str,
    ) -> Result<LaunchPlan, BrowserError> {
        let parsed_custom = self.validate_custom_parameters(custom_parameters)?;
        let login_retention = self.evaluate_login_retention(&parsed_custom);

        let mut arguments = Vec::new();
        // Parameter isolasi sesi Chromium / Chrome
        arguments.push(format!("--user-data-dir={}", profile_data_dir.display()));
        // Tambahkan parameter kustom pengguna
        arguments.extend(parsed_custom);

        Ok(LaunchPlan {
            executable: executable.to_path_buf(),
            arguments,
            user_data_dir: profile_data_dir.to_path_buf(),
            login_retention,
        })
    }
}

/// Adapter kemampuan browser terverifikasi (Edge atau Chrome).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserAdapter {
    Edge(EdgeAdapter),
    Chrome(ChromeAdapter),
}

impl BrowserAdapter {
    pub fn browser_name(&self) -> &'static str {
        match self {
            Self::Edge(a) => a.browser_name(),
            Self::Chrome(a) => a.browser_name(),
        }
    }

    pub fn validate_custom_parameters(
        &self,
        custom_parameters: &str,
    ) -> Result<Vec<String>, BrowserError> {
        match self {
            Self::Edge(a) => a.validate_custom_parameters(custom_parameters),
            Self::Chrome(a) => a.validate_custom_parameters(custom_parameters),
        }
    }

    pub fn validate_parsed_arguments<S: AsRef<str>>(&self, args: &[S]) -> Result<(), BrowserError> {
        match self {
            Self::Edge(a) => a.validate_parsed_arguments(args),
            Self::Chrome(a) => a.validate_parsed_arguments(args),
        }
    }

    pub fn evaluate_login_retention(&self, parsed_args: &[String]) -> LoginRetentionNote {
        match self {
            Self::Edge(a) => a.evaluate_login_retention(parsed_args),
            Self::Chrome(a) => a.evaluate_login_retention(parsed_args),
        }
    }

    pub fn build_launch_plan(
        &self,
        executable: &Path,
        profile_data_dir: &Path,
        custom_parameters: &str,
    ) -> Result<LaunchPlan, BrowserError> {
        match self {
            Self::Edge(a) => a.build_launch_plan(executable, profile_data_dir, custom_parameters),
            Self::Chrome(a) => a.build_launch_plan(executable, profile_data_dir, custom_parameters),
        }
    }
}

impl From<EdgeAdapter> for BrowserAdapter {
    fn from(adapter: EdgeAdapter) -> Self {
        Self::Edge(adapter)
    }
}

impl From<ChromeAdapter> for BrowserAdapter {
    fn from(adapter: ChromeAdapter) -> Self {
        Self::Chrome(adapter)
    }
}

/// Arsitektur biner executable browser (x86, x64, atau tidak diketahui).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserArchitecture {
    X86,
    X64,
    Unknown,
}

impl fmt::Display for BrowserArchitecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::X86 => write!(f, "x86 (32-bit)"),
            Self::X64 => write!(f, "x64 (64-bit)"),
            Self::Unknown => write!(f, "Tidak Diketahui"),
        }
    }
}

/// Membaca arsitektur biner dari file executable PE Windows (.exe).
///
/// Memeriksa DOS MZ header dan PE Machine header tanpa ketergantungan OS runtime khusus.
/// Mengembalikan `BrowserArchitecture::Unknown` jika file tidak dapat dibaca atau bukan PE valid.
pub fn read_executable_architecture(path: &Path) -> BrowserArchitecture {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return BrowserArchitecture::Unknown,
    };

    let mut dos_header = [0u8; 64];
    if file.read_exact(&mut dos_header).is_err() {
        return BrowserArchitecture::Unknown;
    }

    // Periksa MZ signature (0x5A4D)
    if dos_header[0] != b'M' || dos_header[1] != b'Z' {
        return BrowserArchitecture::Unknown;
    }

    // Offset e_lfanew berada pada offset 0x3C (4 bytes little-endian)
    let e_lfanew = u32::from_le_bytes([
        dos_header[0x3C],
        dos_header[0x3D],
        dos_header[0x3E],
        dos_header[0x3F],
    ]);

    if file.seek(SeekFrom::Start(e_lfanew as u64)).is_err() {
        return BrowserArchitecture::Unknown;
    }

    let mut pe_header = [0u8; 6];
    if file.read_exact(&mut pe_header).is_err() {
        return BrowserArchitecture::Unknown;
    }

    // Periksa PE signature: 'PE\0\0'
    if pe_header[0] != b'P' || pe_header[1] != b'E' || pe_header[2] != 0 || pe_header[3] != 0 {
        return BrowserArchitecture::Unknown;
    }

    // Machine architecture pada offset pe_header + 4 (2 bytes little-endian)
    let machine = u16::from_le_bytes([pe_header[4], pe_header[5]]);
    match machine {
        0x014c => BrowserArchitecture::X86, // IMAGE_FILE_MACHINE_I386
        0x8664 => BrowserArchitecture::X64, // IMAGE_FILE_MACHINE_AMD64
        _ => BrowserArchitecture::Unknown,
    }
}

/// Bukti dan rincian dukungan kompatibilitas browser secara jujur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSupportEvidence {
    pub browser_name: String,
    pub version: Option<String>,
    pub architecture: BrowserArchitecture,
    pub is_safe_to_activate: bool,
    pub evidence_text: String,
    pub limitations: Option<String>,
}

/// Deskripsi browser yang terdeteksi terpasang pada sistem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectedBrowser {
    pub name: String,
    pub executable_path: PathBuf,
    pub version: Option<String>,
    pub architecture: BrowserArchitecture,
}

/// Status evaluasi kompatibilitas browser yang komprehensif.
///
/// Membedakan:
/// 1. `SafeToActivate`: Executable terverifikasi untuk isolasi sesi dan kepemilikan instance (misal Microsoft Edge, Google Chrome).
/// 2. `Unverified`: Executable ditemukan pada disk tetapi belum diverifikasi; profil boleh disimpan namun gate aktivasi diblokir.
/// 3. `Unavailable`: Executable tidak ditemukan atau tidak dapat diakses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserCompatibilityStatus {
    SafeToActivate {
        adapter: BrowserAdapter,
        evidence: BrowserSupportEvidence,
    },
    Unverified {
        evidence: BrowserSupportEvidence,
    },
    Unavailable {
        path: PathBuf,
        reason: String,
    },
}

impl BrowserCompatibilityStatus {
    /// Apakah browser aman diaktifkan untuk meluncurkan profil.
    pub fn is_safe_to_activate(&self) -> bool {
        matches!(self, Self::SafeToActivate { .. })
    }

    /// Mengambil adapter browser jika terverifikasi.
    pub fn adapter(&self) -> Option<BrowserAdapter> {
        match self {
            Self::SafeToActivate { adapter, .. } => Some(*adapter),
            _ => None,
        }
    }

    /// Mengambil informasi bukti dukungan kompatibilitas jika executable tersedia.
    pub fn evidence(&self) -> Option<&BrowserSupportEvidence> {
        match self {
            Self::SafeToActivate { evidence, .. } => Some(evidence),
            Self::Unverified { evidence } => Some(evidence),
            Self::Unavailable { .. } => None,
        }
    }
}

/// Mengambil nama file executable dalam huruf kecil dari path (mendukung pemisah Windows '\\' maupun Unix '/').
pub fn extract_executable_filename(executable: &Path) -> String {
    let path_str = executable.to_string_lossy();
    path_str
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Memeriksa apakah file executable adalah Microsoft Edge.
pub fn is_edge_executable(executable: &Path) -> bool {
    let file_name = extract_executable_filename(executable);
    file_name == "msedge.exe" || file_name == "msedge"
}

/// Memeriksa apakah file executable adalah Google Chrome.
pub fn is_chrome_executable(executable: &Path) -> bool {
    let file_name = extract_executable_filename(executable);
    file_name == "chrome.exe" || file_name == "chrome"
}

/// Mencoba membaca nomor versi browser dari direktori instalasi terkait jika tersedia.
pub fn read_installed_browser_version(executable: &Path) -> Option<String> {
    let parent = executable.parent()?;
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    let dir_name = entry.file_name().to_string_lossy().to_string();
                    if let Some(first_char) = dir_name.chars().next() {
                        if first_char.is_ascii_digit() && dir_name.contains('.') {
                            return Some(dir_name);
                        }
                    }
                }
            }
        }
    }
    None
}

impl DetectedBrowser {
    /// Memeriksa apakah browser terdeteksi memiliki adapter yang terverifikasi dan aman diaktifkan.
    pub fn is_verified(&self) -> bool {
        is_edge_executable(&self.executable_path) || is_chrome_executable(&self.executable_path)
    }
}

/// Status dukungan verifikasi browser sederhana (kompatibilitas backward).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserSupportStatus {
    /// Browser telah diverifikasi isolasi sesi dan penanganan kepemilikannya.
    Verified(BrowserAdapter),
    /// Browser belum diverifikasi dalam cakupan yang diuji.
    Unverified,
}

/// Mendeteksi apakah path executable merujuk ke browser yang terverifikasi (Edge atau Chrome).
pub fn detect_browser_support(executable: &Path) -> BrowserSupportStatus {
    let path_str = executable.to_string_lossy();
    let is_simulated_windows =
        cfg!(not(windows)) && (path_str.starts_with("C:") || path_str.starts_with("c:"));

    if is_edge_executable(executable) {
        if executable.exists() && executable.is_file() {
            let arch = read_executable_architecture(executable);
            if arch == BrowserArchitecture::Unknown && !is_simulated_windows {
                return BrowserSupportStatus::Unverified;
            }
        }
        BrowserSupportStatus::Verified(BrowserAdapter::Edge(EdgeAdapter::new()))
    } else if is_chrome_executable(executable) {
        if executable.exists() && executable.is_file() {
            let arch = read_executable_architecture(executable);
            if arch == BrowserArchitecture::Unknown && !is_simulated_windows {
                return BrowserSupportStatus::Unverified;
            }
        }
        BrowserSupportStatus::Verified(BrowserAdapter::Chrome(ChromeAdapter::new()))
    } else {
        BrowserSupportStatus::Unverified
    }
}

/// Mengevaluasi status kompatibilitas browser secara lengkap dan jujur berdasarkan keberadaan file,
/// arsitektur biner, dan adapter yang terverifikasi.
pub fn evaluate_browser_compatibility(executable: &Path) -> BrowserCompatibilityStatus {
    let path_str = executable.to_string_lossy();
    let is_simulated_windows =
        cfg!(not(windows)) && (path_str.starts_with("C:") || path_str.starts_with("c:"));
    let is_single_command = !executable.is_absolute() && executable.components().count() == 1;

    if !is_simulated_windows
        && !is_single_command
        && (!executable.exists() || !executable.is_file())
    {
        return BrowserCompatibilityStatus::Unavailable {
            path: executable.to_path_buf(),
            reason: format!(
                "File executable browser '{}' tidak ditemukan atau tidak tersedia",
                executable.display()
            ),
        };
    }

    let architecture = read_executable_architecture(executable);
    let version = read_installed_browser_version(executable);
    let file_name = extract_executable_filename(executable);

    let is_verified_arch = architecture == BrowserArchitecture::X86
        || architecture == BrowserArchitecture::X64
        || is_simulated_windows;

    if is_edge_executable(executable) {
        if !is_verified_arch {
            BrowserCompatibilityStatus::Unverified {
                evidence: BrowserSupportEvidence {
                    browser_name: "Microsoft Edge".to_string(),
                    version,
                    architecture,
                    is_safe_to_activate: false,
                    evidence_text: "File executable biner Microsoft Edge memiliki arsitektur yang tidak didukung atau rusak; hanya x86 dan x64 pada Windows 10/11 x64 yang diverifikasi.".to_string(),
                    limitations: Some("Aktivasi diblokir karena arsitektur executable belum diverifikasi.".to_string()),
                },
            }
        } else {
            let adapter = EdgeAdapter::new();
            BrowserCompatibilityStatus::SafeToActivate {
                adapter: BrowserAdapter::Edge(adapter),
                evidence: BrowserSupportEvidence {
                    browser_name: "Microsoft Edge".to_string(),
                    version,
                    architecture,
                    is_safe_to_activate: true,
                    evidence_text: "Dukungan pemisahan sesi browser profil (--user-data-dir) dan kepemilikan instance telah diverifikasi pada Windows 10/11 x64.".to_string(),
                    limitations: Some("Mode kiosk atau InPrivate tidak mempertahankan sesi login persisten.".to_string()),
                },
            }
        }
    } else if is_chrome_executable(executable) {
        if !is_verified_arch {
            BrowserCompatibilityStatus::Unverified {
                evidence: BrowserSupportEvidence {
                    browser_name: "Google Chrome".to_string(),
                    version,
                    architecture,
                    is_safe_to_activate: false,
                    evidence_text: "File executable biner Google Chrome memiliki arsitektur yang tidak didukung atau rusak; hanya x86 dan x64 pada Windows 10/11 x64 yang diverifikasi.".to_string(),
                    limitations: Some("Aktivasi diblokir karena arsitektur executable belum diverifikasi.".to_string()),
                },
            }
        } else {
            let adapter = ChromeAdapter::new();
            BrowserCompatibilityStatus::SafeToActivate {
                adapter: BrowserAdapter::Chrome(adapter),
                evidence: BrowserSupportEvidence {
                    browser_name: "Google Chrome".to_string(),
                    version,
                    architecture,
                    is_safe_to_activate: true,
                    evidence_text: "Dukungan pemisahan sesi browser profil (--user-data-dir) dan kepemilikan instance telah diverifikasi pada Windows 10/11 x64.".to_string(),
                    limitations: Some("Mode kiosk atau incognito tidak mempertahankan sesi login persisten.".to_string()),
                },
            }
        }
    } else {
        let browser_name = if file_name == "firefox.exe" || file_name == "firefox" {
            "Mozilla Firefox".to_string()
        } else if file_name == "brave.exe" || file_name == "brave" {
            "Brave Browser".to_string()
        } else {
            let stem = executable
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Browser Portable".to_string());
            format!("Browser {}", stem)
        };

        BrowserCompatibilityStatus::Unverified {
            evidence: BrowserSupportEvidence {
                browser_name,
                version,
                architecture,
                is_safe_to_activate: false,
                evidence_text: "Browser ditemukan pada sistem, namun adapter pemisahan sesi dan kepemilikan instance belum diverifikasi pada Windows 10/11 x64. Profil dapat disimpan tetapi belum aman diaktifkan.".to_string(),
                limitations: Some("Aktivasi diblokir sampai adapter terverifikasi tersedia.".to_string()),
            },
        }
    }
}
