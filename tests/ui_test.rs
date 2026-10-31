//! Integration and contract tests for the modular Window Hop UI architecture.

use std::path::PathBuf;

use gpui_kit::component::tag::TagVariant;
use gpui_kit::px;
use window_hop::browsers::{
    BrowserArchitecture, BrowserCompatibilityStatus, BrowserSupportEvidence, DetectedBrowser,
};
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
    find_occupying_profile, BrowserCompatibilityBadge, BrowserQuickTileModel, MonitorCardModel,
    WindowHopView, DISCONNECTED_BADGE, DISCONNECTED_WARNING, MONITOR_POLL_INTERVAL,
    NURSE_PRIMARY_BADGE, NURSE_PRIMARY_WARNING, RECOMMENDED_QUEUE_BADGE,
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

#[test]
fn test_browser_quick_tile_model_selection_and_badges() {
    let chrome_browser = DetectedBrowser {
        name: "Google Chrome".to_string(),
        executable_path: PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        version: Some("122.0.6261.95".to_string()),
        architecture: BrowserArchitecture::X64,
    };
    let edge_browser = DetectedBrowser {
        name: "Microsoft Edge".to_string(),
        executable_path: PathBuf::from(
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        ),
        version: Some("120.0.2210.144".to_string()),
        architecture: BrowserArchitecture::X86,
    };
    let brave_browser = DetectedBrowser {
        name: "Brave Browser".to_string(),
        executable_path: PathBuf::from(
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
        ),
        version: None,
        architecture: BrowserArchitecture::X64,
    };
    let unverified_browser = DetectedBrowser {
        name: "Custom Embedded Browser".to_string(),
        executable_path: PathBuf::from(r"C:\Custom\custom_browser.exe"),
        version: None,
        architecture: BrowserArchitecture::Unknown,
    };

    // 1. Selection matching (case-insensitive)
    let chrome_tile = BrowserQuickTileModel::from_detected(
        &chrome_browser,
        r"c:\program files\google\chrome\application\chrome.exe",
    );
    assert!(chrome_tile.is_selected);
    assert_eq!(chrome_tile.brand_icon(), "🌐");
    assert_eq!(
        chrome_tile.status_badge(),
        ("Terverifikasi", TagVariant::Success)
    );
    assert_eq!(chrome_tile.subtitle(), "x64 (64-bit) • v122.0.6261.95");

    // 2. Unselected Edge
    let edge_tile = BrowserQuickTileModel::from_detected(&edge_browser, r"C:\other\path.exe");
    assert!(!edge_tile.is_selected);
    assert_eq!(edge_tile.brand_icon(), "🧭");
    assert_eq!(
        edge_tile.status_badge(),
        ("Terverifikasi", TagVariant::Success)
    );
    assert_eq!(edge_tile.subtitle(), "x86 (32-bit) • v120.0.2210.144");

    // 3. Brave Browser
    let brave_tile = BrowserQuickTileModel::from_detected(
        &brave_browser,
        r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
    );
    assert!(brave_tile.is_selected);
    assert_eq!(brave_tile.brand_icon(), "🦁");
    assert_eq!(
        brave_tile.status_badge(),
        ("Terverifikasi", TagVariant::Success)
    );
    assert_eq!(brave_tile.subtitle(), "x64 (64-bit)");

    // 4. Unverified Browser gets Warning badge
    let unverified_tile = BrowserQuickTileModel::from_detected(&unverified_browser, "");
    assert!(!unverified_tile.is_selected);
    assert_eq!(
        unverified_tile.status_badge(),
        ("Belum Diverifikasi", TagVariant::Warning)
    );
}

