use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use window_hop::browsers::{LaunchPlan, LoginRetentionNote};
use window_hop::domain::{MonitorDescriptor, MonitorIdentity, ProcessId, ProfileId};
use window_hop::platform::{
    sanitize_shortcut_filename, ActivationCommand, Clock, LocalSingleInstanceCoordinator,
    MockClock, MockMonitorProvider, MockProcessLauncher, MockSingleInstanceCoordinator,
    MonitorProvider, PlatformError, ProcessLauncher, SingleInstanceCoordinator,
};

#[test]
fn test_mock_monitor_provider_returns_configured_monitors() {
    let m1 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-A"),
        name: "Monitor A".to_string(),
        is_primary: true,
    };
    let m2 = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-B"),
        name: "Monitor B".to_string(),
        is_primary: false,
    };

    let provider = MockMonitorProvider::new(vec![m1.clone(), m2.clone()]);
    let monitors = provider
        .available_monitors()
        .expect("must return available monitors");
    assert_eq!(monitors.len(), 2);
    assert_eq!(monitors[0], m1);
    assert_eq!(monitors[1], m2);

    // Simulate unplugging one monitor and changing order
    provider.set_monitors(vec![m2.clone()]);
    let updated = provider.available_monitors().expect("updated monitors");
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0], m2);
}

#[test]
fn test_mock_monitor_provider_can_simulate_platform_errors() {
    let provider = MockMonitorProvider::new(vec![]);
    provider.set_error(Some("Gagal enumerasi display".to_string()));

    let res = provider.available_monitors();
    assert!(res.is_err());
    match res.unwrap_err() {
        PlatformError::Os(msg) => assert!(msg.contains("Gagal enumerasi display")),
        other => panic!("expected PlatformError::Os, got {:?}", other),
    }
}

#[test]
fn test_mock_process_launcher_and_managed_process() {
    let launcher = MockProcessLauncher::new();
    let monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Display".to_string(),
        is_primary: false,
    };
    let plan = LaunchPlan {
        executable: PathBuf::from("msedge.exe"),
        arguments: vec!["--user-data-dir=data".to_string(), "--kiosk".to_string()],
        user_data_dir: PathBuf::from("data"),
        login_retention: LoginRetentionNote::KioskOrInPrivateNoRetention {
            explanation: "test".to_string(),
        },
    };

    let process = launcher.launch(&plan, &monitor).expect("launch process");
    assert_eq!(process.pid(), ProcessId::new(1001));
    assert!(process.is_alive());

    // Reposition to a new monitor
    let other_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-OTHER"),
        name: "Other Display".to_string(),
        is_primary: true,
    };
    process.reposition(&other_monitor).expect("reposition");

    // Graceful close (proses kooperatif berhenti)
    process.request_close().expect("request close");
    assert!(!process.is_alive());

    // Force stop (proses yang macet atau butuh force kill)
    let proc2 = launcher.launch(&plan, &monitor).expect("launch proc2");
    assert!(proc2.is_alive());
    proc2.force_stop().expect("force stop");
    assert!(!proc2.is_alive());
}

#[test]
fn test_mock_clock_advancement() {
    let clock = MockClock::new();
    let t0 = clock.now();
    clock.advance(Duration::from_secs(3));
    let t1 = clock.now();
    assert_eq!(t1.duration_since(t0), Duration::from_secs(3));
}

#[test]
fn test_sanitize_shortcut_filename_removes_invalid_characters() {
    assert_eq!(
        sanitize_shortcut_filename("Antrian: Poli/Gigi* & Mulut?"),
        "Antrian_ Poli_Gigi_ & Mulut_"
    );
    assert_eq!(sanitize_shortcut_filename("  "), "Profil Window Hop");
}

#[test]
fn test_mock_single_instance_coordinator_lifecycle() {
    let coordinator = Arc::new(MockSingleInstanceCoordinator::new());
    let target_profile = ProfileId::generate();

    // 1. Initial attempt: no instance is running -> returns Ok(false)
    let command = ActivationCommand::ActivateProfile(target_profile.clone());
    let sent = coordinator
        .send_activation(&command)
        .expect("send activation before listener");
    assert!(
        !sent,
        "Should return false when primary instance is not yet running"
    );

    // 2. Start listener
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();
    coordinator
        .start_listener(Box::new(move |_cmd| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        }))
        .expect("start listener");

    // 3. Second attempt: now primary is listening -> returns Ok(true) and increments counter
    let sent = coordinator
        .send_activation(&command)
        .expect("send activation after listener");
    assert!(
        sent,
        "Should return true when primary instance is listening"
    );
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    // 4. Repeated invocation
    let sent2 = coordinator
        .send_activation(&command)
        .expect("second repeated activation");
    assert!(sent2);
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

