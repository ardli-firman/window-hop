//! Status notification banners and delete confirmation dialogs for Window Hop UI.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _};
use gpui_kit::*;

use crate::application::StopOutcome;
use crate::domain::{ProfileId, ProfileLifecycleState};
use crate::ui::WindowHopView;

/// Marker type identifying the dialogs submodule.
pub struct DialogsMarker;

impl WindowHopView {
    /// Renders the top notification / status banner if a message is present.
    pub(crate) fn render_status_banner(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        let msg = self.status_message.as_ref()?;
        let theme = cx.theme().clone();
        let bg_color = if self.is_error {
            theme.danger.opacity(0.15)
        } else {
            theme.primary.opacity(0.15)
        };
        let text_color = if self.is_error {
            theme.danger
        } else {
            theme.primary
        };

        Some(
            div()
                .p_3()
                .rounded_md()
                .bg(bg_color)
                .border_1()
                .border_color(text_color)
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(div().text_sm().text_color(text_color).child(msg.clone()))
                        .child(Button::new("btn-dismiss-status").label("Tutup").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.status_message = None;
                                cx.notify();
                            }),
                        )),
                ),
        )
    }

    /// Renders the confirmation dialog banner when deleting an active running profile.
    pub(crate) fn render_delete_confirmation_dialog(
        &self,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let confirm_pid = self.confirming_delete_profile.clone()?;
        let theme = cx.theme().clone();
        let is_busy = self.is_busy;

        let p_name = self
            .cached_profiles
            .iter()
            .find(|p| p.id() == &confirm_pid)
            .map(|p| p.name().to_string())
            .unwrap_or_else(|| confirm_pid.to_string());
        let p_mon = self
            .cached_lifecycle_states
            .get(&confirm_pid)
            .and_then(|st| match st {
                ProfileLifecycleState::Running { target_monitor, .. } => {
                    Some(format!("monitor '{}'", target_monitor.name))
                }
                _ => None,
            })
            .unwrap_or_else(|| "monitor tujuan".to_string());

        Some(
            div()
                .id("delete-confirmation-banner")
                .p_4()
                .rounded_lg()
                .border_2()
                .border_color(theme.danger)
                .bg(theme.danger.opacity(0.12))
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.danger)
                                .child("⚠ Konfirmasi Penghapusan Profil Aktif"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .child(format!(
                                    "Profil '{}' sedang aktif berjalan di {}. Menghapus profil ini akan menutup jendela browser yang sedang tayang (dengan batas tenggat 3 detik) dan menghapus konfigurasinya secara permanen.",
                                    p_name, p_mon
                                )),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Apakah Anda yakin ingin menghapus profil ini? Tindakan ini tidak dapat dibatalkan."),
                        )
                        .child(
                            h_flex()
                                .gap_3()
                                .mt_1()
                                .child(
                                    Button::new("btn-confirm-delete")
                                        .danger()
                                        .label(if is_busy {
                                            "Menghapus..."
                                        } else {
                                            "Ya, Hapus Profil & Tutup Jendela"
                                        })
                                        .disabled(is_busy)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.execute_delete_profile(confirm_pid.clone(), cx);
                                        })),
                                )
                                .child(
                                    Button::new("btn-cancel-delete")
                                        .label("Batal")
                                        .disabled(is_busy)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.cancel_delete_profile(cx);
                                        })),
                                ),
                        ),
                ),
        )
    }

    /// Meminta konfirmasi penghapusan profil yang sedang dipilih jika profil sedang aktif.
    /// Jika profil tidak aktif (draft/idle), langsung menjalankan penghapusan.
    pub fn request_delete_selected_profile(&mut self, cx: &mut Context<Self>) {
        if self.is_busy {
            return;
        }
        let Some(ref pid) = self.selected_profile_id else {
            return;
        };

        let is_running = self.cached_running_ids.contains(pid);
        if is_running {
            // Minta konfirmasi sebelum menghapus profil aktif
            self.confirming_delete_profile = Some(pid.clone());
            self.status_message = None;
            self.is_error = false;
            cx.notify();
        } else {
            self.execute_delete_profile(pid.clone(), cx);
        }
    }

    /// Membatalkan konfirmasi penghapusan profil aktif tanpa mengubah tayangan maupun data profil.
    pub(crate) fn cancel_delete_profile(&mut self, cx: &mut Context<Self>) {
        self.confirming_delete_profile = None;
        cx.notify();
    }

    /// Menjalankan penghapusan profil di background thread dengan penutupan instance 3 detik.
    pub(crate) fn execute_delete_profile(&mut self, pid: ProfileId, cx: &mut Context<Self>) {
        if self.is_busy {
            return;
        }
        self.confirming_delete_profile = None;
        self.is_busy = true;

        let profile_name = self
            .cached_profiles
            .iter()
            .find(|p| p.id() == &pid)
            .map(|p| p.name().to_string())
            .unwrap_or_else(|| pid.to_string());

        let coordinator = self.coordinator.clone();
        let target_pid = pid.clone();
        self.status_message = Some(format!("Menghapus profil '{}'...", profile_name));
        self.is_error = false;
        cx.notify();

        let window_handle = self.window_handle;
        let name_input = self.name_input.clone();
        let path_input = self.path_input.clone();
        let params_input = self.params_input.clone();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let coord_for_task = coordinator.clone();
                let pid_to_delete = target_pid.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        coord_for_task
                            .lock()
                            .unwrap()
                            .delete_profile(&pid_to_delete)
                    })
                    .await;

                cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.is_busy = false;
                        if let Ok(coord) = coordinator.try_lock() {
                            view.sync_cache_from_coordinator(&coord);
                        }

                        match result {
                            Ok(stop_outcome) => {
                                // Bersihkan formulir dan seleksi profil
                                view.selected_profile_id = None;
                                view.selected_target_monitor = None;
                                let _ = window_handle.update(cx, |_view, window, cx| {
                                    name_input.update(cx, |input, cx| {
                                        input.set_value("", window, cx);
                                    });
                                    path_input.update(cx, |input, cx| {
                                        input.set_value("", window, cx);
                                    });
                                    params_input.update(cx, |input, cx| {
                                        input.set_value("", window, cx);
                                    });
                                });

                                let stop_detail = match stop_outcome {
                                    StopOutcome::Graceful { .. } => {
                                        " dan jendela browser berhasil ditutup secara normal."
                                    }
                                    StopOutcome::Forced { .. } => {
                                        " dan jendela browser dihentikan paksa setelah batas tenggat 3 detik."
                                    }
                                    StopOutcome::NotRunning => ".",
                                };
                                view.status_message = Some(format!(
                                    "Profil '{}' berhasil dihapus{}",
                                    profile_name, stop_detail
                                ));
                                view.is_error = false;
                            }
                            Err(err) => {
                                view.status_message =
                                    Some(format!("Gagal menghapus profil: {}", err));
                                view.is_error = true;
                            }
                        }
                        cx.notify();
                    });
                });
            }
        })
        .detach();
    }
}
