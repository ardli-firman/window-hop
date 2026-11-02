//! Window Hop installer CLI and GUI setup executable.
//!
//! Provides automated, interactive, and self-contained bundled installation of Window Hop
//! to a consistent application directory (%LOCALAPPDATA%\Programs\Window Hop on Windows),
//! creating Start Menu and Desktop shortcuts, registering Windows Uninstaller, and bundling
//! GPUI dependencies and runtime guidance.

mod payload {
    include!(concat!(env!("OUT_DIR"), "/embedded_payload.rs"));
}

use std::env;
use std::path::PathBuf;
use window_hop::installer::{
    default_install_dir, install, uninstall, validate_os_baseline, InstallError, InstallOptions,
    OsVersion,
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
            -q, --silent           Mode senyap (tanpa keluaran stdout / dialog)\n    \
            -u, --uninstall        Copot (uninstall) Window Hop dari sistem\n    \
            --no-shortcuts         Jangan buat shortcut Start Menu atau Desktop\n    \
            -h, --help             Tampilkan bantuan ini\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn detect_current_os() -> Result<OsVersion, InstallError> {
    #[cfg(windows)]
    {
        window_hop::installer::detect_windows_os_version()
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
fn show_confirm_dialog(message: &str, title: &str) -> bool {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO};

    let res = unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from(title),
            MB_YESNO | MB_ICONQUESTION,
        )
    };
    res == IDYES
}

#[cfg(windows)]
fn show_info_dialog(message: &str, title: &str) -> bool {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONINFORMATION, MB_YESNO,
    };

    let res = unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from(title),
            MB_YESNO | MB_ICONINFORMATION,
        )
    };
    res == IDYES
}

