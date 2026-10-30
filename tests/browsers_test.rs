use std::path::PathBuf;
use window_hop::browsers::{
    detect_browser_support, evaluate_browser_compatibility, parse_windows_command_line,
    read_executable_architecture, BrowserArchitecture, BrowserCompatibilityStatus, BrowserError,
    BrowserSupportStatus, ChromeAdapter, EdgeAdapter, LoginRetentionNote,
};

#[test]
fn test_parse_windows_command_line_simple_and_quoted_arguments() {
    // Kosong
    assert_eq!(
        parse_windows_command_line("").unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(
        parse_windows_command_line("   ").unwrap(),
        Vec::<String>::new()
    );

    // Sederhana
    assert_eq!(
        parse_windows_command_line("--kiosk https://antrian.local").unwrap(),
        vec!["--kiosk", "https://antrian.local"]
    );

    // Argumen berpetik dengan spasi di dalamnya
    assert_eq!(
        parse_windows_command_line(r#"--app="https://antrian.poli/display 1" --kiosk"#).unwrap(),
        vec!["--app=https://antrian.poli/display 1", "--kiosk"]
    );

    // Path berpetik
    assert_eq!(
        parse_windows_command_line(r#""C:\Program Files\App\app.exe" --fullscreen"#).unwrap(),
        vec![r"C:\Program Files\App\app.exe", "--fullscreen"]
    );

    // Escaped quote di dalam quote
    assert_eq!(
        parse_windows_command_line(r#"--title="Antrian \"Poli Gigi\"""#).unwrap(),
        vec![r#"--title=Antrian "Poli Gigi""#]
    );
}

#[test]
fn test_parse_windows_command_line_unterminated_quote() {
    let err = parse_windows_command_line(r#"--app="https://unclosed"#).unwrap_err();
    match err {
        BrowserError::UnterminatedQuote(_) => {}
        other => panic!("expected UnterminatedQuote, got: {:?}", other),
    }
}

#[test]
fn test_edge_adapter_rejects_conflicting_session_isolation_arguments() {
    let adapter = EdgeAdapter::new();

    // Valid arguments
    let valid_args = ["--kiosk", "https://antrian.poli/gigi", "--inprivate"];
    assert!(adapter.validate_parsed_arguments(&valid_args).is_ok());

    // Conflicting: --user-data-dir
    let err1 = adapter
        .validate_parsed_arguments(&["--user-data-dir=C:\\temp"])
        .unwrap_err();
    match err1 {
        BrowserError::ConflictingIsolationParameter { arg, .. } => {
            assert!(arg.contains("--user-data-dir"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting: --profile-directory
    let err2 = adapter
        .validate_parsed_arguments(&["--profile-directory=Default"])
        .unwrap_err();
    match err2 {
        BrowserError::ConflictingIsolationParameter { arg, .. } => {
            assert!(arg.contains("--profile-directory"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting: case-insensitive & prefix-agnostic (/USER-DATA-DIR, -profile-directory)
    let err3 = adapter
        .validate_parsed_arguments(&["/USER-DATA-DIR=D:\\data"])
        .unwrap_err();
    match err3 {
        BrowserError::ConflictingIsolationParameter { arg, .. } => {
            assert!(arg.contains("/USER-DATA-DIR"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    let err4 = adapter
        .validate_parsed_arguments(&["-profile-directory:Profile2"])
        .unwrap_err();
    match err4 {
        BrowserError::ConflictingIsolationParameter { arg, .. } => {
            assert!(arg.contains("-profile-directory"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }
}

#[test]
fn test_edge_adapter_generates_isolated_launch_plan_and_login_retention_note() {
    let edge_path = PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe");
    let support = detect_browser_support(&edge_path);

    match support {
        BrowserSupportStatus::Verified(adapter) => {
            assert_eq!(adapter.browser_name(), "Microsoft Edge");
            let data_dir = PathBuf::from(r"C:\AppData\WindowHop\Profiles\p1\data");
            let plan = adapter
                .build_launch_plan(
                    &edge_path,
                    &data_dir,
                    r#"--kiosk "https://antrian.poli/gigi""#,
                )
                .expect("build launch plan");

            assert_eq!(plan.executable, edge_path);
            assert!(plan
                .arguments
                .iter()
                .any(|arg| arg == r"--user-data-dir=C:\AppData\WindowHop\Profiles\p1\data"));
            assert!(plan.arguments.iter().any(|arg| arg == "--kiosk"));
            assert!(plan
                .arguments
                .iter()
                .any(|arg| arg == "https://antrian.poli/gigi"));

            // Login retention note checks
            match plan.login_retention {
                LoginRetentionNote::KioskOrInPrivateNoRetention { explanation } => {
                    assert!(explanation.contains("kiosk"));
                }
                other => panic!("expected KioskOrInPrivateNoRetention, got: {:?}", other),
            }
        }
        BrowserSupportStatus::Unverified => {
            panic!("Edge should be verified");
        }
    }
}

#[test]
fn test_edge_adapter_login_retention_normal_mode() {
    let edge_path = PathBuf::from(r"C:\Windows\System32\msedge.exe");
    let support = detect_browser_support(&edge_path);

    match support {
        BrowserSupportStatus::Verified(adapter) => {
            let data_dir = PathBuf::from(r"C:\data");
            let plan = adapter
                .build_launch_plan(&edge_path, &data_dir, "https://antrian.local")
                .expect("build launch plan");

            match plan.login_retention {
                LoginRetentionNote::SessionIsolated { explanation } => {
                    assert!(explanation.contains("sesi browser profil"));
                }
                other => panic!("expected SessionIsolated, got: {:?}", other),
            }
        }
        BrowserSupportStatus::Unverified => panic!("Edge must be verified"),
    }
}

#[test]
fn test_chrome_adapter_rejects_conflicting_session_isolation_arguments() {
    let adapter = ChromeAdapter::new();

    // Valid arguments
    let valid_args = ["--kiosk", "https://antrian.poli/mata", "--incognito"];
    assert!(adapter.validate_parsed_arguments(&valid_args).is_ok());

    // Conflicting: --user-data-dir
    let err1 = adapter
        .validate_parsed_arguments(&["--user-data-dir=C:\\temp\\data"])
        .unwrap_err();
    match err1 {
        BrowserError::ConflictingIsolationParameter { arg, .. } => {
            assert!(arg.contains("--user-data-dir"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting: --profile-directory
    let err2 = adapter
        .validate_parsed_arguments(&["--profile-directory=Profile 1"])
        .unwrap_err();
    match err2 {
        BrowserError::ConflictingIsolationParameter { arg, .. } => {
            assert!(arg.contains("--profile-directory"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting: case-insensitive & prefix-agnostic (/USER-DATA-DIR, -profile-directory)
    let err3 = adapter
        .validate_parsed_arguments(&["/USER-DATA-DIR=D:\\chrome_data"])
        .unwrap_err();
    match err3 {
        BrowserError::ConflictingIsolationParameter { arg, .. } => {
            assert!(arg.contains("/USER-DATA-DIR"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    let err4 = adapter
        .validate_parsed_arguments(&["-profile-directory:Default"])
        .unwrap_err();
    match err4 {
        BrowserError::ConflictingIsolationParameter { arg, .. } => {
            assert!(arg.contains("-profile-directory"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }
}

#[test]
fn test_chrome_adapter_generates_isolated_launch_plan_and_login_retention_note() {
    let chrome_path = PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe");
    let support = detect_browser_support(&chrome_path);

    match support {
        BrowserSupportStatus::Verified(adapter) => {
            assert_eq!(adapter.browser_name(), "Google Chrome");
            let data_dir = PathBuf::from(r"C:\AppData\WindowHop\Profiles\p2\browser_session");
            let plan = adapter
                .build_launch_plan(
                    &chrome_path,
                    &data_dir,
                    r#"--incognito --kiosk "https://antrian.poli/mata""#,
                )
                .expect("build launch plan");

            assert_eq!(plan.executable, chrome_path);
            assert!(plan
                .arguments
                .iter()
                .any(|arg| arg
                    == r"--user-data-dir=C:\AppData\WindowHop\Profiles\p2\browser_session"));
            assert!(plan.arguments.iter().any(|arg| arg == "--incognito"));
            assert!(plan.arguments.iter().any(|arg| arg == "--kiosk"));
            assert!(plan
                .arguments
                .iter()
                .any(|arg| arg == "https://antrian.poli/mata"));

            // Login retention note checks for incognito/kiosk
            match plan.login_retention {
                LoginRetentionNote::KioskOrInPrivateNoRetention { explanation } => {
                    assert!(explanation.contains("Google Chrome"));
                    assert!(explanation.contains("incognito") || explanation.contains("kiosk"));
                }
                other => panic!("expected KioskOrInPrivateNoRetention, got: {:?}", other),
            }
        }
        BrowserSupportStatus::Unverified => {
            panic!("Chrome should be verified");
        }
    }
}

#[test]
fn test_chrome_adapter_login_retention_normal_mode() {
    let chrome_path = PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe");
    let support = detect_browser_support(&chrome_path);

    match support {
        BrowserSupportStatus::Verified(adapter) => {
            let data_dir = PathBuf::from(r"C:\AppData\WindowHop\Profiles\p2\browser_session");
            let plan = adapter
                .build_launch_plan(&chrome_path, &data_dir, "https://antrian.poli/mata")
                .expect("build launch plan");

            match plan.login_retention {
                LoginRetentionNote::SessionIsolated { explanation } => {
                    assert!(explanation.contains("sesi browser profil"));
                    assert!(explanation.contains("terpisah dari browser kerja perawat"));
                }
                other => panic!("expected SessionIsolated, got: {:?}", other),
            }
        }
        BrowserSupportStatus::Unverified => panic!("Chrome must be verified"),
    }
}

#[test]
fn test_unverified_browser_is_rejected_with_informative_reason() {
    // Firefox is unverified
    let firefox_path = PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe");
    let support_firefox = detect_browser_support(&firefox_path);
    assert_eq!(support_firefox, BrowserSupportStatus::Unverified);

    let unknown_path = PathBuf::from(r"D:\Portable\CustomBrowser.exe");
    let support_unknown = detect_browser_support(&unknown_path);
    assert_eq!(support_unknown, BrowserSupportStatus::Unverified);
}

fn create_mock_pe_file(dir: &std::path::Path, filename: &str, machine: u16) -> PathBuf {
    let file_path = dir.join(filename);
    let mut data = vec![0u8; 256];
    // DOS header: 'MZ'
    data[0] = b'M';
    data[1] = b'Z';
    // Pointer to PE header at offset 0x3C
    let pe_offset: u32 = 0x80;
    data[0x3C..0x40].copy_from_slice(&pe_offset.to_le_bytes());
    // PE signature: 'PE\0\0'
    data[0x80] = b'P';
    data[0x81] = b'E';
    data[0x82] = 0;
    data[0x83] = 0;
    // Machine architecture at pe_offset + 4
    data[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
    std::fs::write(&file_path, data).expect("write mock pe file");
    file_path
}

#[test]
fn test_read_executable_architecture_pe_header() {
    let temp_dir = tempfile::tempdir().expect("tempdir");

    // x86 (IMAGE_FILE_MACHINE_I386 = 0x014c)
    let x86_exe = create_mock_pe_file(temp_dir.path(), "test_x86.exe", 0x014c);
    assert_eq!(
        read_executable_architecture(&x86_exe),
        BrowserArchitecture::X86
    );

    // x64 (IMAGE_FILE_MACHINE_AMD64 = 0x8664)
    let x64_exe = create_mock_pe_file(temp_dir.path(), "test_x64.exe", 0x8664);
    assert_eq!(
        read_executable_architecture(&x64_exe),
        BrowserArchitecture::X64
    );

    // Non-existent file
    assert_eq!(
        read_executable_architecture(&temp_dir.path().join("missing.exe")),
        BrowserArchitecture::Unknown
    );

    // Corrupt / non-PE file
    let corrupt_file = temp_dir.path().join("corrupt.exe");
    std::fs::write(&corrupt_file, b"not a pe binary").unwrap();
    assert_eq!(
        read_executable_architecture(&corrupt_file),
        BrowserArchitecture::Unknown
    );
}

#[test]
fn test_evaluate_browser_compatibility_distinguishes_verified_unverified_and_unavailable() {
    let temp_dir = tempfile::tempdir().expect("tempdir");

    // 1. Unavailable: file does not exist
    let missing_path = temp_dir.path().join("non_existent_browser.exe");
    let status_missing = evaluate_browser_compatibility(&missing_path);
    match status_missing {
        BrowserCompatibilityStatus::Unavailable { path, reason } => {
            assert_eq!(path, missing_path);
            assert!(reason.contains("tidak ditemukan") || reason.contains("tidak tersedia"));
        }
        other => panic!("expected Unavailable, got {:?}", other),
    }

    // 2. Verified: Edge x64
    let edge_x64 = create_mock_pe_file(temp_dir.path(), "msedge.exe", 0x8664);
    let status_edge = evaluate_browser_compatibility(&edge_x64);
    match status_edge {
        BrowserCompatibilityStatus::SafeToActivate { adapter, evidence } => {
            assert_eq!(adapter.browser_name(), "Microsoft Edge");
            assert_eq!(evidence.browser_name, "Microsoft Edge");
            assert_eq!(evidence.architecture, BrowserArchitecture::X64);
            assert!(evidence.is_safe_to_activate);
            assert!(evidence.evidence_text.contains("user-data-dir"));
            assert!(evidence.evidence_text.contains("Windows 10/11 x64"));
        }
        other => panic!("expected SafeToActivate, got {:?}", other),
    }

    // 3. Verified: Edge x86 on 64-bit OS (architecture is recorded, still verified)
    let edge_x86 = create_mock_pe_file(temp_dir.path(), "msedge_x86.exe", 0x014c);
    // Even if named msedge, x86 architecture is preserved honestly
    let edge_x86_named = temp_dir.path().join("edge_32").join("msedge.exe");
    std::fs::create_dir_all(edge_x86_named.parent().unwrap()).unwrap();
    std::fs::copy(&edge_x86, &edge_x86_named).unwrap();
    let status_edge_x86 = evaluate_browser_compatibility(&edge_x86_named);
    match status_edge_x86 {
        BrowserCompatibilityStatus::SafeToActivate { evidence, .. } => {
            assert_eq!(evidence.architecture, BrowserArchitecture::X86);
            assert!(evidence.is_safe_to_activate);
        }
        other => panic!("expected SafeToActivate for Edge x86, got {:?}", other),
    }

    // 4. Verified: Chrome x64
    let chrome_exe = create_mock_pe_file(temp_dir.path(), "chrome.exe", 0x8664);
    let status_chrome = evaluate_browser_compatibility(&chrome_exe);
    match status_chrome {
        BrowserCompatibilityStatus::SafeToActivate { adapter, evidence } => {
            assert_eq!(adapter.browser_name(), "Google Chrome");
            assert_eq!(evidence.browser_name, "Google Chrome");
            assert_eq!(evidence.architecture, BrowserArchitecture::X64);
            assert!(evidence.is_safe_to_activate);
            assert!(evidence.evidence_text.contains("user-data-dir"));
            assert!(evidence.evidence_text.contains("Windows 10/11 x64"));
            assert!(evidence.limitations.as_ref().unwrap().contains("incognito"));
        }
        other => panic!("expected SafeToActivate for Chrome x64, got {:?}", other),
    }

    // 4b. Verified: Chrome x86
    let chrome_x86 = create_mock_pe_file(temp_dir.path(), "chrome_x86.exe", 0x014c);
    let chrome_x86_named = temp_dir.path().join("chrome_32").join("chrome.exe");
    std::fs::create_dir_all(chrome_x86_named.parent().unwrap()).unwrap();
    std::fs::copy(&chrome_x86, &chrome_x86_named).unwrap();
    let status_chrome_x86 = evaluate_browser_compatibility(&chrome_x86_named);
    match status_chrome_x86 {
        BrowserCompatibilityStatus::SafeToActivate { evidence, .. } => {
            assert_eq!(evidence.architecture, BrowserArchitecture::X86);
            assert!(evidence.is_safe_to_activate);
        }
        other => panic!("expected SafeToActivate for Chrome x86, got {:?}", other),
    }

    // 4c. Blocked: Chrome with corrupt/non-PE binary on disk (unknown architecture)
    let corrupt_chrome = temp_dir.path().join("corrupt_chrome").join("chrome.exe");
    std::fs::create_dir_all(corrupt_chrome.parent().unwrap()).unwrap();
    std::fs::write(&corrupt_chrome, b"not a valid pe header").unwrap();
    let status_corrupt_chrome = evaluate_browser_compatibility(&corrupt_chrome);
    match status_corrupt_chrome {
        BrowserCompatibilityStatus::Unverified { evidence } => {
            assert_eq!(evidence.browser_name, "Google Chrome");
            assert_eq!(evidence.architecture, BrowserArchitecture::Unknown);
            assert!(!evidence.is_safe_to_activate);
            assert!(evidence.evidence_text.contains("arsitektur"));
            assert!(evidence
                .limitations
                .as_ref()
                .unwrap()
                .contains("Aktivasi diblokir"));
        }
        other => panic!(
            "expected Unverified for corrupt Chrome binary, got {:?}",
            other
        ),
    }

    // 5. Unverified: Firefox x64 (detected/present on disk, but unverified adapter)
    let firefox_exe = create_mock_pe_file(temp_dir.path(), "firefox.exe", 0x8664);
    let status_firefox = evaluate_browser_compatibility(&firefox_exe);
    match status_firefox {
        BrowserCompatibilityStatus::Unverified { evidence } => {
            assert_eq!(evidence.browser_name, "Mozilla Firefox");
            assert_eq!(evidence.architecture, BrowserArchitecture::X64);
            assert!(!evidence.is_safe_to_activate);
            assert!(evidence.evidence_text.contains("belum diverifikasi"));
            assert!(evidence.evidence_text.contains("disimpan"));
        }
        other => panic!("expected Unverified for Firefox, got {:?}", other),
    }

    // 5b. Unverified: Custom portable executable
    let portable_exe = create_mock_pe_file(temp_dir.path(), "portable_app.exe", 0x014c);
    let status_portable = evaluate_browser_compatibility(&portable_exe);
    match status_portable {
        BrowserCompatibilityStatus::Unverified { evidence } => {
            assert_eq!(evidence.architecture, BrowserArchitecture::X86);
            assert!(!evidence.is_safe_to_activate);
            assert!(evidence.evidence_text.contains("belum diverifikasi"));
        }
        other => panic!("expected Unverified, got {:?}", other),
    }

    // 6. Installed browser with sibling version folder (e.g. Chrome Application\120.0.6099.130)
    let chrome_app_dir = temp_dir.path().join("chrome_versioned");
    let chrome_version_dir = chrome_app_dir.join("120.0.6099.130");
    std::fs::create_dir_all(&chrome_version_dir).unwrap();
    let chrome_versioned_exe = create_mock_pe_file(&chrome_app_dir, "chrome.exe", 0x8664);
    let status_chrome_ver = evaluate_browser_compatibility(&chrome_versioned_exe);
    match status_chrome_ver {
        BrowserCompatibilityStatus::SafeToActivate { evidence, .. } => {
            assert_eq!(evidence.version, Some("120.0.6099.130".to_string()));
            assert_eq!(evidence.architecture, BrowserArchitecture::X64);
            assert!(evidence.is_safe_to_activate);
        }
        other => panic!(
            "expected SafeToActivate with Chrome version, got {:?}",
            other
        ),
    }

    // 6. Installed browser with sibling version folder (e.g. Edge Application\120.0.0.0)
    let app_dir = temp_dir.path().join("edge_versioned");
    let version_dir = app_dir.join("120.0.2210.144");
    std::fs::create_dir_all(&version_dir).unwrap();
    let edge_versioned_exe = create_mock_pe_file(&app_dir, "msedge.exe", 0x8664);
    let status_versioned = evaluate_browser_compatibility(&edge_versioned_exe);
    match status_versioned {
        BrowserCompatibilityStatus::SafeToActivate { evidence, .. } => {
            assert_eq!(evidence.version, Some("120.0.2210.144".to_string()));
            assert_eq!(evidence.architecture, BrowserArchitecture::X64);
            assert!(evidence.is_safe_to_activate);
        }
        other => panic!("expected SafeToActivate with version, got {:?}", other),
    }
}
