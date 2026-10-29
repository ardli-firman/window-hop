use std::path::PathBuf;
use window_hop::domain::{
    MonitorDescriptor, MonitorIdentity, Profile, ProfileId, ProfileStatus, TargetMonitorStatus,
};

#[test]
fn test_profile_evaluate_status_draft_without_target_monitor() {
    let profile = Profile::new_draft(
        ProfileId::new("p1"),
        "Poli Umum",
        PathBuf::from("chrome.exe"),
        "",
    );

    let monitors = vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-1"),
        name: "Dell P2419H".to_string(),
        is_primary: true,
    }];

    assert_eq!(
        profile.evaluate_target_monitor(&monitors),
        TargetMonitorStatus::Unset
    );
    assert_eq!(
        profile.evaluate_status(&monitors),
        ProfileStatus::DraftNotReady
    );
}

#[test]
fn test_profile_evaluate_status_ready_when_target_monitor_matches_uniquely() {
    let mut profile = Profile::new_draft(
        ProfileId::new("p1"),
        "Poli Umum",
        PathBuf::from("chrome.exe"),
        "",
    );
    profile.set_target_monitor(Some(MonitorIdentity::new("MON-2")));

    let monitors = vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-1"),
            name: "Dell Primary".to_string(),
            is_primary: true,
        },
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-2"),
            name: "LG Secondary Queue".to_string(),
            is_primary: false,
        },
    ];

    match profile.evaluate_target_monitor(&monitors) {
        TargetMonitorStatus::Available(desc) => {
            assert_eq!(desc.identity, MonitorIdentity::new("MON-2"));
            assert_eq!(desc.name, "LG Secondary Queue");
        }
        other => panic!("expected Available status, got {:?}", other),
    }
    assert_eq!(profile.evaluate_status(&monitors), ProfileStatus::Ready);
}

#[test]
fn test_profile_evaluate_status_unavailable_when_target_monitor_missing() {
    let mut profile = Profile::new_draft(
        ProfileId::new("p1"),
        "Poli Gigi",
        PathBuf::from("chrome.exe"),
        "",
    );
    profile.set_target_monitor(Some(MonitorIdentity::new("DISCONNECTED-MONITOR")));

    let monitors = vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-1"),
        name: "Primary Screen".to_string(),
        is_primary: true,
    }];

    match profile.evaluate_target_monitor(&monitors) {
        TargetMonitorStatus::Unavailable { recorded_identity } => {
            assert_eq!(
                recorded_identity,
                MonitorIdentity::new("DISCONNECTED-MONITOR")
            );
        }
        other => panic!("expected Unavailable status, got {:?}", other),
    }
    assert_eq!(
        profile.evaluate_status(&monitors),
        ProfileStatus::TargetUnavailable
    );
}

#[test]
fn test_profile_evaluate_status_ambiguous_when_multiple_monitors_share_identity() {
    let mut profile = Profile::new_draft(
        ProfileId::new("p1"),
        "Poli Mata",
        PathBuf::from("chrome.exe"),
        "",
    );
    let ambiguous_id = MonitorIdentity::new("CLONED-MONITOR");
    profile.set_target_monitor(Some(ambiguous_id.clone()));

    let monitors = vec![
        MonitorDescriptor {
            identity: ambiguous_id.clone(),
            name: "Screen 1".to_string(),
            is_primary: false,
        },
        MonitorDescriptor {
            identity: ambiguous_id.clone(),
            name: "Screen 2".to_string(),
            is_primary: false,
        },
    ];

    match profile.evaluate_target_monitor(&monitors) {
        TargetMonitorStatus::Ambiguous {
            recorded_identity,
            match_count,
        } => {
            assert_eq!(recorded_identity, ambiguous_id);
            assert_eq!(match_count, 2);
        }
        other => panic!("expected Ambiguous status, got {:?}", other),
    }
    assert_eq!(
        profile.evaluate_status(&monitors),
        ProfileStatus::TargetAmbiguous
    );
}

#[test]
fn test_target_monitor_status_evaluate_pure_function() {
    let monitors = vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-ABC"),
        name: "LG Display".to_string(),
        is_primary: false,
    }];

    // Unset when target identity is None
    assert_eq!(
        TargetMonitorStatus::evaluate(None, &monitors),
        TargetMonitorStatus::Unset
    );

    // Available when target identity matches uniquely
    let target = MonitorIdentity::new("MON-ABC");
    match TargetMonitorStatus::evaluate(Some(&target), &monitors) {
        TargetMonitorStatus::Available(desc) => {
            assert_eq!(desc.identity, target);
            assert_eq!(desc.name, "LG Display");
        }
        other => panic!("expected Available status, got {:?}", other),
    }

    // Unavailable when target identity is not found
    let missing = MonitorIdentity::new("MON-MISSING");
    assert_eq!(
        TargetMonitorStatus::evaluate(Some(&missing), &monitors),
        TargetMonitorStatus::Unavailable {
            recorded_identity: missing
        }
    );
}
