//! Pure profile list helpers and startup preference commands.

use gpui_kit::*;

use crate::domain::{
    ExitReason, MonitorIdentity, Profile, ProfileId, ProfileLifecycleState, ProfileStatus,
};
use crate::ui::WindowHopView;

/// Minimum profile count required before the search box appears on the dashboard.
pub const SEARCH_MIN_PROFILES: usize = 4;

/// Determines whether the profile search box should be displayed.
pub fn should_show_profile_search(total_profiles: usize) -> bool {
    total_profiles >= SEARCH_MIN_PROFILES
}

/// Filters profiles by search query (case-insensitive substring match on profile name).
/// If query is empty or whitespace-only, returns all profiles.
pub fn filter_profiles_by_query<'a>(profiles: &'a [Profile], query: &str) -> Vec<&'a Profile> {
    let trimmed = query.trim().to_lowercase();
    if trimmed.is_empty() {
        profiles.iter().collect()
    } else {
        profiles
            .iter()
            .filter(|p| p.name().to_lowercase().contains(&trimmed))
            .collect()
    }
}

/// Indicator text for unsaved profile configuration changes.
pub const UNSAVED_CHANGES_INDICATOR: &str = "●";

/// Form input snapshot used to determine whether a profile has unsaved configuration changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileFormDraft<'a> {
    pub name: Option<&'a str>,
    pub browser_path: Option<&'a str>,
    pub custom_parameters: Option<&'a str>,
    pub target_monitor: Option<&'a Option<MonitorIdentity>>,
}

/// Checks if a profile has unsaved configuration changes (dirty form inputs).
pub fn profile_has_unsaved_changes(
    profile: &Profile,
    form_draft: Option<&ProfileFormDraft<'_>>,
) -> bool {
    let Some(draft) = form_draft else {
        return false;
    };

    if let Some(name) = draft.name {
        if name != profile.name() {
            return true;
        }
    }
    if let Some(path) = draft.browser_path {
        if path != profile.browser_path().to_string_lossy() {
            return true;
        }
    }
    if let Some(params) = draft.custom_parameters {
        let p_lp = crate::browsers::LaunchParameters::parse(profile.custom_parameters());
        let d_lp = crate::browsers::LaunchParameters::parse(params);
        match (p_lp, d_lp) {
            (Ok(p), Ok(d)) => {
                if p != d {
                    return true;
                }
            }
            _ => {
                if params != profile.custom_parameters() {
                    return true;
                }
            }
        }
    }
    if let Some(target) = draft.target_monitor {
        if target.as_ref() != profile.target_monitor() {
            return true;
        }
    }

    false
}

/// Badge text for profile configured as Windows startup profile (includes pin icon).
pub const STARTUP_BADGE_LABEL: &str = "📌 [Startup]";

/// Semantic shadcn status badge representation for a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileStatusBadge {
    pub label: &'static str,
    pub variant: gpui_kit::component::tag::TagVariant,
}

impl ProfileStatusBadge {
    /// Determines the semantic badge label and variant based on lifecycle state and profile readiness.
    pub fn for_profile(lifecycle: &ProfileLifecycleState, status: ProfileStatus) -> Self {
        use gpui_kit::component::tag::TagVariant;

        match lifecycle {
            ProfileLifecycleState::Running { .. } => Self {
                label: "Aktif",
                variant: TagVariant::Success,
            },
            ProfileLifecycleState::WaitingForMonitor { .. } => Self {
                label: "Menunggu",
                variant: TagVariant::Warning,
            },
            ProfileLifecycleState::Disconnected { .. } => Self {
                label: "Terputus",
                variant: TagVariant::Warning,
            },
            ProfileLifecycleState::Exited { reason } => match reason {
                ExitReason::Crash => Self {
                    label: "Crash",
                    variant: TagVariant::Danger,
                },
                ExitReason::ManualClose => Self {
                    label: "Berhenti",
                    variant: TagVariant::Secondary,
                },
            },
            ProfileLifecycleState::Idle => match status {
                ProfileStatus::DraftNotReady => Self {
                    label: "Draft",
                    variant: TagVariant::Secondary,
                },
                ProfileStatus::Ready => Self {
                    label: "Siap",
                    variant: TagVariant::Primary,
                },
                ProfileStatus::TargetUnavailable => Self {
                    label: "Tidak Tersedia",
                    variant: TagVariant::Warning,
                },
                ProfileStatus::TargetAmbiguous => Self {
                    label: "Ambigu",
                    variant: TagVariant::Danger,
                },
            },
        }
    }
}

