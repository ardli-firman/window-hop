use std::fs;
use std::path::{Path, PathBuf};
use window_hop::application::LifecycleCoordinator;
use window_hop::domain::DraftProfileInput;
use window_hop::installer::{
    default_install_dir, install, is_dev_build_path, resolve_installed_app_executable, uninstall,
    validate_os_baseline, verify_installation, Arch, InstallError, InstallOptions, OsVersion,
};
use window_hop::persistence::FileProfileStore;
use window_hop::platform::{MockShortcutCreator, MockStartupManager};

#[test]
fn test_default_installation_directory_is_consistent() {
    let dir = default_install_dir();
    assert!(!dir.as_os_str().is_empty());
    let dir_str = dir.to_string_lossy();
    assert!(
        dir_str.ends_with("Window Hop") || dir_str.ends_with("window-hop"),
        "Default install directory should end with Window Hop, got: {}",
        dir_str
    );
}

#[test]
fn test_is_dev_build_path_detects_target_debug_and_release() {
    assert!(is_dev_build_path(Path::new(
        "C:\\projects\\window-hop\\target\\debug\\window-hop.exe"
    )));
    assert!(is_dev_build_path(Path::new(
        "C:\\projects\\window-hop\\target\\release\\window-hop.exe"
    )));
    assert!(is_dev_build_path(Path::new(
        "C:\\Projects\\Window-Hop\\Target\\Debug\\window-hop.exe"
    )));
    assert!(is_dev_build_path(Path::new(
        "C:\\REPO\\TARGET\\RELEASE\\window-hop.exe"
    )));
    assert!(is_dev_build_path(Path::new(
        "/home/user/window-hop/target/debug/window-hop"
    )));
    assert!(is_dev_build_path(Path::new(
        "/home/user/window-hop/target/release/window-hop"
    )));

    assert!(!is_dev_build_path(Path::new(
        "C:\\Users\\Perawat\\AppData\\Local\\Programs\\Window Hop\\window-hop.exe"
    )));
    assert!(!is_dev_build_path(Path::new(
        "C:\\Program Files\\Window Hop\\window-hop.exe"
    )));
    assert!(!is_dev_build_path(Path::new("/usr/local/bin/window-hop")));
    assert!(!is_dev_build_path(Path::new(
        "D:\\CustomApp\\window-hop.exe"
    )));
}

#[test]
fn test_validate_os_baseline_accepts_supported_windows_x64() {
    // Windows 10 x64 Build 19045 (22H2)
    let win10 = OsVersion {
        major: 10,
        minor: 0,
        build: 19045,
        arch: Arch::X64,
    };
    assert!(validate_os_baseline(&win10).is_ok());

    // Windows 11 x64 Build 22631 (23H2)
    let win11 = OsVersion {
        major: 10,
        minor: 0,
        build: 22631,
        arch: Arch::X64,
    };
    assert!(validate_os_baseline(&win11).is_ok());
}

#[test]
fn test_validate_os_baseline_rejects_32bit_windows() {
    let win10_32 = OsVersion {
        major: 10,
        minor: 0,
        build: 19045,
        arch: Arch::X86,
    };
    let err = validate_os_baseline(&win10_32).unwrap_err();
    match err {
        InstallError::UnsupportedArchitecture { found } => {
            assert_eq!(found, "x86 (32-bit)");
        }
        other => panic!("Expected UnsupportedArchitecture, got {:?}", other),
    }
}

#[test]
fn test_validate_os_baseline_rejects_older_windows_versions() {
    // Windows 7 SP1 x64 (build 7601)
    let win7 = OsVersion {
        major: 6,
        minor: 1,
        build: 7601,
        arch: Arch::X64,
    };
    let err = validate_os_baseline(&win7).unwrap_err();
    match err {
        InstallError::UnsupportedOsVersion { found, minimum } => {
            assert!(found.contains("6.1.7601"));
            assert!(minimum.contains("10.0.19041"));
        }
        other => panic!("Expected UnsupportedOsVersion, got {:?}", other),
    }

    // Windows 10 older than build 19041 (e.g. build 17763 / 1809)
    let win10_old = OsVersion {
        major: 10,
        minor: 0,
        build: 17763,
        arch: Arch::X64,
    };
    let err = validate_os_baseline(&win10_old).unwrap_err();
    match err {
        InstallError::UnsupportedOsVersion { found, minimum } => {
            assert!(found.contains("10.0.17763"));
            assert!(minimum.contains("10.0.19041"));
        }
        other => panic!("Expected UnsupportedOsVersion, got {:?}", other),
    }
}

