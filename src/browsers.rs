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

/// Arsitektur biner executable browser (x86, x64, ARM64, atau tidak diketahui).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrowserArchitecture {
    X86,
    X64,
    Arm64,
    Unknown,
}

impl fmt::Display for BrowserArchitecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::X86 => write!(f, "x86 (32-bit)"),
            Self::X64 => write!(f, "x64 (64-bit)"),
            Self::Arm64 => write!(f, "ARM64"),
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
        0x014c => BrowserArchitecture::X86,   // IMAGE_FILE_MACHINE_I386
        0x8664 => BrowserArchitecture::X64,   // IMAGE_FILE_MACHINE_AMD64
        0xaa64 => BrowserArchitecture::Arm64, // IMAGE_FILE_MACHINE_ARM64
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
/// 1. `SafeToActivate`: Executable terverifikasi untuk isolasi sesi dan kepemilikan instance (misal Microsoft Edge).
/// 2. `Unverified`: Executable ditemukan pada disk tetapi belum diverifikasi; profil boleh disimpan namun gate aktivasi diblokir.
/// 3. `Unavailable`: Executable tidak ditemukan atau tidak dapat diakses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserCompatibilityStatus {
    SafeToActivate {
        adapter: EdgeAdapter,
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
    pub fn adapter(&self) -> Option<EdgeAdapter> {
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

/// Status dukungan verifikasi browser sederhana (kompatibilitas backward).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserSupportStatus {
    /// Browser telah diverifikasi isolasi sesi dan penanganan kepemilikannya.
    Verified(EdgeAdapter),
    /// Browser belum diverifikasi dalam cakupan yang diuji.
    Unverified,
}

/// Mendeteksi apakah path executable merujuk ke browser yang terverifikasi (Edge).
pub fn detect_browser_support(executable: &Path) -> BrowserSupportStatus {
    let path_str = executable.to_string_lossy();
    let file_name = path_str
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    if file_name == "msedge.exe" || file_name == "msedge" {
        BrowserSupportStatus::Verified(EdgeAdapter::new())
    } else {
        BrowserSupportStatus::Unverified
    }
}

/// Mengevaluasi status kompatibilitas browser secara lengkap dan jujur berdasarkan keberadaan file,
/// arsitektur biner, dan adapter yang terverifikasi.
pub fn evaluate_browser_compatibility(executable: &Path) -> BrowserCompatibilityStatus {
    if !executable.exists() || !executable.is_file() {
        return BrowserCompatibilityStatus::Unavailable {
            path: executable.to_path_buf(),
            reason: format!(
                "File executable browser '{}' tidak ditemukan atau tidak tersedia",
                executable.display()
            ),
        };
    }

    let architecture = read_executable_architecture(executable);
    let path_str = executable.to_string_lossy();
    let file_name = path_str
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();

    if file_name == "msedge.exe" || file_name == "msedge" {
        let adapter = EdgeAdapter::new();
        BrowserCompatibilityStatus::SafeToActivate {
            adapter,
            evidence: BrowserSupportEvidence {
                browser_name: "Microsoft Edge".to_string(),
                version: None,
                architecture,
                is_safe_to_activate: true,
                evidence_text: "Dukungan pemisahan sesi browser profil (--user-data-dir) dan kepemilikan instance telah diverifikasi pada Windows 10/11 x64.".to_string(),
                limitations: Some("Mode kiosk atau InPrivate tidak mempertahankan sesi login persisten.".to_string()),
            },
        }
    } else {
        let browser_name = if file_name == "chrome.exe" || file_name == "chrome" {
            "Google Chrome".to_string()
        } else if file_name == "firefox.exe" || file_name == "firefox" {
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
                version: None,
                architecture,
                is_safe_to_activate: false,
                evidence_text: "Browser ditemukan pada sistem, namun adapter pemisahan sesi dan kepemilikan instance belum diverifikasi pada Windows 10/11 x64. Profil dapat disimpan tetapi belum aman diaktifkan.".to_string(),
                limitations: Some("Aktivasi diblokir sampai adapter terverifikasi tersedia.".to_string()),
            },
        }
    }
}
