//! Integration and contract tests for the modular Window Hop UI architecture.

use std::path::PathBuf;

use gpui_kit::component::tag::TagVariant;
use gpui_kit::px;
use window_hop::domain::{
    ExitReason, MonitorDescriptor, MonitorIdentity, ProcessId, Profile, ProfileId,
    ProfileLifecycleState, ProfileStatus, TargetMonitorStatus,
};
use window_hop::ui::sidebar::{
    filter_profiles_by_query, profile_has_unsaved_changes, should_show_profile_search,
    ProfileFormDraft, ProfileStatusBadge, SEARCH_MIN_PROFILES, STARTUP_BADGE_LABEL,
    UNSAVED_CHANGES_INDICATOR,
};
use window_hop::ui::tokens::{
    ShadcnTokens, SIDEBAR_WIDTH, WINDOW_INITIAL_HEIGHT, WINDOW_INITIAL_WIDTH, WINDOW_MIN_HEIGHT,
    WINDOW_MIN_WIDTH,
};
use window_hop::ui::{
    find_occupying_profile, MonitorCardModel, WindowHopView, DISCONNECTED_BADGE,
    DISCONNECTED_WARNING, MONITOR_POLL_INTERVAL, NURSE_PRIMARY_BADGE, NURSE_PRIMARY_WARNING,
    RECOMMENDED_QUEUE_BADGE,
};

#[test]
fn test_window_initial_and_minimum_dimensions_match_spec() {
    assert_eq!(WINDOW_INITIAL_WIDTH, px(1040.0));
    assert_eq!(WINDOW_INITIAL_HEIGHT, px(680.0));
    assert_eq!(WINDOW_MIN_WIDTH, px(920.0));
    assert_eq!(WINDOW_MIN_HEIGHT, px(580.0));
}

#[test]
fn test_sidebar_fixed_width_matches_shadcn_master_detail_spec() {
    assert_eq!(SIDEBAR_WIDTH, px(320.0));
}

#[test]
fn test_shadcn_zinc_and_slate_theme_tokens_are_valid() {
    let zinc_light = ShadcnTokens::zinc_light();
    let zinc_dark = ShadcnTokens::zinc_dark();
    assert_ne!(zinc_light.background, zinc_dark.background);
    assert_ne!(zinc_light.border, zinc_dark.border);

    let slate_light = ShadcnTokens::slate_light();
    let slate_dark = ShadcnTokens::slate_dark();
    assert_ne!(slate_light.background, slate_dark.background);
    assert_ne!(slate_light.border, slate_dark.border);

    // Verify WindowHopView type is publicly available
    let _ = std::any::type_name::<WindowHopView>();
}

#[test]
fn test_profile_status_badge_semantic_variants() {
    let dummy_monitor = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-1"),
        name: "Dell Monitor".to_string(),
        is_primary: true,
    };

    // 1. Running -> Aktif (Success)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Running {
            pid: ProcessId::new(1234),
            target_monitor: dummy_monitor,
        },
        ProfileStatus::Ready,
    );
    assert_eq!(badge.label, "Aktif");
    assert_eq!(badge.variant, TagVariant::Success);

    // 2. WaitingForMonitor -> Menunggu (Warning)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::WaitingForMonitor {
            target_monitor: MonitorIdentity::new("MON-1"),
        },
        ProfileStatus::Ready,
    );
    assert_eq!(badge.label, "Menunggu");
    assert_eq!(badge.variant, TagVariant::Warning);

    // 3. Disconnected -> Terputus (Warning)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Disconnected {
            target_monitor: MonitorIdentity::new("MON-1"),
        },
        ProfileStatus::Ready,
    );
    assert_eq!(badge.label, "Terputus");
    assert_eq!(badge.variant, TagVariant::Warning);

    // 4. Exited Crash -> Crash (Danger)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Exited {
            reason: ExitReason::Crash,
        },
        ProfileStatus::Ready,
    );
    assert_eq!(badge.label, "Crash");
    assert_eq!(badge.variant, TagVariant::Danger);

    // 5. Exited ManualClose -> Berhenti (Secondary)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Exited {
            reason: ExitReason::ManualClose,
        },
        ProfileStatus::Ready,
    );
    assert_eq!(badge.label, "Berhenti");
    assert_eq!(badge.variant, TagVariant::Secondary);

    // 6. Idle + Ready -> Siap (Primary)
    let badge = ProfileStatusBadge::for_profile(&ProfileLifecycleState::Idle, ProfileStatus::Ready);
    assert_eq!(badge.label, "Siap");
    assert_eq!(badge.variant, TagVariant::Primary);

    // 7. Idle + DraftNotReady -> Draft (Secondary)
    let badge =
        ProfileStatusBadge::for_profile(&ProfileLifecycleState::Idle, ProfileStatus::DraftNotReady);
    assert_eq!(badge.label, "Draft");
    assert_eq!(badge.variant, TagVariant::Secondary);

    // 8. Idle + TargetUnavailable -> Tidak Tersedia (Warning)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Idle,
        ProfileStatus::TargetUnavailable,
    );
    assert_eq!(badge.label, "Tidak Tersedia");
    assert_eq!(badge.variant, TagVariant::Warning);

    // 9. Idle + TargetAmbiguous -> Ambigu (Danger)
    let badge = ProfileStatusBadge::for_profile(
        &ProfileLifecycleState::Idle,
        ProfileStatus::TargetAmbiguous,
    );
    assert_eq!(badge.label, "Ambigu");
    assert_eq!(badge.variant, TagVariant::Danger);
}