#[test]
fn test_install_copies_executable_resources_and_creates_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let source_dir = temp.path().join("source");
    fs::create_dir_all(&source_dir).unwrap();
    let source_exe = source_dir.join("window-hop.exe");
    fs::write(&source_exe, b"MZ_FAKE_PE_EXECUTABLE_BINARY_DATA").unwrap();

    let target_dir = temp.path().join("installed_window_hop");

    let options = InstallOptions {
        install_dir: target_dir.clone(),
        source_exe: source_exe.clone(),
        embedded_payload: None,
        bundle_resources: true,
        create_start_menu_shortcut: false,
        create_desktop_shortcut: false,
        register_uninstaller: false,
    };

    let receipt = install(&options).expect("Installation should succeed");
    assert_eq!(receipt.install_dir, target_dir);
    assert_eq!(receipt.installed_exe, target_dir.join("window-hop.exe"));
    assert!(receipt.installed_exe.exists());

    // Verify bundle resources are present
    let guide_path = target_dir.join("INSTALL-GUIDE.txt");
    assert!(guide_path.exists(), "INSTALL-GUIDE.txt must be bundled");
    let guide_content = fs::read_to_string(&guide_path).unwrap();
    assert!(guide_content.contains("Windows 10"));
    assert!(guide_content.contains("Direct3D 11"));

    // Verify install-manifest.json
    let manifest_path = target_dir.join("install-manifest.json");
    assert!(
        manifest_path.exists(),
        "install-manifest.json must be created"
    );
    let manifest_raw = fs::read_to_string(&manifest_path).unwrap();
    assert!(
        manifest_raw.contains("installed_at")
            && manifest_raw.contains("T")
            && manifest_raw.contains("Z"),
        "Manifest must include a valid ISO-8601 UTC timestamp"
    );

    // Verify verify_installation succeeds
    let verified = verify_installation(&target_dir).expect("Verification should succeed");
    assert_eq!(verified.installed_exe, receipt.installed_exe);
}

#[test]
fn test_install_fails_when_source_payload_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let missing_exe = temp.path().join("non_existent_window_hop.exe");
    let target_dir = temp.path().join("target");

    let options = InstallOptions {
        install_dir: target_dir,
        source_exe: missing_exe.clone(),
        embedded_payload: None,
        bundle_resources: true,
        create_start_menu_shortcut: false,
        create_desktop_shortcut: false,
        register_uninstaller: false,
    };

    let err = install(&options).unwrap_err();
    match err {
        InstallError::MissingPayload(p) => assert_eq!(p, missing_exe),
        other => panic!("Expected MissingPayload, got {:?}", other),
    }
}

#[test]
fn test_verify_installation_detects_missing_executable() {
    let temp = tempfile::tempdir().unwrap();
    let source_dir = temp.path().join("source");
    fs::create_dir_all(&source_dir).unwrap();
    let source_exe = source_dir.join("window-hop.exe");
    fs::write(&source_exe, b"MZ_FAKE_PE").unwrap();

    let target_dir = temp.path().join("installed");
    let options = InstallOptions {
        install_dir: target_dir.clone(),
        source_exe,
        embedded_payload: None,
        bundle_resources: true,
        create_start_menu_shortcut: false,
        create_desktop_shortcut: false,
        register_uninstaller: false,
    };

    install(&options).unwrap();

    // Now remove the executable
    fs::remove_file(target_dir.join("window-hop.exe")).unwrap();

    let err = verify_installation(&target_dir).unwrap_err();
    match err {
        InstallError::VerificationFailed(msg) => {
            assert!(msg.contains("Executable tidak ditemukan"));
        }
        other => panic!("Expected VerificationFailed, got {:?}", other),
    }
}

#[test]
fn test_resolve_installed_app_executable_prefers_installed_location_over_dev_build() {
    let temp = tempfile::tempdir().unwrap();
    let install_dir = temp.path().join("Window Hop");
    fs::create_dir_all(&install_dir).unwrap();
    let installed_exe = install_dir.join("window-hop.exe");
    fs::write(&installed_exe, b"MZ_INSTALLED").unwrap();

    let dev_build_exe = temp
        .path()
        .join("target")
        .join("debug")
        .join("window-hop.exe");
    fs::create_dir_all(dev_build_exe.parent().unwrap()).unwrap();
    fs::write(&dev_build_exe, b"MZ_DEV_BUILD").unwrap();

    // When passing dev_build_exe and installed_dir
    let resolved = resolve_installed_app_executable(Some(&dev_build_exe), Some(&install_dir));
    assert_eq!(
        resolved, installed_exe,
        "Should resolve to installed executable, not the dev build"
    );
}

#[test]
fn test_resolve_installed_app_executable_preserves_custom_installed_path() {
    let temp = tempfile::tempdir().unwrap();
    let custom_install_dir = temp.path().join("CustomDrive").join("Hop");
    fs::create_dir_all(&custom_install_dir).unwrap();
    let custom_exe = custom_install_dir.join("window-hop.exe");
    fs::write(&custom_exe, b"MZ_CUSTOM_INSTALLED").unwrap();

    // When running from custom installed path (which is NOT a target/debug dev build)
    let resolved = resolve_installed_app_executable(Some(&custom_exe), None);
    assert_eq!(
        resolved, custom_exe,
        "Should preserve custom installation directory when running from it"
    );
}

