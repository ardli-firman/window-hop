use std::path::PathBuf;
use window_hop::browsers::{
    detect_browser_support, parse_windows_command_line, BrowserError, BrowserSupportStatus,
    EdgeAdapter, LoginRetentionNote,
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
fn test_unverified_browser_is_rejected_with_informative_reason() {
    let chrome_path = PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe");
    let support_chrome = detect_browser_support(&chrome_path);
    assert_eq!(support_chrome, BrowserSupportStatus::Unverified);

    let unknown_path = PathBuf::from(r"D:\Portable\CustomBrowser.exe");
    let support_unknown = detect_browser_support(&unknown_path);
    assert_eq!(support_unknown, BrowserSupportStatus::Unverified);
}
