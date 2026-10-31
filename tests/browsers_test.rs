use std::path::PathBuf;
use window_hop::browsers::{
    detect_browser_support, evaluate_browser_compatibility, is_brave_executable,
    is_firefox_executable, parse_windows_command_line, read_executable_architecture, BraveAdapter,
    BrowserAdapter, BrowserArchitecture, BrowserCompatibilityStatus, BrowserError,
    BrowserSupportStatus, ChromeAdapter, DetectedBrowser, EdgeAdapter, FirefoxAdapter,
    LaunchParameters, LoginRetentionNote,
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

            // Login retention note checks: incognito prevents persistent login
            match plan.login_retention {
                LoginRetentionNote::KioskOrInPrivateNoRetention { explanation } => {
                    assert!(explanation.contains("Google Chrome"));
                    assert!(explanation.contains("incognito"));
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
fn test_chrome_adapter_kiosk_retains_persistent_login_when_not_incognito() {
    let chrome_path = PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe");
    let support = detect_browser_support(&chrome_path);

    match support {
        BrowserSupportStatus::Verified(adapter) => {
            let data_dir = PathBuf::from(r"C:\AppData\WindowHop\Profiles\p2\browser_session");
            let plan = adapter
                .build_launch_plan(
                    &chrome_path,
                    &data_dir,
                    r#"--kiosk "https://antrian.poli/mata""#,
                )
                .expect("build launch plan");

            // Unlike Edge, Chrome fullscreen kiosk without --incognito preserves persistent login in user-data-dir
            match plan.login_retention {
                LoginRetentionNote::SessionIsolated { explanation } => {
                    assert!(explanation.contains("sesi browser profil"));
                    assert!(explanation.contains("terpisah dari browser kerja perawat"));
                }
                other => panic!(
                    "expected SessionIsolated for Chrome kiosk, got: {:?}",
                    other
                ),
            }
        }
        BrowserSupportStatus::Unverified => panic!("Chrome must be verified"),
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
fn test_firefox_adapter_rejects_conflicting_session_isolation_arguments() {
    let adapter = FirefoxAdapter::new();

    // Valid arguments
    let valid_args = ["--kiosk", "https://antrian.poli/mata", "-private-window"];
    assert!(adapter.validate_parsed_arguments(&valid_args).is_ok());

    // Conflicting: -profile
    let err1 = adapter
        .validate_parsed_arguments(&["-profile", "C:\\temp\\data"])
        .unwrap_err();
    match err1 {
        BrowserError::ConflictingIsolationParameter { arg, reason } => {
            assert!(arg.contains("-profile"));
            assert!(reason.contains("direktori profil terisolasi"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting: -profile=C:\temp
    let err1b = adapter
        .validate_parsed_arguments(&["--profile=C:\\temp"])
        .unwrap_err();
    match err1b {
        BrowserError::ConflictingIsolationParameter { arg, .. } => {
            assert!(arg.contains("--profile"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting: -P / -p
    let err2 = adapter
        .validate_parsed_arguments(&["-P", "NurseProfile"])
        .unwrap_err();
    match err2 {
        BrowserError::ConflictingIsolationParameter { arg, reason } => {
            assert!(arg.contains("-P"));
            assert!(reason.contains("-P"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    let err2b = adapter
        .validate_parsed_arguments(&["-p:NurseProfile"])
        .unwrap_err();
    match err2b {
        BrowserError::ConflictingIsolationParameter { arg, .. } => {
            assert!(arg.contains("-p"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting: unspaced -P<name> (e.g. -Pnurse)
    let err2c = adapter.validate_parsed_arguments(&["-Pnurse"]).unwrap_err();
    match err2c {
        BrowserError::ConflictingIsolationParameter { arg, reason } => {
            assert!(arg.contains("-Pnurse"));
            assert!(reason.contains("-P"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting: -ProfileManager
    let err3 = adapter
        .validate_parsed_arguments(&["-ProfileManager"])
        .unwrap_err();
    match err3 {
        BrowserError::ConflictingIsolationParameter { arg, reason } => {
            assert!(arg.contains("-ProfileManager"));
            assert!(reason.contains("Profile Manager"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting: -CreateProfile
    let err4 = adapter
        .validate_parsed_arguments(&["-CreateProfile", "Nurse"])
        .unwrap_err();
    match err4 {
        BrowserError::ConflictingIsolationParameter { arg, reason } => {
            assert!(arg.contains("-CreateProfile"));
            assert!(reason.contains("pembuatan profil"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting: -no-remote
    let err5 = adapter
        .validate_parsed_arguments(&["-no-remote"])
        .unwrap_err();
    match err5 {
        BrowserError::ConflictingIsolationParameter { arg, reason } => {
            assert!(arg.contains("-no-remote"));
            assert!(reason.contains("-no-remote"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting Chromium arguments: --user-data-dir
    let err6 = adapter
        .validate_parsed_arguments(&["--user-data-dir=C:\\data"])
        .unwrap_err();
    match err6 {
        BrowserError::ConflictingIsolationParameter { arg, reason } => {
            assert!(arg.contains("--user-data-dir"));
            assert!(reason.contains("Chromium"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }
}

#[test]
fn test_firefox_adapter_generates_isolated_launch_plan_and_login_retention_note() {
    let adapter = FirefoxAdapter::new();
    assert_eq!(adapter.browser_name(), "Mozilla Firefox");

    let firefox_path = PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe");
    let data_dir = PathBuf::from(r"C:\AppData\WindowHop\Profiles\p3\browser_session");

    let plan = adapter
        .build_launch_plan(
            &firefox_path,
            &data_dir,
            r#"-private-window --kiosk "https://antrian.poli/mata""#,
        )
        .expect("build launch plan");

    assert_eq!(plan.executable, firefox_path);
    assert_eq!(plan.user_data_dir, data_dir);

    // Verify Firefox isolation arguments: -no-remote -profile <path>
    assert_eq!(plan.arguments[0], "-no-remote");
    assert_eq!(plan.arguments[1], "-profile");
    assert_eq!(
        plan.arguments[2],
        r"C:\AppData\WindowHop\Profiles\p3\browser_session"
    );
    assert!(plan.arguments.iter().any(|arg| arg == "-private-window"));
    assert!(plan.arguments.iter().any(|arg| arg == "--kiosk"));
    assert!(plan
        .arguments
        .iter()
        .any(|arg| arg == "https://antrian.poli/mata"));

    // Private mode does not retain login
    match plan.login_retention {
        LoginRetentionNote::KioskOrInPrivateNoRetention { explanation } => {
            assert!(explanation.contains("Mozilla Firefox"));
            assert!(explanation.contains("privat"));
        }
        other => panic!("expected KioskOrInPrivateNoRetention, got: {:?}", other),
    }
}

#[test]
fn test_firefox_adapter_login_retention_normal_and_kiosk_mode() {
    let adapter = FirefoxAdapter::new();
    let firefox_path = PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe");
    let data_dir = PathBuf::from(r"C:\AppData\WindowHop\Profiles\p3\browser_session");

    // Normal mode: retains persistent session in isolated profile
    let plan_normal = adapter
        .build_launch_plan(&firefox_path, &data_dir, "https://antrian.poli/mata")
        .expect("build normal launch plan");

    match plan_normal.login_retention {
        LoginRetentionNote::SessionIsolated { explanation } => {
            assert!(explanation.contains("sesi browser profil"));
            assert!(explanation.contains("terpisah dari browser kerja perawat"));
        }
        other => panic!("expected SessionIsolated, got: {:?}", other),
    }

    // Fullscreen kiosk mode (without private): retains persistent session in isolated profile
    let plan_kiosk = adapter
        .build_launch_plan(
            &firefox_path,
            &data_dir,
            r#"--kiosk "https://antrian.poli/mata""#,
        )
        .expect("build kiosk launch plan");

    match plan_kiosk.login_retention {
        LoginRetentionNote::SessionIsolated { explanation } => {
            assert!(explanation.contains("sesi browser profil"));
            assert!(explanation.contains("terpisah dari browser kerja perawat"));
        }
        other => panic!("expected SessionIsolated, got: {:?}", other),
    }
}

#[test]
fn test_brave_adapter_rejects_conflicting_session_isolation_arguments() {
    let adapter = BraveAdapter::new();

    // Valid arguments
    let valid_args = ["--kiosk", "https://antrian.poli/anak", "--incognito"];
    assert!(adapter.validate_parsed_arguments(&valid_args).is_ok());

    // Conflicting: --user-data-dir
    let err1 = adapter
        .validate_parsed_arguments(&["--user-data-dir=C:\\temp\\brave_data"])
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
        .validate_parsed_arguments(&["/USER-DATA-DIR=D:\\brave_data"])
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
fn test_brave_adapter_generates_isolated_launch_plan_and_login_retention_note() {
    let brave_path =
        PathBuf::from(r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe");
    let adapter = BraveAdapter::new();
    assert_eq!(adapter.browser_name(), "Brave Browser");

    let data_dir = PathBuf::from(r"C:\AppData\WindowHop\Profiles\p_brave\browser_session");
    let plan = adapter
        .build_launch_plan(
            &brave_path,
            &data_dir,
            r#"--incognito --kiosk "https://antrian.poli/anak""#,
        )
        .expect("build launch plan");

    assert_eq!(plan.executable, brave_path);
    assert!(
        plan.arguments
            .iter()
            .any(|arg| arg
                == r"--user-data-dir=C:\AppData\WindowHop\Profiles\p_brave\browser_session")
    );
    assert!(plan.arguments.iter().any(|arg| arg == "--incognito"));
    assert!(plan.arguments.iter().any(|arg| arg == "--kiosk"));
    assert!(plan
        .arguments
        .iter()
        .any(|arg| arg == "https://antrian.poli/anak"));

    // Login retention note checks: incognito prevents persistent login
    match plan.login_retention {
        LoginRetentionNote::KioskOrInPrivateNoRetention { explanation } => {
            assert!(explanation.contains("Brave Browser"));
            assert!(explanation.contains("incognito") || explanation.contains("Tor"));
        }
        other => panic!("expected KioskOrInPrivateNoRetention, got: {:?}", other),
    }

    // Also test --tor mode
    let plan_tor = adapter
        .build_launch_plan(
            &brave_path,
            &data_dir,
            r#"--tor "https://antrian.poli/anak""#,
        )
        .expect("build tor launch plan");
    match plan_tor.login_retention {
        LoginRetentionNote::KioskOrInPrivateNoRetention { explanation } => {
            assert!(explanation.contains("Brave Browser"));
            assert!(explanation.contains("Tor") || explanation.contains("incognito"));
        }
        other => panic!(
            "expected KioskOrInPrivateNoRetention for Tor, got: {:?}",
            other
        ),
    }
}

#[test]
fn test_brave_adapter_kiosk_retains_persistent_login_when_not_incognito() {
    let brave_path =
        PathBuf::from(r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe");
    let adapter = BraveAdapter::new();
    let data_dir = PathBuf::from(r"C:\AppData\WindowHop\Profiles\p_brave\browser_session");

    let plan = adapter
        .build_launch_plan(
            &brave_path,
            &data_dir,
            r#"--kiosk "https://antrian.poli/anak""#,
        )
        .expect("build launch plan");

    match plan.login_retention {
        LoginRetentionNote::SessionIsolated { explanation } => {
            assert!(explanation.contains("sesi browser profil"));
            assert!(explanation.contains("terpisah dari browser kerja perawat"));
        }
        other => panic!("expected SessionIsolated for Brave kiosk, got: {:?}", other),
    }
}

#[test]
fn test_brave_adapter_login_retention_normal_mode() {
    let brave_path =
        PathBuf::from(r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe");
    let adapter = BraveAdapter::new();
    let data_dir = PathBuf::from(r"C:\AppData\WindowHop\Profiles\p_brave\browser_session");

    let plan = adapter
        .build_launch_plan(&brave_path, &data_dir, "https://antrian.poli/anak")
        .expect("build launch plan");

    match plan.login_retention {
        LoginRetentionNote::SessionIsolated { explanation } => {
            assert!(explanation.contains("sesi browser profil"));
            assert!(explanation.contains("terpisah dari browser kerja perawat"));
        }
        other => panic!("expected SessionIsolated, got: {:?}", other),
    }
}

#[test]
fn test_is_firefox_executable_and_detected_browser_is_verified() {
    assert!(is_firefox_executable(&PathBuf::from("firefox.exe")));
    assert!(is_firefox_executable(&PathBuf::from("firefox")));
    assert!(is_firefox_executable(&PathBuf::from(
        r"C:\Program Files\Mozilla Firefox\firefox.exe"
    )));
    assert!(!is_firefox_executable(&PathBuf::from("msedge.exe")));
    assert!(!is_firefox_executable(&PathBuf::from("chrome.exe")));
    assert!(!is_firefox_executable(&PathBuf::from("brave.exe")));

    let detected_firefox = DetectedBrowser {
        name: "Mozilla Firefox".to_string(),
        executable_path: PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe"),
        version: Some("120.0".to_string()),
        architecture: BrowserArchitecture::X64,
    };
    assert!(detected_firefox.is_verified());
}

#[test]
fn test_is_brave_executable_and_detected_browser_is_verified() {
    assert!(is_brave_executable(&PathBuf::from("brave.exe")));
    assert!(is_brave_executable(&PathBuf::from("brave")));
    assert!(is_brave_executable(&PathBuf::from(
        r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe"
    )));
    assert!(!is_brave_executable(&PathBuf::from("msedge.exe")));
    assert!(!is_brave_executable(&PathBuf::from("chrome.exe")));
    assert!(!is_brave_executable(&PathBuf::from("firefox.exe")));

    let detected_brave = DetectedBrowser {
        name: "Brave Browser".to_string(),
        executable_path: PathBuf::from(
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
        ),
        version: Some("1.62.153".to_string()),
        architecture: BrowserArchitecture::X64,
    };
    assert!(detected_brave.is_verified());
}

#[test]
fn test_unverified_browser_is_rejected_with_informative_reason() {
    // Brave is verified on simulated Windows or valid PE
    let brave_path =
        PathBuf::from(r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe");
    let support_brave = detect_browser_support(&brave_path);
    match support_brave {
        BrowserSupportStatus::Verified(adapter) => {
            assert_eq!(adapter.browser_name(), "Brave Browser");
        }
        BrowserSupportStatus::Unverified => panic!("Brave must be verified"),
    }

    // Opera or custom browser remains unverified
    let opera_path = PathBuf::from(r"C:\Program Files\Opera\opera.exe");
    let support_opera = detect_browser_support(&opera_path);
    assert_eq!(support_opera, BrowserSupportStatus::Unverified);

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

    // 5. Verified: Firefox x64
    let firefox_exe = create_mock_pe_file(temp_dir.path(), "firefox.exe", 0x8664);
    let status_firefox = evaluate_browser_compatibility(&firefox_exe);
    match status_firefox {
        BrowserCompatibilityStatus::SafeToActivate { adapter, evidence } => {
            assert_eq!(adapter.browser_name(), "Mozilla Firefox");
            assert_eq!(evidence.browser_name, "Mozilla Firefox");
            assert_eq!(evidence.architecture, BrowserArchitecture::X64);
            assert!(evidence.is_safe_to_activate);
            assert!(evidence.evidence_text.contains("-no-remote -profile"));
            assert!(evidence.evidence_text.contains("Windows 10/11 x64"));
            assert!(evidence.limitations.as_ref().unwrap().contains("privat"));
        }
        other => panic!("expected SafeToActivate for Firefox x64, got {:?}", other),
    }

    // 5b. Verified: Firefox x86
    let firefox_x86 = create_mock_pe_file(temp_dir.path(), "firefox_x86.exe", 0x014c);
    let firefox_x86_named = temp_dir.path().join("firefox_32").join("firefox.exe");
    std::fs::create_dir_all(firefox_x86_named.parent().expect("parent dir"))
        .expect("create dir firefox_32");
    std::fs::copy(&firefox_x86, &firefox_x86_named).expect("copy firefox_x86");
    let status_firefox_x86 = evaluate_browser_compatibility(&firefox_x86_named);
    match status_firefox_x86 {
        BrowserCompatibilityStatus::SafeToActivate { evidence, .. } => {
            assert_eq!(evidence.architecture, BrowserArchitecture::X86);
            assert!(evidence.is_safe_to_activate);
        }
        other => panic!("expected SafeToActivate for Firefox x86, got {:?}", other),
    }

    // 5c. Blocked: Firefox with corrupt/non-PE binary on disk (unknown architecture)
    let corrupt_firefox = temp_dir.path().join("corrupt_firefox").join("firefox.exe");
    std::fs::create_dir_all(corrupt_firefox.parent().expect("parent dir"))
        .expect("create dir corrupt_firefox");
    std::fs::write(&corrupt_firefox, b"not a valid pe header").expect("write corrupt header");
    let status_corrupt_firefox = evaluate_browser_compatibility(&corrupt_firefox);
    match status_corrupt_firefox {
        BrowserCompatibilityStatus::Unverified { evidence } => {
            assert_eq!(evidence.browser_name, "Mozilla Firefox");
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
            "expected Unverified for corrupt Firefox binary, got {:?}",
            other
        ),
    }

    // 5d. Verified: Brave x64
    let brave_exe = create_mock_pe_file(temp_dir.path(), "brave.exe", 0x8664);
    let status_brave = evaluate_browser_compatibility(&brave_exe);
    match status_brave {
        BrowserCompatibilityStatus::SafeToActivate { adapter, evidence } => {
            assert_eq!(adapter.browser_name(), "Brave Browser");
            assert_eq!(evidence.browser_name, "Brave Browser");
            assert_eq!(evidence.architecture, BrowserArchitecture::X64);
            assert!(evidence.is_safe_to_activate);
            assert!(evidence.evidence_text.contains("user-data-dir"));
            assert!(evidence.evidence_text.contains("Windows 10/11 x64"));
            assert!(
                evidence.limitations.as_ref().unwrap().contains("incognito")
                    || evidence.limitations.as_ref().unwrap().contains("Tor")
            );
        }
        other => panic!("expected SafeToActivate for Brave x64, got {:?}", other),
    }

    // 5d-2. Verified: Brave x86
    let brave_x86 = create_mock_pe_file(temp_dir.path(), "brave_x86.exe", 0x014c);
    let brave_x86_named = temp_dir.path().join("brave_32").join("brave.exe");
    std::fs::create_dir_all(brave_x86_named.parent().expect("parent dir"))
        .expect("create dir brave_32");
    std::fs::copy(&brave_x86, &brave_x86_named).expect("copy brave_x86");
    let status_brave_x86 = evaluate_browser_compatibility(&brave_x86_named);
    match status_brave_x86 {
        BrowserCompatibilityStatus::SafeToActivate { evidence, .. } => {
            assert_eq!(evidence.architecture, BrowserArchitecture::X86);
            assert!(evidence.is_safe_to_activate);
        }
        other => panic!("expected SafeToActivate for Brave x86, got {:?}", other),
    }

    // 5d-3. Blocked: Brave with corrupt/non-PE binary on disk (unknown architecture)
    let corrupt_brave = temp_dir.path().join("corrupt_brave").join("brave.exe");
    std::fs::create_dir_all(corrupt_brave.parent().expect("parent dir"))
        .expect("create dir corrupt_brave");
    std::fs::write(&corrupt_brave, b"not a valid pe header").expect("write corrupt brave header");
    let status_corrupt_brave = evaluate_browser_compatibility(&corrupt_brave);
    match status_corrupt_brave {
        BrowserCompatibilityStatus::Unverified { evidence } => {
            assert_eq!(evidence.browser_name, "Brave Browser");
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
            "expected Unverified for corrupt Brave binary, got {:?}",
            other
        ),
    }

    // 5e. Unverified: Custom portable executable
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

    // 6b. Installed Firefox with sibling application.ini version file
    let firefox_app_dir = temp_dir.path().join("firefox_versioned");
    std::fs::create_dir_all(&firefox_app_dir).expect("create firefox versioned dir");
    let app_ini = firefox_app_dir.join("application.ini");
    std::fs::write(
        &app_ini,
        "[App]\nVendor=Mozilla\nName=Firefox\nVersion=122.0.1\nBuildID=20240101\n",
    )
    .expect("write application.ini");
    let firefox_versioned_exe = create_mock_pe_file(&firefox_app_dir, "firefox.exe", 0x8664);
    let status_firefox_versioned = evaluate_browser_compatibility(&firefox_versioned_exe);
    match status_firefox_versioned {
        BrowserCompatibilityStatus::SafeToActivate { evidence, .. } => {
            assert_eq!(evidence.version, Some("122.0.1".to_string()));
            assert_eq!(evidence.architecture, BrowserArchitecture::X64);
            assert!(evidence.is_safe_to_activate);
        }
        other => panic!(
            "expected SafeToActivate with Firefox version, got {:?}",
            other
        ),
    }

    // 6c. Installed browser with sibling version folder (e.g. Edge Application\120.0.0.0)
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

#[test]
fn test_smart_bidirectional_parser_extracts_app_url_and_kiosk() {
    // 1. Standard Chrome / Edge format
    let p = LaunchParameters::parse("--app=https://antrian.poli/gigi --kiosk").unwrap();
    assert_eq!(p.url, "https://antrian.poli/gigi");
    assert!(p.kiosk);
    assert_eq!(p.advanced_parameters, "");

    // 2. Reversed order
    let p2 = LaunchParameters::parse("--kiosk --app=https://queue.local").unwrap();
    assert_eq!(p2.url, "https://queue.local");
    assert!(p2.kiosk);
    assert_eq!(p2.advanced_parameters, "");

    // 3. Quoted URL with spaces
    let p3 = LaunchParameters::parse(r#"--app="https://antrian.poli/display 1" --kiosk"#).unwrap();
    assert_eq!(p3.url, "https://antrian.poli/display 1");
    assert!(p3.kiosk);
    assert_eq!(p3.advanced_parameters, "");

    // 4. Two-token --app argument
    let p4 = LaunchParameters::parse("--app https://antrian.poli/umum").unwrap();
    assert_eq!(p4.url, "https://antrian.poli/umum");
    assert!(!p4.kiosk);
    assert_eq!(p4.advanced_parameters, "");

    // 5. Raw URL without --app flag
    let p5 = LaunchParameters::parse("https://antrian.rumahsakit.local/poli --kiosk").unwrap();
    assert_eq!(p5.url, "https://antrian.rumahsakit.local/poli");
    assert!(p5.kiosk);
    assert_eq!(p5.advanced_parameters, "");

    // 6. Only kiosk flag
    let p6 = LaunchParameters::parse("--kiosk").unwrap();
    assert_eq!(p6.url, "");
    assert!(p6.kiosk);
    assert_eq!(p6.advanced_parameters, "");

    // 7. Empty string
    let p7 = LaunchParameters::parse("").unwrap();
    assert_eq!(p7.url, "");
    assert!(!p7.kiosk);
    assert_eq!(p7.advanced_parameters, "");
}

#[test]
fn test_smart_bidirectional_parser_preserves_advanced_parameters() {
    // Advanced parameters are kept intact
    let p = LaunchParameters::parse("--app=https://antrian.local --kiosk --disable-gpu --lang=id")
        .unwrap();
    assert_eq!(p.url, "https://antrian.local");
    assert!(p.kiosk);
    assert_eq!(p.advanced_parameters, "--disable-gpu --lang=id");

    // Interleaved order preserves non-app and non-kiosk flags
    let p2 = LaunchParameters::parse(
        "--disable-features=Translate --kiosk --app=https://antrian.local --incognito",
    )
    .unwrap();
    assert_eq!(p2.url, "https://antrian.local");
    assert!(p2.kiosk);
    assert_eq!(
        p2.advanced_parameters,
        "--disable-features=Translate --incognito"
    );

    // Quoted values in advanced parameters
    let p3 = LaunchParameters::parse(
        r#"--app=https://antrian.local --custom-flag="hello world" --kiosk"#,
    )
    .unwrap();
    assert_eq!(p3.url, "https://antrian.local");
    assert!(p3.kiosk);
    assert_eq!(p3.advanced_parameters, r#"--custom-flag="hello world""#);
}

#[test]
fn test_smart_bidirectional_parser_roundtrip_composition() {
    // 1. Standard compose
    let lp = LaunchParameters::new("https://antrian.poli/gigi", true, "--disable-gpu");
    let composed = lp.compose();
    assert_eq!(
        composed,
        "--app=https://antrian.poli/gigi --kiosk --disable-gpu"
    );
    let reparsed = LaunchParameters::parse(&composed).unwrap();
    assert_eq!(reparsed, lp);

    // 2. URL with space requires quotes
    let lp2 = LaunchParameters::new("https://antrian.poli/display 1", false, "");
    let composed2 = lp2.compose();
    assert_eq!(composed2, r#"--app="https://antrian.poli/display 1""#);
    let reparsed2 = LaunchParameters::parse(&composed2).unwrap();
    assert_eq!(reparsed2, lp2);

    // 3. No URL, only kiosk and advanced
    let lp3 = LaunchParameters::new("", true, "--inprivate");
    let composed3 = lp3.compose();
    assert_eq!(composed3, "--kiosk --inprivate");
    let reparsed3 = LaunchParameters::parse(&composed3).unwrap();
    assert_eq!(reparsed3, lp3);

    // 4. Empty
    let lp4 = LaunchParameters::new("", false, "");
    assert_eq!(lp4.compose(), "");
    assert_eq!(LaunchParameters::parse("").unwrap(), lp4);

    // 5. Firefox adapter compose does not prepend --app=
    let firefox_adapter = BrowserAdapter::Firefox(FirefoxAdapter::new());
    let lp_ff = LaunchParameters::new("https://antrian.poli/mata", true, "--private-window");
    let ff_composed = lp_ff.compose_for(Some(&firefox_adapter));
    assert_eq!(
        ff_composed,
        "https://antrian.poli/mata --kiosk --private-window"
    );
    assert!(!ff_composed.contains("--app="));
}

#[test]
fn test_smart_bidirectional_parser_rejects_conflicting_isolation_parameters() {
    // Chromium conflicting isolation arguments rejected
    let lp = LaunchParameters::new("https://antrian.local", true, "--user-data-dir=C:\\custom");
    let err = lp.validate_isolation(None).unwrap_err();
    match err {
        BrowserError::ConflictingIsolationParameter { arg, .. } => {
            assert!(arg.contains("--user-data-dir"));
        }
        other => panic!("expected ConflictingIsolationParameter, got: {:?}", other),
    }

    // Conflicting isolation arguments in URL field are also rejected
    let lp_url_attack = LaunchParameters::new(
        "https://antrian.local --user-data-dir=C:\\custom",
        false,
        "",
    );
    assert!(lp_url_attack.validate_isolation(None).is_err());

    // Profile-directory rejected
    let lp2 = LaunchParameters::new(
        "https://antrian.local",
        false,
        r#"--profile-directory="Profile 1""#,
    );
    assert!(lp2.validate_isolation(None).is_err());

    // Firefox conflicting arguments rejected
    let lp3 = LaunchParameters::new(
        "https://antrian.local",
        true,
        "-profile C:\\firefox_profile",
    );
    assert!(lp3.validate_isolation(None).is_err());

    // Safe parameters pass validation
    let lp_ok = LaunchParameters::new("https://antrian.local", true, "--disable-gpu --lang=id");
    assert!(lp_ok.validate_isolation(None).is_ok());
}