#[cfg(windows)]
fn show_error_dialog(message: &str, title: &str) {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(message),
            &HSTRING::from(title),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn resolve_payload_source(custom_source: Option<PathBuf>) -> PathBuf {
    if let Some(path) = custom_source {
        return path;
    }

    // Jika ada embedded payload, kita tidak mewajibkan file fisik eksternal
    if payload::EMBEDDED_PAYLOAD.is_some() {
        return PathBuf::from("embedded:window-hop.exe");
    }

    // Fallback pencarian file loose window-hop.exe di direktori aktif atau samping installer
    let current_dir_exe = PathBuf::from("window-hop.exe");
    if current_dir_exe.exists() {
        return current_dir_exe;
    }

    if let Ok(curr) = env::current_exe() {
        if let Some(parent) = curr.parent() {
            let sibling = parent.join("window-hop.exe");
            if sibling.exists() {
                return sibling;
            }
        }
    }

    PathBuf::from("window-hop.exe")
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let exe_name = env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    let is_uninstall_binary = exe_name.contains("uninstall");

    let mut custom_dir = None;
    let mut custom_source = None;
    let mut is_silent = false;
    let mut is_uninstall = is_uninstall_binary;
    let mut no_shortcuts = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_help();
                return;
            }
            "-q" | "--silent" => {
                is_silent = true;
            }
            "-u" | "--uninstall" => {
                is_uninstall = true;
            }
            "--no-shortcuts" => {
                no_shortcuts = true;
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
    let _is_interactive = args.len() == 1 || (is_uninstall && !is_silent); // Pengguna klik ganda installer/uninstaller di Windows Explorer

    // Alur Pencopotan (Uninstallation)
    if is_uninstall {
        #[cfg(windows)]
        {
            if !is_silent && _is_interactive {
                let msg = format!(
                    "Apakah Anda yakin ingin mencopot pemasangan Window Hop dari komputer?\n\nDirektori: {}",
                    install_dir.display()
                );
                if !show_confirm_dialog(&msg, "Window Hop Uninstaller") {
                    std::process::exit(0);
                }
            }
        }

        match uninstall(&install_dir) {
            Ok(receipt) => {
                if !is_silent {
                    println!("Window Hop berhasil dicopot dari sistem.");
                    println!(
                        "  Berkas dibersihkan: {} berkas",
                        receipt.uninstalled_files.len()
                    );
                    println!(
                        "  Shortcut dibersihkan: {} shortcut",
                        receipt.cleaned_shortcuts.len()
                    );
                }
                #[cfg(windows)]
                {
                    if !is_silent && _is_interactive {
                        let _ = show_info_dialog(
                            "Window Hop telah berhasil dicopot dari komputer Anda.",
                            "Pencopotan Selesai",
                        );
                    }
                }
                std::process::exit(0);
            }
            Err(e) => {
                let err_msg = format!("Pencopotan Window Hop gagal:\n  {}", e);
                eprintln!("{}", err_msg);
                #[cfg(windows)]
                {
                    if !is_silent && _is_interactive {
                        show_error_dialog(&err_msg, "Pencopotan Gagal");
                    }
                }
                std::process::exit(1);
            }
        }
    }

    // 1. Validasi baseline sistem operasi
    let os_version = match detect_current_os() {
        Ok(v) => v,
        Err(e) => {
            let err_msg = format!("Pemeriksaan baseline sistem operasi gagal:\n  {}", e);
            eprintln!("{}", err_msg);
            #[cfg(windows)]
            {
                if !is_silent && _is_interactive {
                    show_error_dialog(&err_msg, "Kesalahan Sistem Operasi");
                }
            }
            std::process::exit(1);
        }
    };

    if let Err(e) = validate_os_baseline(&os_version) {
        let err_msg = format!(
            "Pemeriksaan baseline sistem operasi gagal:\n  {}\n\nWindow Hop memerlukan Windows 10 x64 (build 19041+) atau Windows 11 x64.",
            e
        );
        eprintln!("{}", err_msg);
        #[cfg(windows)]
        {
            if !is_silent && _is_interactive {
                show_error_dialog(&err_msg, "Sistem Operasi Tidak Didukung");
            }
        }
        std::process::exit(1);
    }

    // 2. Dialog konfirmasi interaktif saat pengguna mengklik installer di Windows
    #[cfg(windows)]
    {
        if !is_silent && _is_interactive {
            let prompt = format!(
                "Selamat datang di Window Hop Installer!\n\n\
                Aplikasi Window Hop akan dipasang ke lokasi resmi per-pengguna:\n\
                {}\n\n\
                Shortcut Start Menu dan Desktop akan dibuat secara otomatis.\n\n\
                Apakah Anda ingin melanjutkan instalasi?",
                install_dir.display()
            );
            if !show_confirm_dialog(&prompt, "Window Hop Setup") {
                std::process::exit(0);
            }
        }
    }

    // 3. Tentukan payload source
    let source_exe = resolve_payload_source(custom_source);

    if !is_silent {
        println!("Memasang Window Hop ke: {}", install_dir.display());
    }

    let options = InstallOptions {
        install_dir: install_dir.clone(),
        source_exe,
        embedded_payload: payload::EMBEDDED_PAYLOAD,
        bundle_resources: true,
        create_start_menu_shortcut: !no_shortcuts,
        create_desktop_shortcut: !no_shortcuts,
        register_uninstaller: true,
    };

    match install(&options) {
        Ok(receipt) => {
            if !is_silent {
                println!("\nInstalasi Window Hop berhasil diselesaikan!");
                println!("  Versi:       {}", receipt.version);
                println!("  Lokasi:      {}", receipt.installed_exe.display());
                println!(
                    "  Berkas lain: {} berkas terpasang",
                    receipt.installed_files.len()
                );
                println!(
                    "  Shortcut:    {} shortcut dibuat",
                    receipt.shortcuts_created.len()
                );
                println!(
                    "  Dokumentasi: {}",
                    receipt.install_dir.join("INSTALL-GUIDE.txt").display()
                );
            }

            #[cfg(windows)]
            {
                if !is_silent && _is_interactive {
                    let finish_msg = format!(
                        "Instalasi Window Hop berhasil diselesaikan!\n\n\
                        Versi:  {}\n\
                        Lokasi: {}\n\n\
                        Apakah Anda ingin langsung menjalankan Window Hop sekarang?",
                        receipt.version,
                        receipt.installed_exe.display()
                    );
                    let should_run = show_info_dialog(&finish_msg, "Instalasi Selesai");
                    if should_run {
                        let _ = std::process::Command::new(&receipt.installed_exe).spawn();
                    }
                }
            }

            std::process::exit(0);
        }
        Err(e) => {
            let err_msg = format!("\nInstalasi Window Hop gagal:\n  {}", e);
            eprintln!("{}", err_msg);
            #[cfg(windows)]
            {
                if !is_silent && _is_interactive {
                    show_error_dialog(&err_msg, "Instalasi Gagal");
                }
            }
            std::process::exit(1);
        }
    }
}