#[test]
fn test_startup_badge_indicator() {
    assert!(STARTUP_BADGE_LABEL.contains("[Startup]"));
    assert!(STARTUP_BADGE_LABEL.contains("📌"));
}

#[test]
fn test_unsaved_configuration_changes_form_draft() {
    assert_eq!(UNSAVED_CHANGES_INDICATOR, "●");

    let p1 = Profile::new_draft(
        ProfileId::new("p1"),
        "Poli Gigi",
        PathBuf::from(r"C:\Chrome\chrome.exe"),
        "--kiosk",
    );

    // 1. None draft -> false
    assert!(!profile_has_unsaved_changes(&p1, None));

    // 2. Draft matching profile -> false
    let matching_draft = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&None),
    };
    assert!(!profile_has_unsaved_changes(&p1, Some(&matching_draft)));

    // 3. Modified name -> true
    let dirty_name = ProfileFormDraft {
        name: Some("Poli Gigi Baru"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&None),
    };
    assert!(profile_has_unsaved_changes(&p1, Some(&dirty_name)));

    // 4. Modified browser path -> true
    let dirty_path = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Edge\edge.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&None),
    };
    assert!(profile_has_unsaved_changes(&p1, Some(&dirty_path)));

    // 5. Modified custom parameters -> true
    let dirty_params = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk --incognito"),
        target_monitor: Some(&None),
    };
    assert!(profile_has_unsaved_changes(&p1, Some(&dirty_params)));

    // 6. Modified target monitor -> true
    let mon = Some(MonitorIdentity::new("MON-2"));
    let dirty_mon = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&mon),
    };
    assert!(profile_has_unsaved_changes(&p1, Some(&dirty_mon)));

    // 7. Profile with monitor: matching monitor -> false, cleared monitor -> true
    let mut p2 = p1.clone();
    p2.set_target_monitor(Some(MonitorIdentity::new("MON-1")));
    let same_mon = Some(MonitorIdentity::new("MON-1"));
    let matching_mon_draft = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&same_mon),
    };
    assert!(!profile_has_unsaved_changes(&p2, Some(&matching_mon_draft)));

    let cleared_mon_draft = ProfileFormDraft {
        name: Some("Poli Gigi"),
        browser_path: Some(r"C:\Chrome\chrome.exe"),
        custom_parameters: Some("--kiosk"),
        target_monitor: Some(&None),
    };
    assert!(profile_has_unsaved_changes(&p2, Some(&cleared_mon_draft)));
}

#[test]
fn test_search_box_threshold_and_instant_filtering() {
    assert_eq!(SEARCH_MIN_PROFILES, 4);
    assert!(!should_show_profile_search(0));
    assert!(!should_show_profile_search(1));
    assert!(!should_show_profile_search(3));
    assert!(should_show_profile_search(4));
    assert!(should_show_profile_search(10));

    let profiles = vec![
        Profile::new_draft(
            ProfileId::new("p1"),
            "Poli Umum",
            PathBuf::from("chrome.exe"),
            "",
        ),
        Profile::new_draft(
            ProfileId::new("p2"),
            "Poli Gigi",
            PathBuf::from("chrome.exe"),
            "",
        ),
        Profile::new_draft(
            ProfileId::new("p3"),
            "Poli Anak",
            PathBuf::from("chrome.exe"),
            "",
        ),
        Profile::new_draft(
            ProfileId::new("p4"),
            "Poli Mata",
            PathBuf::from("chrome.exe"),
            "",
        ),
        Profile::new_draft(
            ProfileId::new("p5"),
            "Farmasi Rawat Jalan",
            PathBuf::from("chrome.exe"),
            "",
        ),
    ];

    let res = filter_profiles_by_query(&profiles, "");
    assert_eq!(res.len(), 5);

    let res = filter_profiles_by_query(&profiles, "   ");
    assert_eq!(res.len(), 5);

    let res = filter_profiles_by_query(&profiles, "gigi");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].name(), "Poli Gigi");

    let res = filter_profiles_by_query(&profiles, "POLI");
    assert_eq!(res.len(), 4);

    let res = filter_profiles_by_query(&profiles, "rawat");
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].name(), "Farmasi Rawat Jalan");

    let res = filter_profiles_by_query(&profiles, "Laboratorium");
    assert!(res.is_empty());
}

