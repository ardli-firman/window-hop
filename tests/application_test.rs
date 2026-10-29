use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use window_hop::application::{ApplicationError, LifecycleCoordinator};
use window_hop::domain::{
    DraftProfileInput, MonitorDescriptor, MonitorIdentity, ProfileStatus, TargetMonitorStatus,
};
use window_hop::persistence::{FileProfileStore, MemoryProfileStore};
use window_hop::platform::MockMonitorProvider;

#[test]
fn test_create_multiple_draft_profiles_and_verify_draft_status() {
    let store = Arc::new(MemoryProfileStore::new());
    let mut coordinator = LifecycleCoordinator::from_store(store);

    let id1 = coordinator
        .create_draft_profile(DraftProfileInput::new(
            "Antrian Poli Gigi",
            PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            "--app=https://antrian.poli/gigi --kiosk",
        ))
        .expect("create profile 1");

    let id2 = coordinator
        .create_draft_profile(DraftProfileInput::new(
            "Antrian Poli Mata",
            PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe"),
            "-kiosk https://antrian.poli/mata",
        ))
        .expect("create profile 2");

    assert_ne!(id1, id2, "Each profile must have unique consistent id");

    let profiles = coordinator.list_profiles();
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].id(), &id1);
    assert_eq!(profiles[0].name(), "Antrian Poli Gigi");
    assert_eq!(profiles[0].status(), ProfileStatus::DraftNotReady);

    assert_eq!(profiles[1].id(), &id2);
    assert_eq!(profiles[1].name(), "Antrian Poli Mata");
    assert_eq!(profiles[1].status(), ProfileStatus::DraftNotReady);

    // Draft profiles cannot be activated because target monitor is unselected
    let activation_result = coordinator.can_activate(&id1);
    assert!(
        activation_result.is_err(),
        "Draft profiles must not be ready for activation"
    );
    match activation_result.unwrap_err() {
        ApplicationError::DraftNotReady { id } => assert_eq!(id, id1),
        err => panic!("expected DraftNotReady error, got: {:?}", err),
    }
}

#[test]
fn test_persist_and_reopen_profiles_across_application_restarts() {
    let dir = tempdir().expect("temp dir");
    let file_path = dir.path().join("window_hop_profiles.json");

    let store1 = Arc::new(FileProfileStore::new(&file_path));
    let mut app1 = LifecycleCoordinator::from_store(store1);

    let id1 = app1
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Umum",
                PathBuf::from(r"C:\Browsers\Chrome\chrome.exe"),
                "--kiosk",
            ),
        )
        .expect("save profile 1");

    let id2 = app1
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Spesialis",
                PathBuf::from(r"C:\Browsers\Edge\msedge.exe"),
                "--app=https://queue.local",
            ),
        )
        .expect("save profile 2");

    // Simulate closing Window Hop and opening it again
    let store2 = Arc::new(FileProfileStore::new(&file_path));
    let mut app2 = LifecycleCoordinator::from_store(store2);
    app2.load().expect("load application profiles on restart");

    let reloaded = app2.list_profiles();
    assert_eq!(reloaded.len(), 2);

    let p1 = app2.get_profile(&id1).expect("profile 1 must exist");
    assert_eq!(p1.name(), "Antrian Umum");
    assert_eq!(
        p1.browser_path(),
        &PathBuf::from(r"C:\Browsers\Chrome\chrome.exe")
    );
    assert_eq!(p1.custom_parameters(), "--kiosk");
    assert_eq!(p1.status(), ProfileStatus::DraftNotReady);

    let p2 = app2.get_profile(&id2).expect("profile 2 must exist");
    assert_eq!(p2.name(), "Antrian Spesialis");
    assert_eq!(
        p2.browser_path(),
        &PathBuf::from(r"C:\Browsers\Edge\msedge.exe")
    );
    assert_eq!(p2.custom_parameters(), "--app=https://queue.local");
    assert_eq!(p2.status(), ProfileStatus::DraftNotReady);
}

