use window_hop::domain::{MonitorDescriptor, MonitorIdentity};
use window_hop::platform::{MockMonitorProvider, MonitorProvider, PlatformError};

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