#[test]
fn test_monitor_card_model_badges_and_resolution() {
    let primary_desc = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Dell P2419H".to_string(),
        is_primary: true,
    };
    let secondary_desc = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "LG TV 43 Inch".to_string(),
        is_primary: false,
    };

    // 1. Primary nurse screen gets gray badge [Layar Utama Perawat]
    let primary_card = MonitorCardModel::from_descriptor(&primary_desc, false, false, None);
    assert_eq!(primary_card.identity, MonitorIdentity::new("MON-PRIMARY"));
    assert_eq!(primary_card.name, "Dell P2419H");
    assert_eq!(primary_card.resolution, "1920 × 1080");
    assert_eq!(
        primary_card.role_badge(),
        Some((NURSE_PRIMARY_BADGE, TagVariant::Secondary))
    );

    // 2. Queue monitor gets green badge [Monitor Antrian Rekomendasi]
    let secondary_card = MonitorCardModel::from_descriptor(&secondary_desc, true, true, None)
        .with_resolution("3840 × 2160");
    assert_eq!(secondary_card.resolution, "3840 × 2160");
    assert_eq!(
        secondary_card.role_badge(),
        Some((RECOMMENDED_QUEUE_BADGE, TagVariant::Success))
    );

    // 3. Disconnected monitor gets warning badge [Terputus]
    let disconnected_card = MonitorCardModel::new_disconnected(
        MonitorIdentity::new("MON-LOST"),
        "Monitor Antrian Poli",
        true,
        None,
    );
    assert!(!disconnected_card.is_connected);
    assert_eq!(
        disconnected_card.role_badge(),
        Some((DISCONNECTED_BADGE, TagVariant::Warning))
    );
}

#[test]
fn test_nurse_primary_screen_guard_and_selection_warning() {
    let primary_desc = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Layar Kerja Perawat".to_string(),
        is_primary: true,
    };
    let secondary_desc = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Monitor Antrian".to_string(),
        is_primary: false,
    };

    // When primary monitor is NOT selected, no selection warning is triggered
    let unselected_primary = MonitorCardModel::from_descriptor(&primary_desc, false, false, None);
    assert_eq!(unselected_primary.selection_warning(), None);

    // When primary monitor IS selected, protection guard warning is emitted
    let selected_primary = MonitorCardModel::from_descriptor(&primary_desc, false, true, None);
    assert_eq!(
        selected_primary.selection_warning(),
        Some(NURSE_PRIMARY_WARNING)
    );

    // When queue monitor IS selected, no primary warning is triggered
    let selected_secondary = MonitorCardModel::from_descriptor(&secondary_desc, true, true, None);
    assert_eq!(selected_secondary.selection_warning(), None);
}

#[test]
fn test_active_profile_monitor_ownership_detection() {
    let mut profiles = vec![
        Profile::new_draft(
            ProfileId::new("p1"),
            "Poli Umum",
            PathBuf::from("chrome.exe"),
            "",
        ),
        Profile::new_draft(
            ProfileId::new("p2"),
            "Poli Anak",
            PathBuf::from("chrome.exe"),
            "",
        ),
    ];
    profiles[0].set_target_monitor(Some(MonitorIdentity::new("MON-1")));
    profiles[1].set_target_monitor(Some(MonitorIdentity::new("MON-2")));

    let queue_mon_desc = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-2"),
        name: "Monitor Antrian Poli".to_string(),
        is_primary: false,
    };

    let mut lifecycle_states = std::collections::HashMap::new();
    // Profile p2 (Poli Anak) is actively running on MON-2
    lifecycle_states.insert(
        ProfileId::new("p2"),
        ProfileLifecycleState::Running {
            pid: ProcessId::new(4567),
            target_monitor: queue_mon_desc.clone(),
        },
    );

    // 1. When editing Profile p1 (Poli Umum), MON-2 is occupied by "Poli Anak"
    let occupying = find_occupying_profile(
        &MonitorIdentity::new("MON-2"),
        Some(&ProfileId::new("p1")),
        &profiles,
        &lifecycle_states,
    );
    assert_eq!(occupying, Some("Poli Anak"));

    let card = MonitorCardModel::from_descriptor(
        &queue_mon_desc,
        true,
        false,
        occupying.map(String::from),
    );
    assert_eq!(
        card.occupancy_text(),
        Some("Dikuasai oleh profil: Poli Anak".to_string())
    );

    // 2. When editing Profile p2 itself, it should not report self-conflict
    let occupying_self = find_occupying_profile(
        &MonitorIdentity::new("MON-2"),
        Some(&ProfileId::new("p2")),
        &profiles,
        &lifecycle_states,
    );
    assert_eq!(occupying_self, None);

    // 3. When querying an unoccupied monitor (MON-1), returns None
    let occupying_mon1 = find_occupying_profile(
        &MonitorIdentity::new("MON-1"),
        Some(&ProfileId::new("p2")),
        &profiles,
        &lifecycle_states,
    );
    assert_eq!(occupying_mon1, None);
}

