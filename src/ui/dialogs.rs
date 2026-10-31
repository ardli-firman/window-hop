//! Status notification banners and delete confirmation dialogs for Window Hop UI.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _};
use gpui_kit::*;

use crate::application::StopOutcome;
use crate::domain::{Profile, ProfileId, ProfileLifecycleState};
use crate::ui::WindowHopView;

/// Model representasi modal konfirmasi AlertDialog saat menghapus profil aktif.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteConfirmationModalModel {
    pub profile_id: ProfileId,
    pub profile_name: String,
    pub target_monitor_name: String,
    pub title: &'static str,
    pub description: String,
    pub confirm_button_label: String,
    pub cancel_button_label: &'static str,
    pub is_destructive: bool,
    pub has_dim_backdrop: bool,
}

impl DeleteConfirmationModalModel {
    /// Menentukan apakah penghapusan profil memerlukan prompt konfirmasi modal.
    /// Hanya profil yang sedang aktif berjalan yang memerlukan konfirmasi sebelum browser ditutup.
    pub fn should_prompt_confirmation(is_running: bool) -> bool {
        is_running
    }

    /// Mengevaluasi model dialog konfirmasi untuk profil dan status tertentu.
    pub fn from_profile(
        profile: &Profile,
        lifecycle_state: &ProfileLifecycleState,
        is_busy: bool,
    ) -> Option<Self> {
        let target_monitor_name = match lifecycle_state {
            ProfileLifecycleState::Running { target_monitor, .. } => target_monitor.name.clone(),
            _ => return None,
        };

        let description = format!(
            "Profil '{}' sedang aktif berjalan di monitor '{}'. Menghapus profil ini akan menutup jendela browser yang sedang tayang (dengan batas tenggat 3 detik) dan menghapus konfigurasinya secara permanen.",
            profile.name(),
            target_monitor_name
        );

        let confirm_button_label = if is_busy {
            "Menghapus...".to_string()
        } else {
            "Ya, Hapus Profil & Tutup Jendela".to_string()
        };

        Some(Self {
            profile_id: profile.id().clone(),
            profile_name: profile.name().to_string(),
            target_monitor_name,
            title: "Konfirmasi Penghapusan Profil Aktif",
            description,
            confirm_button_label,
            cancel_button_label: "Batal",
            is_destructive: true,
            has_dim_backdrop: true,
        })
    }
}

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

    /// Renders the confirmation modal dialog when deleting an active running profile.
    pub(crate) fn render_delete_confirmation_dialog(
        &self,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let confirm_pid = self.confirming_delete_profile.clone()?;
        let theme = cx.theme().clone();
        let is_busy = self.is_busy;

        let profile = self.find_cached_profile(&confirm_pid)?;
        let lifecycle_state = self
            .cached_lifecycle_states
            .get(&confirm_pid)
            .cloned()
            .unwrap_or(ProfileLifecycleState::Idle);

        let model = match DeleteConfirmationModalModel::from_profile(profile, &lifecycle_state, is_busy) {
            Some(m) => m,
            None => DeleteConfirmationModalModel {
                profile_id: profile.id().clone(),
                profile_name: profile.name().to_string(),
                target_monitor_name: "-".to_string(),
                title: "Konfirmasi Hapus Profil Aktif",
                description: format!(
                    "Apakah Anda yakin ingin menghapus profil antrian '{}'? Tindakan ini tidak dapat dibatalkan.",
                    profile.name()
                ),
                confirm_button_label: if is_busy {
                    "Menghapus...".to_string()
                } else {
                    "Ya, Hapus Profil".to_string()
                },
                cancel_button_label: "Batal",
                is_destructive: true,
                has_dim_backdrop: true,
            },
        };

        Some(
            div()
                .id("delete-confirmation-modal-backdrop")
                .absolute()
                .inset_0()
                .bg(hsla(0.0, 0.0, 0.0, 0.65))
                .flex()
                .items_center()
                .justify_center()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.cancel_delete_profile(cx);
                }))
                .child(
                    div()
                        .id("delete-confirmation-dialog")
                        .w(px(480.0))
                        .p_6()
                        .rounded_xl()
                        .border_2()
                        .border_color(theme.danger)
                        .bg(theme.background)
                        .shadow_lg()
                        .on_click(|_, _, _| {})
                        .child(
                            v_flex()
                                .gap_4()
                                .child(
                                    h_flex()
                                        .gap_3()
                                        .items_center()
                                        .child(
                                            div()
                                                .p_2()
                                                .rounded_full()
                                                .bg(theme.danger.opacity(0.15))
                                                .text_color(theme.danger)
                                                .child("⚠"),
                                        )
                                        .child(
                                            div()
                                                .text_lg()
                                                .font_weight(FontWeight::BOLD)
                                                .text_color(theme.danger)
                                                .child(model.title),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.foreground)
                                        .child(model.description),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("Apakah Anda yakin ingin menghapus profil ini? Tindakan ini tidak dapat dibatalkan."),
                                )
                                .child(
                                    h_flex()
                                        .justify_end()
                                        .gap_3()
                                        .mt_2()
                                        .child(
                                            Button::new("btn-cancel-delete")
                                                .label(model.cancel_button_label)
                                                .disabled(is_busy)
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.cancel_delete_profile(cx);
                                                })),
                                        )
                                        .child(
                                            Button::new("btn-confirm-delete")
                                                .danger()
                                                .label(model.confirm_button_label)
                                                .disabled(is_busy)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.execute_delete_profile(confirm_pid.clone(), cx);
                                                })),
                                        ),
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
        let is_running_state = self
            .cached_lifecycle_states
            .get(pid)
            .map(|s| DeleteConfirmationModalModel::should_prompt_confirmation(s.is_running()))
            .unwrap_or(false);

        if is_running || is_running_state {
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
                                let _ = window_handle.update(cx, |_view, window, cx| {
                                    view.reset_form_inputs(window, cx);
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
