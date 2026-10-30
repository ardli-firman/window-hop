//! Master sidebar navigation column for Window Hop profile management.

use gpui_kit::component::button::Button;
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::domain::{ExitReason, ProfileId, ProfileLifecycleState, ProfileStatus};
use crate::ui::tokens::{ShadcnTokens, SIDEBAR_WIDTH};
use crate::ui::WindowHopView;

impl WindowHopView {
    /// Renders the master navigation sidebar (fixed 320 px).
    pub(crate) fn render_sidebar(
        &self,
        shadcn: &ShadcnTokens,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let profiles = self.cached_profiles.clone();
        let selected_id = self.selected_profile_id.clone();
        let available_monitors = &self.cached_monitors;
        let is_busy = self.is_busy;

        v_flex()
            .w(SIDEBAR_WIDTH)
            .gap_3()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(shadcn.border)
            .bg(shadcn.card)
            .text_color(shadcn.card_foreground)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Daftar Profil ({})", profiles.len())),
                    )
                    .child(
                        Button::new("btn-new-profile")
                            .label("+ Profil Baru")
                            .disabled(is_busy)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.select_profile(None, window, cx);
                            })),
                    ),
            )
            .child(if profiles.is_empty() {
                div()
                    .p_4()
                    .text_sm()
                    .text_color(shadcn.muted_foreground)
                    .child("Belum ada profil. Klik '+ Profil Baru' untuk membuat.")
                    .into_any_element()
            } else {
                v_flex()
                    .gap_2()
                    .children(profiles.into_iter().map(|p| {
                        let is_selected = selected_id.as_ref() == Some(p.id());
                        let pid = p.id().clone();
                        let p_status = p.evaluate_status(available_monitors);
                        let p_lifecycle = self
                            .cached_lifecycle_states
                            .get(&pid)
                            .cloned()
                            .unwrap_or(ProfileLifecycleState::Idle);

                        let bg = if is_selected {
                            shadcn.accent.opacity(0.3)
                        } else {
                            shadcn.muted.opacity(0.4)
                        };
                        let border = if is_selected {
                            shadcn.primary
                        } else {
                            shadcn.border
                        };
                        let item_id = SharedString::from(format!("profile-item-{}", pid));

                        let (status_text, status_color) = match &p_lifecycle {
                            ProfileLifecycleState::Running { .. } => ("● Aktif", theme.primary),
                            ProfileLifecycleState::WaitingForMonitor { .. } => {
                                ("Menunggu", theme.warning)
                            }
                            ProfileLifecycleState::Disconnected { .. } => {
                                ("Terputus", theme.warning)
                            }
                            ProfileLifecycleState::Exited { reason } => match reason {
                                ExitReason::Crash => ("Crash", theme.danger),
                                ExitReason::ManualClose => ("Berhenti", shadcn.muted_foreground),
                            },
                            ProfileLifecycleState::Idle => match p_status {
                                ProfileStatus::DraftNotReady => ("Draft", shadcn.muted_foreground),
                                ProfileStatus::Ready => ("Siap", theme.primary),
                                ProfileStatus::TargetUnavailable => {
                                    ("Tidak Tersedia", theme.warning)
                                }
                                ProfileStatus::TargetAmbiguous => ("Ambigu", theme.danger),
                            },
                        };

                        div()
                            .id(item_id)
                            .p_3()
                            .rounded_md()
                            .border_1()
                            .border_color(border)
                            .bg(bg)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_profile(Some(pid.clone()), window, cx);
                            }))
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .child(
                                                h_flex()
                                                    .gap_1p5()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .child(p.name().to_string()),
                                                    )
                                                    .when(
                                                        Some(p.id())
                                                            == self
                                                                .cached_startup_profile_id
                                                                .as_ref(),
                                                        |this| {
                                                            this.child(
                                                                div()
                                                                    .text_xs()
                                                                    .px_1p5()
                                                                    .py_0p5()
                                                                    .rounded_sm()
                                                                    .bg(theme.warning.opacity(0.18))
                                                                    .text_color(theme.warning)
                                                                    .font_weight(
                                                                        FontWeight::SEMIBOLD,
                                                                    )
                                                                    .child("★ Startup"),
                                                            )
                                                        },
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .px_2()
                                                    .py_0p5()
                                                    .rounded_sm()
                                                    .bg(shadcn.muted)
                                                    .text_color(status_color)
                                                    .child(status_text),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(shadcn.muted_foreground)
                                            .child(p.browser_path().to_string_lossy().to_string()),
                                    )
                                    .child(
                                        div().text_xs().text_color(shadcn.muted_foreground).child(
                                            match p.target_monitor() {
                                                Some(m) => format!("Monitor: {}", m),
                                                None => "Monitor: (Belum Dipilih)".to_string(),
                                            },
                                        ),
                                    ),
                            )
                    }))
                    .into_any_element()
            })
            // Bagian Pengaturan Mulai Otomatis (Startup Windows)
            .child(
                v_flex()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(shadcn.border)
                    .bg(shadcn.muted.opacity(0.3))
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
                                                shadcn.muted_foreground
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
                    .child(div().text_xs().text_color(shadcn.muted_foreground).child(
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
                            .cached_profiles
                            .iter()
                            .find(|p| p.id() == pid)
                            .map(|p| p.name())
                            .unwrap_or("Profil");
                        format!(
                            "Profil '{}' dipilih sebagai profil startup setelah login.",
                            name
                        )
                    }
                    None => "Pilihan profil startup dibatalkan.".to_string(),
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
