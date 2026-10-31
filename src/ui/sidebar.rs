//! Master sidebar navigation column for Window Hop profile management.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::empty::{Empty, EmptyDescription, EmptyHeader, EmptyTitle};
use gpui_kit::component::input::Input;
use gpui_kit::component::tag::{Tag as Badge, TagVariant};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _, Icon, IconName, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::domain::{
    ExitReason, MonitorIdentity, Profile, ProfileId, ProfileLifecycleState, ProfileStatus,
};
use crate::ui::tokens::{ShadcnTokens, SIDEBAR_WIDTH};
use crate::ui::WindowHopView;

/// Minimum profile count required before the search box appears in the sidebar.
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

/// Title displayed in the empty state card when no profiles exist.
pub(crate) const EMPTY_STATE_TITLE: &str = "Belum Ada Profil";

/// Description displayed in the empty state card when no profiles exist.
pub(crate) const EMPTY_STATE_DESCRIPTION: &str =
    "Buat profil baru untuk mengatur browser antrian pada monitor poliklinik.";

/// Form input snapshot used to determine whether a profile has unsaved configuration changes in the form.
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
    pub variant: TagVariant,
}

impl ProfileStatusBadge {
    /// Determines the semantic badge label and variant based on lifecycle state and profile readiness.
    pub fn for_profile(lifecycle: &ProfileLifecycleState, status: ProfileStatus) -> Self {
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
    /// Renders the master navigation sidebar (fixed 320 px).
    pub(crate) fn render_sidebar(
        &self,
        _shadcn: &ShadcnTokens,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let profiles = self.cached_profiles.clone();
        let selected_id = self.selected_profile_id.clone();
        let available_monitors = &self.cached_monitors;
        let is_busy = self.is_busy;
        let search_query = self.search_input.read(cx).value().to_string();
        let show_search = should_show_profile_search(profiles.len());
        let filtered_profiles = filter_profiles_by_query(&profiles, &search_query);

        v_flex()
            .w(SIDEBAR_WIDTH)
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .overflow_hidden()
            .gap_3()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.tokens.group_box)
            .text_color(theme.foreground)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Icon::new(IconName::LayoutDashboard).size_4().text_color(theme.muted_foreground))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("Daftar Profil ({})", profiles.len())),
                            ),
                    )
                    .child(
                        Button::new("btn-new-profile")
                            .primary()
                            .small()
                            .icon(IconName::Plus)
                            .label("Profil Baru")
                            .disabled(is_busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.select_profile(None, window, cx);
                            })),
                    ),
            )
            .when(show_search, |this| {
                this.child(
                    div().child(
                        Input::new(&self.search_input)
                            .cleanable(true)
                            .prefix(Icon::new(IconName::Search).size_3p5().text_color(theme.muted_foreground)),
                    ),
                )
            })
            .child(if profiles.is_empty() {
                v_flex()
                    .flex_1()
                    .justify_center()
                    .child(
                        Empty::new()
                            .header(
                                EmptyHeader::new()
                                    .title(EmptyTitle::new().child(EMPTY_STATE_TITLE))
                                    .description(EmptyDescription::new().child(EMPTY_STATE_DESCRIPTION)),
                            )
                            .child(
                                Button::new("btn-empty-new-profile")
                                    .primary()
                                    .icon(IconName::Plus)
                                    .label("+ Buat Profil Pertama")
                                    .disabled(is_busy)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.select_profile(None, window, cx);
                                    })),
                            ),
                    )
                    .into_any_element()
            } else if filtered_profiles.is_empty() {
                v_flex()
                    .flex_1()
                    .justify_center()
                    .items_center()
                    .p_4()
                    .child(
                        div()
                            .text_sm()
                            .text_center()
                            .text_color(theme.muted_foreground)
                            .child("Tidak ada profil yang cocok dengan pencarian."),
                    )
                    .into_any_element()
            } else {
                v_flex()
                    .id("sidebar-profiles-list")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .gap_2()
                    .children(filtered_profiles.into_iter().map(|p| {
                        let is_selected = selected_id.as_ref() == Some(p.id());
                        let pid = p.id().clone();
                        let p_status = p.evaluate_status(available_monitors);
                        let p_lifecycle = self
                            .cached_lifecycle_states
                            .get(&pid)
                            .cloned()
                            .unwrap_or(ProfileLifecycleState::Idle);

                        let (bg, border) = if is_selected {
                            (theme.primary.opacity(0.12), theme.primary)
                        } else {
                            (theme.muted.opacity(0.2), theme.border)
                        };
                        let item_id = SharedString::from(format!("profile-item-{}", pid));

                        let status_badge = ProfileStatusBadge::for_profile(&p_lifecycle, p_status);
                        let is_startup = self.cached_startup_profile_id.as_ref() == Some(p.id());
                        let has_unsaved = self.has_unsaved_changes(p.id(), cx);

                        let select_pid = pid.clone();

                        div()
                            .id(item_id)
                            .p_3()
                            .rounded_md()
                            .border_1()
                            .border_color(border)
                            .bg(bg)
                            .hover(|s| s.bg(theme.accent.opacity(0.18)))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_profile(Some(select_pid.clone()), window, cx);
                            }))
                            .child(
                                v_flex()
                                    .gap_1()
                                    .w_full()
                                    .min_w_0()
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .gap_2()
                                            .w_full()
                                            .child(
                                                h_flex()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .gap_1p5()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(theme.foreground)
                                                            .truncate()
                                                            .child(p.name().to_string()),
                                                    )
                                                    .when(is_startup, |this| {
                                                        this.child(
                                                            div()
                                                                .flex_shrink_0()
                                                                .child(
                                                                    Badge::new()
                                                                        .with_variant(TagVariant::Warning)
                                                                        .child(STARTUP_BADGE_LABEL),
                                                                ),
                                                        )
                                                    })
                                                    .when(has_unsaved, |this| {
                                                        this.child(
                                                            div()
                                                                .id(SharedString::from(format!(
                                                                    "dot-unsaved-{}",
                                                                    pid
                                                                )))
                                                                .flex_shrink_0()
                                                                .text_xs()
                                                                .text_color(theme.warning)
                                                                .font_weight(FontWeight::BOLD)
                                                                .child(UNSAVED_CHANGES_INDICATOR),
                                                        )
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .flex_shrink_0()
                                                    .child(
                                                        Badge::new()
                                                            .with_variant(status_badge.variant)
                                                            .child(status_badge.label),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        h_flex()
                                            .gap_1p5()
                                            .items_center()
                                            .w_full()
                                            .min_w_0()
                                            .child(Icon::new(IconName::Globe).size_3().text_color(theme.muted_foreground))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .truncate()
                                                    .child(p.browser_path().to_string_lossy().to_string()),
                                            ),
                                    )
                                    .child(
                                        h_flex()
                                            .gap_1p5()
                                            .items_center()
                                            .w_full()
                                            .min_w_0()
                                            .child(Icon::new(IconName::SquareTerminal).size_3().text_color(theme.muted_foreground))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .truncate()
                                                    .child(
                                                        match p.target_monitor() {
                                                            Some(m) => format!("Monitor: {}", m),
                                                            None => "Monitor: (Belum Dipilih)".to_string(),
                                                        },
                                                    ),
                                            ),
                                    ),
                            )
                    }))
                    .into_any_element()
            })
            // Bagian Pengaturan Mulai Otomatis (Startup Windows)
            .child(
                v_flex()
                    .flex_shrink_0()
                    .w_full()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.tokens.group_box)
                    .gap_2()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                v_flex()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::BOLD)
                                            .child("Mulai Otomatis (Startup Windows)"),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(if self.cached_startup_enabled {
                                                theme.primary
                                            } else {
                                                theme.muted_foreground
                                            })
                                            .child(if self.cached_startup_enabled {
                                                "Status: Aktif di Tray"
                                            } else {
                                                "Status: Nonaktif"
                                            }),
                                    ),
                            )
                            .child(
                                Button::new("btn-toggle-startup")
                                    .outline()
                                    .small()
                                    .label(if self.cached_startup_enabled {
                                        "Nonaktifkan"
                                    } else {
                                        "Aktifkan"
                                    })
                                    .disabled(is_busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_startup_enabled(cx);
                                    })),
                            ),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        match &self.cached_startup_profile_id {
                            Some(pid) => {
                                let name = self
                                    .cached_profiles
                                    .iter()
                                    .find(|p| p.id() == pid)
                                    .map(|p| p.name())
                                    .unwrap_or("Tidak ditemukan");
                                format!("Profil Startup: {}", name)
                            }
                            None => "Profil Startup: (Belum dipilih)".to_string(),
                        },
                    )),
            )
    }

    /// Mengaktifkan atau menonaktifkan startup otomatis saat login Windows.
    pub fn toggle_startup_enabled(&mut self, cx: &mut Context<Self>) {
        let coordinator_arc = self.coordinator.clone();
        let target_enabled = !self.cached_startup_enabled;

        match coordinator_arc
            .lock()
            .unwrap()
            .set_startup_enabled(target_enabled)
        {
            Ok(()) => {
                if let Ok(coord) = coordinator_arc.try_lock() {
                    self.sync_cache_from_coordinator(&coord);
                }
                self.status_message = Some(if target_enabled {
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