#[test]
fn test_save_failure_reports_error_and_does_not_mutate_in_memory_state() {
    let memory_store = Arc::new(MemoryProfileStore::new());
    let mut app = LifecycleCoordinator::from_store(memory_store.clone());

    app.save_draft_profile(
        None,
        DraftProfileInput::new("Profil Valid Awal", PathBuf::from("chrome.exe"), "--kiosk"),
    )
    .expect("initial save should succeed");

    assert_eq!(app.list_profiles().len(), 1);

    // Simulate I/O failure on next save
    memory_store.set_simulate_io_error(true);

    let save_res = app.save_draft_profile(
        None,
        DraftProfileInput::new(
            "Profil Tambahan Gagal Simpan",
            PathBuf::from("firefox.exe"),
            "",
        ),
    );
    assert!(
        save_res.is_err(),
        "Save must fail when store returns I/O error"
    );
    assert!(
        app.last_error().is_some(),
        "Coordinator must record error for UI"
    );

    // Assert that in-memory state was NOT mutated/dirtied on failure!
    assert_eq!(
        app.list_profiles().len(),
        1,
        "In-memory profiles must not be mutated when disk save fails"
    );
    assert_eq!(app.list_profiles()[0].name(), "Profil Valid Awal");

    // Verify last valid state in store was NOT corrupted
    memory_store.set_simulate_io_error(false);
    let mut restart_app = LifecycleCoordinator::from_store(memory_store.clone());
    restart_app.load().expect("load stored data");

    let profiles = restart_app.list_profiles();
    assert_eq!(
        profiles.len(),
        1,
        "Only the last successfully saved configuration should persist"
    );
    assert_eq!(profiles[0].name(), "Profil Valid Awal");
}

#[test]
fn test_corrupt_load_prevents_subsequent_save_from_overwriting_corrupted_file() {
    let dir = tempdir().expect("temp dir");
    let file_path = dir.path().join("profiles.json");
    std::fs::write(&file_path, "INVALID JSON CONTENT").expect("write corrupt file");

    let store = Arc::new(FileProfileStore::new(&file_path));
    let mut app = LifecycleCoordinator::from_store(store);

    let load_res = app.load();
    assert!(load_res.is_err(), "Load should fail on corrupted file");
    assert!(app.last_error().is_some());

    // Attempting to save now must be blocked to prevent destroying the user's damaged file
    let save_res = app.save_draft_profile(
        None,
        DraftProfileInput::new("New Profile", PathBuf::from("chrome.exe"), ""),
    );
    assert!(save_res.is_err());
    match save_res.unwrap_err() {
        ApplicationError::BlockedByCorruptState(_) => {}
        err => panic!("expected BlockedByCorruptState, got: {:?}", err),
    }

    // Verify corrupted file on disk was not overwritten
    let raw = std::fs::read_to_string(&file_path).expect("read file");
    assert_eq!(raw, "INVALID JSON CONTENT");
}

#[test]
fn test_validation_rejects_empty_name_or_empty_path() {
    let store = Arc::new(MemoryProfileStore::new());
    let mut app = LifecycleCoordinator::from_store(store);

    let err_empty_name = app.create_draft_profile(DraftProfileInput::new(
        "   ",
        PathBuf::from("chrome.exe"),
        "",
    ));
    assert!(err_empty_name.is_err());
    match err_empty_name.unwrap_err() {
        ApplicationError::ValidationError(msg) => {
            assert!(msg.to_lowercase().contains("nama"));
        }
        err => panic!("expected ValidationError, got: {:?}", err),
    }

    let err_empty_path =
        app.create_draft_profile(DraftProfileInput::new("Profil", PathBuf::from(""), ""));
    assert!(err_empty_path.is_err());
    match err_empty_path.unwrap_err() {
        ApplicationError::ValidationError(msg) => {
            assert!(
                msg.to_lowercase().contains("browser")
                    || msg.to_lowercase().contains("jalur")
                    || msg.to_lowercase().contains("path")
            );
        }
        err => panic!("expected ValidationError, got: {:?}", err),
    }
}