#[test]
fn test_disconnected_monitor_visual_properties_and_no_fallback() {
    let disconnected_card = MonitorCardModel::new_disconnected(
        MonitorIdentity::new("MON-DISCONNECTED"),
        "Monitor Antrian IGD",
        true,
        None,
    );

    assert!(!disconnected_card.is_connected);
    assert!(disconnected_card.is_dashed_border());
    assert!(disconnected_card.is_dimmed());
    assert_eq!(
        disconnected_card.role_badge(),
        Some((DISCONNECTED_BADGE, TagVariant::Warning))
    );
    assert_eq!(
        disconnected_card.disconnected_warning(),
        Some(DISCONNECTED_WARNING)
    );

    // Connected monitor should NOT have dashed border or dimmed style or disconnected warning
    let connected_card = MonitorCardModel::from_descriptor(
        &MonitorDescriptor {
            identity: MonitorIdentity::new("MON-OK"),
            name: "Monitor Antrian Poli".to_string(),
            is_primary: false,
        },
        true,
        true,
        None,
    );
    assert!(connected_card.is_connected);
    assert!(!connected_card.is_dashed_border());
    assert!(!connected_card.is_dimmed());
    assert_eq!(connected_card.disconnected_warning(), None);
}

#[test]
fn test_monitor_polling_interval_and_cable_change_detection() {
    use std::time::Duration;

    // 1. Verify background polling interval is between 2 and 3 seconds as specified
    assert!(MONITOR_POLL_INTERVAL >= Duration::from_secs(2));
    assert!(MONITOR_POLL_INTERVAL <= Duration::from_secs(3));

    // 2. Test dynamic detection of cable disconnect and reconnection without fallback to primary
    let primary_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-PRIMARY"),
        name: "Layar Utama Perawat".to_string(),
        is_primary: true,
    };
    let queue_mon = MonitorDescriptor {
        identity: MonitorIdentity::new("MON-QUEUE"),
        name: "Monitor Antrian".to_string(),
        is_primary: false,
    };

    // State A: Both cables connected
    let available_a = vec![primary_mon.clone(), queue_mon.clone()];
    let target_id = MonitorIdentity::new("MON-QUEUE");

    // Evaluate target monitor: should be uniquely available
    let status_a = TargetMonitorStatus::evaluate(Some(&target_id), &available_a);
    match status_a {
        TargetMonitorStatus::Available(desc) => {
            assert_eq!(desc.identity, target_id);
            assert_eq!(desc.name, "Monitor Antrian");
        }
        other => panic!("expected Available, got {:?}", other),
    }

    // State B: HDMI/VGA cable unplugged (queue monitor disconnected)
    let available_b = vec![primary_mon.clone()];
    let status_b = TargetMonitorStatus::evaluate(Some(&target_id), &available_b);
    match status_b {
        TargetMonitorStatus::Unavailable { recorded_identity } => {
            assert_eq!(recorded_identity, target_id);
            // Must NOT fallback to primary monitor!
            assert_ne!(recorded_identity, primary_mon.identity);
        }
        other => panic!("expected Unavailable without fallback, got {:?}", other),
    }

    // State C: HDMI/VGA cable plugged back in
    let available_c = vec![primary_mon.clone(), queue_mon.clone()];
    let status_c = TargetMonitorStatus::evaluate(Some(&target_id), &available_c);
    match status_c {
        TargetMonitorStatus::Available(desc) => {
            assert_eq!(desc.identity, target_id);
        }
        other => panic!("expected Available on reconnect, got {:?}", other),
    }
}