#[test]
fn test_create_profile_shortcut_and_startup_use_installed_application_location() {
    let temp = tempfile::tempdir().unwrap();
    let install_dir = temp.path().join("installed_app");
    fs::create_dir_all(&install_dir).unwrap();
    let installed_exe = install_dir.join("window-hop.exe");
    fs::write(&installed_exe, b"MZ_INSTALLED_APP").unwrap();

    let config_path = temp.path().join("profiles.json");
    let store = std::sync::Arc::new(FileProfileStore::new(config_path));
    let mut coord = LifecycleCoordinator::from_store(store);

    let desktop_dir = temp.path().join("Desktop");
    let shortcut_creator = std::sync::Arc::new(MockShortcutCreator::new(desktop_dir.clone()));
    coord.set_shortcut_creator(shortcut_creator.clone());

    let startup_manager = std::sync::Arc::new(MockStartupManager::new(false));
    coord.set_startup_manager(startup_manager.clone());

    // Set the resolved application executable to the installed executable
    coord.set_app_executable(installed_exe.clone());

    // 1. Create a profile
    let profile_id = coord
        .create_draft_profile(DraftProfileInput::new(
            "Antrian Poli A",
            PathBuf::from(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
            "--kiosk https://antrian.clinic.local",
        ))
        .expect("Profile creation should succeed");

    // 2. Create profile shortcut
    let shortcut_path = coord
        .create_profile_shortcut(&profile_id, None)
        .expect("Shortcut creation should succeed");
    assert!(shortcut_path
        .to_string_lossy()
        .contains("Antrian Poli A.lnk"));

    let created_shortcuts = shortcut_creator.created_shortcuts();
    assert_eq!(created_shortcuts.len(), 1);
    let spec = &created_shortcuts[0];
    assert_eq!(
        spec.app_executable, installed_exe,
        "Shortcut must point to the installed application, NOT dev build folder"
    );
    assert_eq!(
        spec.arguments,
        vec!["--activate-profile", profile_id.as_str()]
    );

    // 3. Set startup profile and enable startup
    coord.set_startup_profile_id(Some(profile_id)).unwrap();
    coord.set_startup_enabled(true).unwrap();

    assert!(startup_manager.is_registered());
    assert_eq!(
        startup_manager.registered_executable(),
        Some(installed_exe),
        "Startup registration must use the installed application, NOT dev build folder"
    );
}

#[test]
fn test_install_with_embedded_payload_extracts_executable_without_source_file() {
    let temp = tempfile::tempdir().unwrap();
    let target_dir = temp.path().join("bundled_install");
    let non_existent_source = temp.path().join("does_not_exist.exe");

    let embedded_bytes: &'static [u8] = b"MZ_EMBEDDED_PAYLOAD_TEST_DATA_12345";

    let options = InstallOptions {
        install_dir: target_dir.clone(),
        source_exe: non_existent_source,
        embedded_payload: Some(embedded_bytes),
        bundle_resources: true,
        create_start_menu_shortcut: false,
        create_desktop_shortcut: false,
        register_uninstaller: false,
    };

    let receipt = install(&options).expect("Installation with embedded payload should succeed");
    assert_eq!(receipt.install_dir, target_dir);
    let installed_exe = target_dir.join("window-hop.exe");
    assert!(installed_exe.exists());

    let written_bytes = fs::read(&installed_exe).unwrap();
    assert_eq!(written_bytes, embedded_bytes);

    // Verify manifest was also written properly
    let manifest_path = target_dir.join("install-manifest.json");
    assert!(manifest_path.exists());
}

#[test]
fn test_uninstall_removes_installed_files_and_cleans_directory() {
    let temp = tempfile::tempdir().unwrap();
    let target_dir = temp.path().join("to_uninstall");

    let options = InstallOptions {
        install_dir: target_dir.clone(),
        source_exe: PathBuf::from("dummy.exe"),
        embedded_payload: Some(b"MZ_PAYLOAD_TO_UNINSTALL"),
        bundle_resources: true,
        create_start_menu_shortcut: false,
        create_desktop_shortcut: false,
        register_uninstaller: false,
    };

    install(&options).expect("Initial install should succeed");
    assert!(target_dir.join("window-hop.exe").exists());
    assert!(target_dir.join("INSTALL-GUIDE.txt").exists());
    assert!(target_dir.join("install-manifest.json").exists());

    let receipt = uninstall(&target_dir).expect("Uninstall should succeed");
    assert!(!target_dir.join("window-hop.exe").exists());
    assert!(!target_dir.join("INSTALL-GUIDE.txt").exists());
    assert!(!target_dir.join("install-manifest.json").exists());
    assert!(
        !target_dir.exists(),
        "Empty install directory should be removed"
    );
    assert!(receipt.success);
    assert!(receipt.uninstalled_files.len() >= 3);
}