#[test]
fn test_save_profile_with_target_monitor_and_evaluate_ready_status() {
    let store = Arc::new(MemoryProfileStore::new());
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-PRIMARY"),
            name: "Primary Nurse Workstation".to_string(),
            is_primary: true,
        },
        MonitorDescriptor {
            identity: MonitorIdentity::new("MON-QUEUE-ROOM1"),
            name: "Room 1 Queue Display".to_string(),
            is_primary: false,
        },
    ]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider);

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Rawat Jalan",
                PathBuf::from("chrome.exe"),
                "--kiosk",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-ROOM1"))),
        )
        .expect("save profile with target monitor");

    let target_status = coordinator
        .get_target_monitor_status(&profile_id)
        .expect("get target monitor status");
    match target_status {
        TargetMonitorStatus::Available(desc) => {
            assert_eq!(desc.identity, MonitorIdentity::new("MON-QUEUE-ROOM1"));
            assert_eq!(desc.name, "Room 1 Queue Display");
        }
        other => panic!("expected TargetMonitorStatus::Available, got {:?}", other),
    }

    let profile_status = coordinator
        .get_profile_status(&profile_id)
        .expect("get profile status");
    assert_eq!(profile_status, ProfileStatus::Ready);

    let can_activate = coordinator
        .can_activate(&profile_id)
        .expect("profile with available target monitor should be ready to activate");
    assert!(can_activate);
}

#[test]
fn test_target_monitor_matching_is_invariant_to_enumeration_reordering() {
    let store = Arc::new(MemoryProfileStore::new());
    let m_primary = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Display 1".to_string(),
        is_primary: true,
    };
    let m_queue = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Display 2".to_string(),
        is_primary: false,
    };

    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        m_primary.clone(),
        m_queue.clone(),
    ]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider.clone());

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Farmasi",
                PathBuf::from("chrome.exe"),
                "--kiosk",
            )
            .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    // Order 1: [m_primary, m_queue]
    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready
    );

    // Order 2: OS changes enumeration order to [m_queue, m_primary] (e.g. index 0 is now queue monitor)
    monitor_provider.set_monitors(vec![m_queue.clone(), m_primary.clone()]);
    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready,
        "Target monitor recognition must not depend on display enumeration index"
    );

    let target_status = coordinator.get_target_monitor_status(&profile_id).unwrap();
    match target_status {
        TargetMonitorStatus::Available(desc) => {
            assert_eq!(desc.identity, MonitorIdentity::new("MON-QUEUE"));
        }
        other => panic!("expected Available, got {:?}", other),
    }
}

#[test]
fn test_target_monitor_reconnect_restores_ready_status() {
    let store = Arc::new(MemoryProfileStore::new());
    let m_primary = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Nurse Screen".to_string(),
        is_primary: true,
    };
    let m_queue = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Queue Wall Screen".to_string(),
        is_primary: false,
    };

    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        m_primary.clone(),
        m_queue.clone(),
    ]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider.clone());

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian Poli Bedah", PathBuf::from("chrome.exe"), "--kiosk")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE"))),
        )
        .expect("save profile");

    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready
    );

    // Simulate cable disconnect / monitor powered off: only primary remains
    monitor_provider.set_monitors(vec![m_primary.clone()]);

    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::TargetUnavailable,
        "When target monitor disconnects, status must be TargetUnavailable"
    );

    // Simulate cable reconnected: monitor appears again with matching identity
    monitor_provider.set_monitors(vec![m_primary.clone(), m_queue.clone()]);

    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready,
        "When matching target monitor reconnects, status must be restored to Ready"
    );
}

#[test]
fn test_can_activate_fails_when_target_monitor_unavailable_without_fallback_to_primary() {
    let store = Arc::new(MemoryProfileStore::new());
    // Only primary nurse workstation monitor is available
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY-WORKSTATION"),
        name: "Nurse Primary Screen".to_string(),
        is_primary: true,
    }]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider);

    // Profile targets an external queue monitor that is currently unplugged
    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian Poli THT", PathBuf::from("chrome.exe"), "--kiosk")
                .with_target_monitor(Some(MonitorIdentity::new("MON-QUEUE-UNPLUGGED"))),
        )
        .expect("save profile");

    let status = coordinator.get_profile_status(&profile_id).unwrap();
    assert_eq!(status, ProfileStatus::TargetUnavailable);

    let target_status = coordinator.get_target_monitor_status(&profile_id).unwrap();
    match target_status {
        TargetMonitorStatus::Unavailable { recorded_identity } => {
            assert_eq!(
                recorded_identity,
                MonitorIdentity::new("MON-QUEUE-UNPLUGGED")
            );
        }
        other => panic!("expected TargetMonitorStatus::Unavailable, got {:?}", other),
    }

    // Must NOT fallback to primary workstation monitor; can_activate must fail
    let act_res = coordinator.can_activate(&profile_id);
    assert!(
        act_res.is_err(),
        "can_activate must fail when target monitor is unavailable; NO fallback allowed"
    );
    match act_res.unwrap_err() {
        ApplicationError::TargetMonitorUnavailable {
            id,
            recorded_identity,
        } => {
            assert_eq!(id, profile_id);
            assert_eq!(
                recorded_identity,
                MonitorIdentity::new("MON-QUEUE-UNPLUGGED")
            );
        }
        other => panic!("expected TargetMonitorUnavailable error, got {:?}", other),
    }
}