#[test]
fn test_local_single_instance_coordinator_with_real_sockets() {
    let dir = tempdir().expect("tempdir");
    let primary = LocalSingleInstanceCoordinator::new(dir.path().to_path_buf());
    let profile_id = ProfileId::generate();
    let command = ActivationCommand::ActivateProfile(profile_id.clone());

    // Before primary is listening:
    let sent = primary.send_activation(&command).expect("initial send");
    assert!(!sent, "Before listening, should report no instance running");

    // Start primary listener:
    let received = Arc::new(std::sync::Mutex::new(Vec::new()));
    let received_clone = received.clone();
    primary
        .start_listener(Box::new(move |cmd| {
            received_clone.lock().unwrap().push(cmd);
        }))
        .expect("start listener");

    // Simulate secondary instance in another process / client:
    let secondary = LocalSingleInstanceCoordinator::new(dir.path().to_path_buf());
    let sent = secondary
        .send_activation(&command)
        .expect("send from secondary");
    assert!(
        sent,
        "Secondary must successfully deliver command to primary"
    );

    // Wait briefly for IPC handler to record command
    std::thread::sleep(Duration::from_millis(100));
    let cmds = received.lock().unwrap().clone();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0], command);

    // Send repeated click from secondary
    let sent2 = secondary
        .send_activation(&command)
        .expect("send repeated click");
    assert!(sent2);

    std::thread::sleep(Duration::from_millis(100));
    let cmds2 = received.lock().unwrap().clone();
    assert_eq!(cmds2.len(), 2);
}

#[test]
fn test_parse_activation_flag_extracts_profile_id_when_present() {
    use window_hop::platform::parse_activation_flag;

    let args = vec![
        "window-hop.exe".to_string(),
        "--activate-profile".to_string(),
        "PROF-POLI-123".to_string(),
    ];
    let parsed = parse_activation_flag(args);
    assert_eq!(parsed, Some(ProfileId::new("PROF-POLI-123")));

    let args_without_flag = vec!["window-hop.exe".to_string()];
    assert_eq!(parse_activation_flag(args_without_flag), None);

    let args_trailing_flag = vec![
        "window-hop.exe".to_string(),
        "--activate-profile".to_string(),
    ];
    assert_eq!(parse_activation_flag(args_trailing_flag), None);
}

#[test]
fn test_parse_silent_or_startup_flag() {
    use window_hop::platform::parse_silent_or_startup_flag;

    assert!(parse_silent_or_startup_flag(vec![
        "window-hop.exe".to_string(),
        "--startup".to_string(),
    ]));

    assert!(parse_silent_or_startup_flag(vec![
        "window-hop.exe".to_string(),
        "--silent".to_string(),
    ]));

    assert!(parse_silent_or_startup_flag(vec![
        "window-hop.exe".to_string(),
        "--tray".to_string(),
    ]));

    assert!(!parse_silent_or_startup_flag(vec![
        "window-hop.exe".to_string(),
    ]));

    assert!(!parse_silent_or_startup_flag(vec![
        "window-hop.exe".to_string(),
        "--other-flag".to_string(),
    ]));
}

#[test]
fn test_mock_tray_provider_updates_status_and_triggers_actions() {
    use window_hop::domain::TrayStatusSummary;
    use window_hop::platform::{MockTrayProvider, TrayAction, TrayProvider};

    let tray = MockTrayProvider::new();
    assert_eq!(tray.update_count(), 0);
    assert!(!tray.is_removed());
    assert_eq!(tray.last_status(), None);

    let summary = TrayStatusSummary::new(
        "Profil 'Antrian Poli' berjalan",
        "Window Hop\nProfil 'Antrian Poli' berjalan",
        1,
        0,
        false,
        None,
    );

    tray.update_status(&summary).expect("update status");
    assert_eq!(tray.update_count(), 1);
    assert_eq!(tray.last_status(), Some(summary));

    let recorded_actions = Arc::new(std::sync::Mutex::new(Vec::new()));
    let actions_clone = recorded_actions.clone();

    tray.set_action_handler(Box::new(move |action| {
        actions_clone.lock().unwrap().push(action);
    }))
    .expect("set action handler");

    tray.trigger_action(TrayAction::OpenSettings);
    tray.trigger_action(TrayAction::Exit);

    let actions = recorded_actions.lock().unwrap().clone();
    assert_eq!(actions, vec![TrayAction::OpenSettings, TrayAction::Exit]);

    tray.remove_tray().expect("remove tray");
    assert!(tray.is_removed());
}

