use std::path::PathBuf;
use std::time::Duration;
use window_hop::browsers::{LaunchPlan, LoginRetentionNote};
use window_hop::domain::{MonitorDescriptor, MonitorIdentity, ProcessId};
use window_hop::platform::{
    Clock, MockClock, MockMonitorProvider, MockProcessLauncher, MonitorProvider, PlatformError,
    ProcessLauncher,
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