#[test]
fn test_can_activate_fails_when_target_monitor_is_ambiguous_and_prompts_reselection() {
    let store = Arc::new(MemoryProfileStore::new());
    let ambiguous_id = MonitorIdentity::new("AMBIGUOUS-ID-DUP");

    // Two identical screens plugged in with duplicate/ambiguous hardware IDs
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![
        MonitorDescriptor {
            identity: ambiguous_id.clone(),
            name: "Duplicate Screen 1".to_string(),
            is_primary: false,
        },
        MonitorDescriptor {
            identity: ambiguous_id.clone(),
            name: "Duplicate Screen 2".to_string(),
            is_primary: false,
        },
    ]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider);

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new(
                "Antrian Poli Jantung",
                PathBuf::from("chrome.exe"),
                "--kiosk",
            )
            .with_target_monitor(Some(ambiguous_id.clone())),
        )
        .expect("save profile");

    let status = coordinator.get_profile_status(&profile_id).unwrap();
    assert_eq!(status, ProfileStatus::TargetAmbiguous);

    let target_status = coordinator.get_target_monitor_status(&profile_id).unwrap();
    match target_status {
        TargetMonitorStatus::Ambiguous {
            recorded_identity,
            match_count,
        } => {
            assert_eq!(recorded_identity, ambiguous_id);
            assert_eq!(match_count, 2);
        }
        other => panic!("expected TargetMonitorStatus::Ambiguous, got {:?}", other),
    }

    let act_res = coordinator.can_activate(&profile_id);
    assert!(
        act_res.is_err(),
        "can_activate must fail when target monitor is ambiguous"
    );
    match act_res.unwrap_err() {
        ApplicationError::TargetMonitorAmbiguous {
            id,
            recorded_identity,
            match_count,
        } => {
            assert_eq!(id, profile_id);
            assert_eq!(recorded_identity, ambiguous_id);
            assert_eq!(match_count, 2);
        }
        other => panic!("expected TargetMonitorAmbiguous error, got {:?}", other),
    }
}

#[test]
fn test_replaced_monitor_with_different_identity_requires_reselection() {
    let store = Arc::new(MemoryProfileStore::new());
    let old_id = MonitorIdentity::new("MON-OLD-DELL");
    let new_id = MonitorIdentity::new("MON-NEW-LG");

    // Initially old monitor was recorded
    let monitor_provider = Arc::new(MockMonitorProvider::new(vec![MonitorDescriptor {
        identity: old_id.clone(),
        name: "Old Dell Screen".to_string(),
        is_primary: false,
    }]));
    let mut coordinator = LifecycleCoordinator::new(store, monitor_provider.clone());

    let profile_id = coordinator
        .save_draft_profile(
            None,
            DraftProfileInput::new("Antrian IGD", PathBuf::from("chrome.exe"), "--kiosk")
                .with_target_monitor(Some(old_id.clone())),
        )
        .expect("save profile");

    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready
    );

    // Hardware was replaced in clinic: old monitor gone, new monitor plugged in
    monitor_provider.set_monitors(vec![MonitorDescriptor {
        identity: new_id.clone(),
        name: "New LG Screen".to_string(),
        is_primary: false,
    }]);

    // Old identity does not match -> TargetUnavailable, cannot activate
    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::TargetUnavailable
    );
    assert!(coordinator.can_activate(&profile_id).is_err());

    // Nurse reselects target monitor in UI and saves
    coordinator
        .save_draft_profile(
            Some(&profile_id),
            DraftProfileInput::new("Antrian IGD", PathBuf::from("chrome.exe"), "--kiosk")
                .with_target_monitor(Some(new_id.clone())),
        )
        .expect("reselect target monitor");

    // Now matches new monitor -> Ready!
    assert_eq!(
        coordinator.get_profile_status(&profile_id).unwrap(),
        ProfileStatus::Ready
    );
    assert!(coordinator.can_activate(&profile_id).unwrap());
}