impl WindowHopView {
    /// Mengaktifkan atau menonaktifkan startup otomatis saat login Windows.
    pub fn toggle_startup_enabled(&mut self, cx: &mut Context<Self>) {
        let target_enabled = !self.cached_startup_enabled;
        self.set_startup_enabled(target_enabled, cx);
    }

    /// Mengatur status startup otomatis saat login Windows ke nilai tertentu.
    pub fn set_startup_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let coordinator_arc = self.coordinator.clone();

        match coordinator_arc.lock().unwrap().set_startup_enabled(enabled) {
            Ok(()) => {
                if let Ok(coord) = coordinator_arc.try_lock() {
                    self.sync_cache_from_coordinator(&coord);
                }
                self.status_message = Some(if enabled {
                    "Startup Windows diaktifkan: Window Hop akan berjalan otomatis di system tray setelah login.".to_string()
                } else {
                    "Startup Windows dinonaktifkan: Window Hop tidak akan berjalan otomatis pada login berikutnya.".to_string()
                });
                self.is_error = false;
            }
            Err(err) => {
                let err_msg = err.to_string();
                if let Ok(coord) = coordinator_arc.try_lock() {
                    self.sync_cache_from_coordinator(&coord);
                }
                self.status_message =
                    Some(format!("Gagal mengubah pengaturan startup: {}", err_msg));
                self.is_error = true;
            }
        }
        cx.notify();
    }

    /// Mengatur atau membatalkan profil yang dipilih sebagai profil startup.
    pub fn set_startup_profile(&mut self, profile_id: Option<ProfileId>, cx: &mut Context<Self>) {
        let coordinator_arc = self.coordinator.clone();
        let prev_startup_id = self.cached_startup_profile_id.clone();

        match coordinator_arc
            .lock()
            .unwrap()
            .set_startup_profile_id(profile_id.clone())
        {
            Ok(()) => {
                if let Ok(coord) = coordinator_arc.try_lock() {
                    self.sync_cache_from_coordinator(&coord);
                }
                self.status_message = Some(match profile_id {
                    Some(ref pid) => {
                        let name = self
                            .find_cached_profile(pid)
                            .map(|p| p.name())
                            .unwrap_or("Profil");
                        if let Some(ref prev_id) = prev_startup_id {
                            if prev_id != pid {
                                let prev_name = self
                                    .find_cached_profile(prev_id)
                                    .map(|p| p.name())
                                    .unwrap_or("Sebelumnya");
                                crate::ui::form::StartupCardModel::format_transfer_message(
                                    prev_name, name,
                                )
                            } else {
                                format!(
                                    "Profil '{}' dipilih sebagai profil startup setelah login.",
                                    name
                                )
                            }
                        } else {
                            format!(
                                "Profil '{}' dipilih sebagai profil startup setelah login.",
                                name
                            )
                        }
                    }
                    None => {
                        if let Some(ref prev_id) = prev_startup_id {
                            let prev_name = self
                                .find_cached_profile(prev_id)
                                .map(|p| p.name())
                                .unwrap_or("Profil");
                            crate::ui::form::StartupCardModel::format_unset_message(prev_name)
                        } else {
                            "Pilihan profil startup dibatalkan.".to_string()
                        }
                    }
                });
                self.is_error = false;
            }
            Err(err) => {
                let err_msg = err.to_string();
                if let Ok(coord) = coordinator_arc.try_lock() {
                    self.sync_cache_from_coordinator(&coord);
                }
                self.status_message = Some(format!("Gagal mengatur profil startup: {}", err_msg));
                self.is_error = true;
            }
        }
        cx.notify();
    }
}