#[test]
fn test_local_single_instance_coordinator_handles_open_settings_command() {
    let dir = tempdir().expect("tempdir");
    let primary = LocalSingleInstanceCoordinator::new(dir.path().to_path_buf());

    let received = Arc::new(std::sync::Mutex::new(Vec::new()));
    let received_clone = received.clone();
    primary
        .start_listener(Box::new(move |cmd| {
            received_clone.lock().unwrap().push(cmd);
        }))
        .expect("start listener");

    let secondary = LocalSingleInstanceCoordinator::new(dir.path().to_path_buf());
    let sent = secondary
        .send_activation(&ActivationCommand::OpenSettings)
        .expect("send OpenSettings");
    assert!(sent, "OpenSettings must be delivered to primary");

    std::thread::sleep(Duration::from_millis(100));
    let cmds = received.lock().unwrap().clone();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0], ActivationCommand::OpenSettings);
}

#[test]
fn test_local_single_instance_coordinator_handles_ping_command() {
    let dir = tempdir().expect("tempdir");
    let primary = LocalSingleInstanceCoordinator::new(dir.path().to_path_buf());

    // Without primary listener: ping returns false (no running instance)
    let ping_before = primary
        .send_activation(&ActivationCommand::Ping)
        .expect("ping before start");
    assert!(!ping_before);

    let received = Arc::new(std::sync::Mutex::new(Vec::new()));
    let received_clone = received.clone();
    primary
        .start_listener(Box::new(move |cmd| {
            received_clone.lock().unwrap().push(cmd);
        }))
        .expect("start listener");

    // With primary listener: secondary ping returns true
    let secondary = LocalSingleInstanceCoordinator::new(dir.path().to_path_buf());
    let ping_after = secondary
        .send_activation(&ActivationCommand::Ping)
        .expect("ping after start");
    assert!(ping_after, "Ping to running primary must return true");

    std::thread::sleep(Duration::from_millis(100));
    let cmds = received.lock().unwrap().clone();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0], ActivationCommand::Ping);
}

#[test]
fn test_local_single_instance_coordinator_handles_startup_command() {
    let dir = tempdir().expect("tempdir");
    let primary = LocalSingleInstanceCoordinator::new(dir.path().to_path_buf());

    let received = Arc::new(std::sync::Mutex::new(Vec::new()));
    let received_clone = received.clone();
    primary
        .start_listener(Box::new(move |cmd| {
            received_clone.lock().unwrap().push(cmd);
        }))
        .expect("start listener");

    let secondary = LocalSingleInstanceCoordinator::new(dir.path().to_path_buf());
    let sent = secondary
        .send_activation(&ActivationCommand::Startup)
        .expect("send startup command");
    assert!(sent, "Startup command must be received by running primary");

    std::thread::sleep(Duration::from_millis(100));
    let cmds = received.lock().unwrap().clone();
    assert_eq!(cmds.len(), 1);
    assert_eq!(cmds[0], ActivationCommand::Startup);
}

#[test]
fn test_mock_startup_manager_lifecycle_and_error_handling() {
    use window_hop::platform::{MockStartupManager, StartupManager};

    let manager = MockStartupManager::new(false);
    assert!(!manager.is_startup_registered().unwrap());

    manager.set_startup_registered(true).unwrap();
    assert!(manager.is_startup_registered().unwrap());

    manager.set_startup_registered(false).unwrap();
    assert!(!manager.is_startup_registered().unwrap());

    // Error simulation
    manager.set_error(Some("Access denied to registry".to_string()));
    assert!(manager.is_startup_registered().is_err());
    assert!(manager.set_startup_registered(true).is_err());
}
