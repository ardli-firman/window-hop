//! Action bar controls (save, activate, restart, stop, shortcut, delete, reset).

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{h_flex, Disableable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::application::{ActivationResult, StopOutcome};
use crate::ui::WindowHopView;

impl WindowHopView {
    /// Renders the action buttons at the bottom of the profile detail form.
    pub(crate) fn render_action_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let is_busy = self.is_busy;
        let is_running = self
            .selected_profile_id
            .as_ref()
            .map(|id| self.cached_running_ids.contains(id))
            .unwrap_or(false);

        let has_changes = self
            .selected_profile_id
            .as_ref()
            .map(|pid| self.cached_has_running_changes.contains(pid))
            .unwrap_or(false);

        let is_confirming_delete = self.confirming_delete_profile.is_some();

        h_flex()
            .gap_3()
            .child(
                Button::new("btn-save-profile")
                    .label(if is_busy {
                        "Menyimpan..."
                    } else {
                        "Simpan Profil"
                    })
                    .disabled(is_busy || is_confirming_delete)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.save_profile(window, cx);
                    })),
            )
            .when(self.selected_profile_id.is_some() && !is_running, |this| {
                this.child(
                    Button::new("btn-activate-profile")
                        .primary()
                        .label(if is_busy {
                            "Memproses..."
                        } else {
                            "Aktifkan Profil"
                        })
                        .disabled(is_busy || is_confirming_delete)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.activate_selected_profile(cx);
                        })),
                )
            })
            .when(self.selected_profile_id.is_some() && is_running, |this| {
                this.child(
                    Button::new("btn-restart-profile")
                        .when(has_changes, |btn| btn.primary())
                        .label(if is_busy {
                            "Memproses..."
                        } else {
                            "Jalankan Ulang"
                        })
                        .disabled(is_busy || is_confirming_delete)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.restart_selected_profile(cx);
                        })),
                )
                .child(
                    Button::new("btn-stop-profile")
                        .danger()
                        .label(if is_busy {
                            "Menghentikan..."
                        } else {
                            "Hentikan Profil"
                        })
                        .disabled(is_busy || is_confirming_delete)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.stop_selected_profile(cx);
                        })),
                )
            })
            .when(self.selected_profile_id.is_some(), |this| {
                this.child(
                    Button::new("btn-create-shortcut")
                        .label(if is_busy {
                            "Membuat Shortcut..."
                        } else {
                            "Buat Shortcut"
                        })
                        .disabled(is_busy || is_confirming_delete)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.create_shortcut_for_selected_profile(window, cx);
                        })),
                )
                .child(
                    Button::new("btn-delete-profile")
                        .danger()
                        .label("Hapus Profil")
                        .disabled(is_busy || is_confirming_delete)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.request_delete_selected_profile(cx);
                        })),
                )
            })
            .child(
                Button::new("btn-reset-form")
                    .label("Reset")
                    .disabled(is_busy || is_confirming_delete)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let id = this.selected_profile_id.clone();
                        this.select_profile(id, window, cx);
                    })),
            )
    }

    /// Mengaktifkan profil yang sedang dipilih melalui LifecycleCoordinator.
    /// Dijalankan di background executor untuk menjaga kelancaran antarmuka pengguna.
    pub fn activate_selected_profile(&mut self, cx: &mut Context<Self>) {
        if self.is_busy {
            return;
        }
        let Some(ref pid) = self.selected_profile_id else {
            return;
        };

        self.is_busy = true;
        let pid = pid.clone();
        let coordinator = self.coordinator.clone();
        self.status_message = Some("Mengaktifkan profil...".to_string());
        self.is_error = false;
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let coord_for_task = coordinator.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { coord_for_task.lock().unwrap().activate_profile(&pid) })
                    .await;

                cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.is_busy = false;
                        if let Ok(coord) = coordinator.try_lock() {
                            view.sync_cache_from_coordinator(&coord);
                        }
                        match result {
                            Ok(ActivationResult::Launched { target_monitor, .. }) => {
                                view.status_message = Some(format!(
                                    "Profil berhasil diaktifkan pada monitor '{}'.",
                                    target_monitor.name
                                ));
                                view.is_error = false;
                            }
                            Ok(ActivationResult::Reused { target_monitor, .. }) => {
                                view.status_message = Some(format!(
                                    "Profil sudah aktif; jendela diposisikan kembali pada monitor '{}'.",
                                    target_monitor.name
                                ));
                                view.is_error = false;
                            }
                            Ok(ActivationResult::WaitingForMonitor { target_monitor }) => {
                                view.status_message = Some(format!(
                                    "Monitor tujuan '{}' belum tersedia. Profil menunggu monitor tersambung.",
                                    target_monitor
                                ));
                                view.is_error = false;
                            }
                            Err(err) => {
                                view.status_message =
                                    Some(format!("Gagal mengaktifkan profil: {}", err));
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

    /// Menghentikan profil yang sedang dipilih melalui LifecycleCoordinator.
    /// Dijalankan di background executor untuk menjaga kelancaran antarmuka pengguna.
    pub fn stop_selected_profile(&mut self, cx: &mut Context<Self>) {
        if self.is_busy {
            return;
        }
        let Some(ref pid) = self.selected_profile_id else {
            return;
        };

        self.is_busy = true;
        let pid = pid.clone();
        let coordinator = self.coordinator.clone();
        self.status_message = Some("Menghentikan profil...".to_string());
        self.is_error = false;
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let coord_for_task = coordinator.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { coord_for_task.lock().unwrap().stop_profile(&pid) })
                    .await;

                cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.is_busy = false;
                        if let Ok(coord) = coordinator.try_lock() {
                            view.sync_cache_from_coordinator(&coord);
                        }
                        match result {
                            Ok(StopOutcome::Graceful { .. }) => {
                                view.status_message =
                                    Some("Profil berhasil dihentikan secara normal.".to_string());
                                view.is_error = false;
                            }
                            Ok(StopOutcome::Forced { .. }) => {
                                view.status_message = Some(
                                    "Profil dihentikan secara paksa setelah batas tenggat 3 detik."
                                        .to_string(),
                                );
                                view.is_error = false;
                            }
                            Ok(StopOutcome::NotRunning) => {
                                view.status_message =
                                    Some("Profil tidak sedang berjalan.".to_string());
                                view.is_error = false;
                            }
                            Err(err) => {
                                view.status_message =
                                    Some(format!("Gagal menghentikan profil: {}", err));
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

    /// Menjalankan ulang profil yang sedang dipilih melalui LifecycleCoordinator.
    /// Dijalankan di background executor untuk menjaga kelancaran antarmuka pengguna.
    pub fn restart_selected_profile(&mut self, cx: &mut Context<Self>) {
        if self.is_busy {
            return;
        }
        let Some(ref pid) = self.selected_profile_id else {
            return;
        };

        self.is_busy = true;
        let pid = pid.clone();
        let coordinator = self.coordinator.clone();
        self.status_message = Some("Menjalankan ulang profil...".to_string());
        self.is_error = false;
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let coord_for_task = coordinator.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move { coord_for_task.lock().unwrap().restart_profile(&pid) })
                    .await;

                cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.is_busy = false;
                        if let Ok(coord) = coordinator.try_lock() {
                            view.sync_cache_from_coordinator(&coord);
                        }
                        match result {
                            Ok(ActivationResult::Launched { target_monitor, .. }) => {
                                view.status_message = Some(format!(
                                    "Profil berhasil dijalankan ulang dengan konfigurasi terbaru pada monitor '{}'.",
                                    target_monitor.name
                                ));
                                view.is_error = false;
                            }
                            Ok(ActivationResult::WaitingForMonitor { target_monitor }) => {
                                view.status_message = Some(format!(
                                    "Monitor tujuan '{}' belum tersedia. Profil menunggu monitor tersambung.",
                                    target_monitor
                                ));
                                view.is_error = false;
                            }
                            Ok(ActivationResult::Reused { target_monitor, .. }) => {
                                view.status_message = Some(format!(
                                    "Profil sudah aktif pada monitor '{}'.",
                                    target_monitor.name
                                ));
                                view.is_error = false;
                            }
                            Err(err) => {
                                view.status_message =
                                    Some(format!("Gagal menjalankan ulang profil: {}", err));
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
