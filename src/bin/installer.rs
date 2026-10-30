//! Window Hop installer CLI executable.
//!
//! Provides automated and interactive installation of Window Hop to a consistent
//! application directory (%LOCALAPPDATA%\Programs\Window Hop on Windows), bundling
//! GPUI dependencies and runtime guidance.

use std::env;
use std::path::PathBuf;
use window_hop::installer::{
    default_install_dir, install, validate_os_baseline, InstallError, InstallOptions, OsVersion,
};

fn print_help() {
    println!(
        "Window Hop Installer v{}\n\n\
        PENGGUNAAN:\n    \
            window-hop-installer [OPSI]\n\n\
        OPSI:\n    \
            -d, --dir <PATH>       Tentukan direktori tujuan instalasi khusus\n                           \
                                   (default: %LOCALAPPDATA%\\Programs\\Window Hop)\n    \
            -s, --source <PATH>    Tentukan path berkas executable window-hop.exe sumber\n    \
            -q, --silent           Mode senyap (tanpa konfirmasi interaktif)\n    \
            -h, --help             Tampilkan bantuan ini\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn detect_current_os() -> Result<OsVersion, InstallError> {
    #[cfg(windows)]
    {
        // Deteksi arsitektur via environment PROCESSOR_ARCHITECTURE
        let arch = match env::var("PROCESSOR_ARCHITECTURE").as_deref() {
            Ok("AMD64") | Ok("x86_64") => Arch::X64,
            Ok("x86") => Arch::X86,
            Ok("ARM64") => Arch::Arm64,
            _ => Arch::Other,
        };

        // Mengambil versi Windows via powershell
        let (major, minor, build) = get_windows_version()?;
        Ok(OsVersion {
            major,
            minor,
            build,
            arch,
        })
    }

    #[cfg(not(windows))]
    {
        Err(InstallError::UnsupportedPlatform(
            "Installer executable native Window Hop hanya dapat dijalankan pada platform Windows 10/11 x64."
                .to_string(),
        ))
    }
}

#[cfg(windows)]
fn get_windows_version() -> Result<(u32, u32, u32), InstallError> {
    let script = "& { [System.Environment]::OSVersion.Version.ToString() }";
    let output = std::process::Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(script)
        .output()
        .map_err(|e| {
            InstallError::VerificationFailed(format!(
                "Gagal menjalankan powershell untuk verifikasi versi Windows: {}",
                e
            ))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(InstallError::VerificationFailed(format!(
            "Pemeriksaan versi Windows gagal: {}",
            stderr.trim()
        )));
    }

    let s = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = s.trim().split('.').collect();
    if parts.len() >= 3 {
        let major = parts[0].parse().map_err(|e| {
            InstallError::VerificationFailed(format!("Gagal mem-parsing nomor major OS: {}", e))
        })?;
        let minor = parts[1].parse().map_err(|e| {
            InstallError::VerificationFailed(format!("Gagal mem-parsing nomor minor OS: {}", e))
        })?;
        let build = parts[2].parse().map_err(|e| {
            InstallError::VerificationFailed(format!("Gagal mem-parsing nomor build OS: {}", e))
        })?;
        return Ok((major, minor, build));
    }

    Err(InstallError::VerificationFailed(format!(
        "Format versi OS tidak terduga: {}",
        s.trim()
    )))
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut custom_dir = None;
    let mut custom_source = None;
    let mut _is_silent = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return;
            }
            "-q" | "--silent" => {
                _is_silent = true;
            }
            "-d" | "--dir" => {
                if i + 1 < args.len() {
                    custom_dir = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "-s" | "--source" => {
                if i + 1 < args.len() {
                    custom_source = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            other => {
                eprintln!("Argumen tidak dikenal: {}", other);
                print_help();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let install_dir = custom_dir.unwrap_or_else(default_install_dir);

    // 1. Validasi baseline sistem operasi
    let os_version = match detect_current_os() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Pemeriksaan baseline sistem operasi gagal:\n  {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = validate_os_baseline(&os_version) {
        eprintln!("Pemeriksaan baseline sistem operasi gagal:\n  {}", e);
        eprintln!("\nWindow Hop memerlukan Windows 10 x64 (build 19041+) atau Windows 11 x64.");
        std::process::exit(1);
    }

    // 2. Tentukan source payload window-hop.exe
    let source_exe = match custom_source {
        Some(path) => path,
        None => {
            let current_dir_exe = PathBuf::from("window-hop.exe");
            let mut resolved = None;
            if current_dir_exe.exists() {
                resolved = Some(current_dir_exe);
            } else if let Ok(curr) = env::current_exe() {
                if let Some(parent) = curr.parent() {
                    let sibling = parent.join("window-hop.exe");
                    if sibling.exists() {
                        resolved = Some(sibling);
                    }
                }
            }
            resolved.unwrap_or_else(|| PathBuf::from("window-hop.exe"))
        }
    };

    println!("Memasang Window Hop ke: {}", install_dir.display());
    let options = InstallOptions {
        install_dir: install_dir.clone(),
        source_exe,
        bundle_resources: true,
    };

    match install(&options) {
        Ok(receipt) => {
            println!("\nInstalasi Window Hop berhasil diselesaikan!");
            println!("  Versi:       {}", receipt.version);
            println!("  Lokasi:      {}", receipt.installed_exe.display());
            println!(
                "  Berkas lain: {} berkas terpasang",
                receipt.installed_files.len()
            );
            println!(
                "  Dokumentasi: {}",
                receipt.install_dir.join("INSTALL-GUIDE.txt").display()
            );
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("\nInstalasi Window Hop gagal:\n  {}", e);
            std::process::exit(1);
        }
    }
}
