//! Browser capabilities, isolation verification, and launch planning.

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
            for &(prefix, reason) in Self::CONFLICTING_PREFIXES {
                if trimmed == prefix
                    || trimmed.starts_with(&format!("{}=", prefix))
                    || trimmed.starts_with(&format!("{}:", prefix))
                {
                    return Err(BrowserError::ConflictingIsolationParameter {
                        arg: trimmed.to_string(),
                        reason: reason.to_string(),
                    });
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

/// Status dukungan verifikasi browser.
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