#[test]
fn test_browser_compatibility_badge_variants() {
    // 1. SafeToActivate
    let safe_status = BrowserCompatibilityStatus::SafeToActivate {
        adapter: window_hop::browsers::BrowserAdapter::Chrome(
            window_hop::browsers::ChromeAdapter::new(),
        ),
        evidence: BrowserSupportEvidence {
            browser_name: "Google Chrome".to_string(),
            version: Some("122.0.0".to_string()),
            architecture: BrowserArchitecture::X64,
            is_safe_to_activate: true,
            evidence_text: "Terverifikasi".to_string(),
            limitations: None,
        },
    };
    let badge_safe = BrowserCompatibilityBadge::for_status(Some(&safe_status));
    assert_eq!(badge_safe.label, "Terverifikasi");
    assert_eq!(badge_safe.variant, TagVariant::Success);
    assert!(badge_safe.is_safe);

    // 2. Unverified
    let unverified_status = BrowserCompatibilityStatus::Unverified {
        evidence: BrowserSupportEvidence {
            browser_name: "Custom Browser".to_string(),
            version: None,
            architecture: BrowserArchitecture::Unknown,
            is_safe_to_activate: false,
            evidence_text: "Belum teruji".to_string(),
            limitations: None,
        },
    };
    let badge_unverified = BrowserCompatibilityBadge::for_status(Some(&unverified_status));
    assert_eq!(badge_unverified.label, "Belum Diverifikasi");
    assert_eq!(badge_unverified.variant, TagVariant::Warning);
    assert!(!badge_unverified.is_safe);

    // 3. Unavailable
    let unavailable_status = BrowserCompatibilityStatus::Unavailable {
        path: PathBuf::from("nonexistent.exe"),
        reason: "File tidak ditemukan".to_string(),
    };
    let badge_unavailable = BrowserCompatibilityBadge::for_status(Some(&unavailable_status));
    assert_eq!(badge_unavailable.label, "Tidak Tersedia");
    assert_eq!(badge_unavailable.variant, TagVariant::Danger);
    assert!(!badge_unavailable.is_safe);

    // 4. None
    let badge_none = BrowserCompatibilityBadge::for_status(None);
    assert_eq!(badge_none.label, "Belum Dipilih");
    assert_eq!(badge_none.variant, TagVariant::Secondary);
    assert!(!badge_none.is_safe);
}

#[test]
fn test_form_bidirectional_parameter_sync_and_validation() {
    use window_hop::ui::form::validate_advanced_parameters;

    // 1. Valid advanced parameters
    assert!(validate_advanced_parameters("--disable-gpu --lang=id", None).is_ok());
    assert!(validate_advanced_parameters("", None).is_ok());

    // 2. Conflicting isolation parameters are rejected
    let err_user_data = validate_advanced_parameters(r#"--user-data-dir="C:\ChromeData""#, None);
    assert!(err_user_data.is_err());

    let err_profile_dir = validate_advanced_parameters(r#"--profile-directory="Profile 1""#, None);
    assert!(err_profile_dir.is_err());

    let err_firefox_profile = validate_advanced_parameters("-profile /tmp/firefox", None);
    assert!(err_firefox_profile.is_err());

    // 3. Unterminated quotes
    let err_unclosed = validate_advanced_parameters(r#"--custom="unclosed"#, None);
    assert!(err_unclosed.is_err());
}

#[test]
fn test_form_kiosk_and_url_unsaved_changes_detection() {
    use window_hop::browsers::LaunchParameters;

    let p = Profile::new_draft(
        ProfileId::new("p-queue"),
        "Antrian Poli Bedah",
        PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
        "--app=https://antrian.poli/bedah --kiosk",
    );

    // Initial state matching profile: no unsaved changes
    let lp = LaunchParameters::parse(p.custom_parameters()).unwrap();
    assert_eq!(lp.url, "https://antrian.poli/bedah");
    assert!(lp.kiosk);
    assert_eq!(lp.advanced_parameters, "");

    let matching_composed = lp.compose();
    let draft_matching = ProfileFormDraft {
        name: Some(p.name()),
        browser_path: Some(p.browser_path().to_str().unwrap()),
        custom_parameters: Some(&matching_composed),
        target_monitor: Some(&None),
    };
    assert!(!profile_has_unsaved_changes(&p, Some(&draft_matching)));

    // User changes URL
    let changed_url = LaunchParameters::compose_from(
        "https://antrian.poli/rawat-jalan",
        lp.kiosk,
        &lp.advanced_parameters,
    );
    let draft_changed_url = ProfileFormDraft {
        name: Some(p.name()),
        browser_path: Some(p.browser_path().to_str().unwrap()),
        custom_parameters: Some(&changed_url),
        target_monitor: Some(&None),
    };
    assert!(profile_has_unsaved_changes(&p, Some(&draft_changed_url)));

    // User toggles Kiosk mode off
    let toggled_kiosk = LaunchParameters::compose_from(&lp.url, false, &lp.advanced_parameters);
    let draft_toggled_kiosk = ProfileFormDraft {
        name: Some(p.name()),
        browser_path: Some(p.browser_path().to_str().unwrap()),
        custom_parameters: Some(&toggled_kiosk),
        target_monitor: Some(&None),
    };
    assert!(profile_has_unsaved_changes(&p, Some(&draft_toggled_kiosk)));

    // User adds advanced parameter
    let added_advanced = LaunchParameters::compose_from(&lp.url, lp.kiosk, "--disable-gpu");
    let draft_added_advanced = ProfileFormDraft {
        name: Some(p.name()),
        browser_path: Some(p.browser_path().to_str().unwrap()),
        custom_parameters: Some(&added_advanced),
        target_monitor: Some(&None),
    };
    assert!(profile_has_unsaved_changes(&p, Some(&draft_added_advanced)));
}
