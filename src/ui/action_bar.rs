//! Action bar controls (save, activate, restart, stop, shortcut, delete, reset).

use std::path::PathBuf;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, Disableable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::application::{ActivationResult, StopOutcome};
use crate::domain::ProfileLifecycleState;
use crate::ui::tokens::ShadcnTokens;
use crate::ui::WindowHopView;

/// Jenis aksi pada action bar profil.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Save,
    Activate,
    OpenWindow,
    ApplyRestart,
    Reconnect,
    Stop,
    CreateDesktopShortcut,
    Delete,
    Reset,
}

/// Deskriptor aksi individual pada action bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionBarAction {
    pub kind: ActionKind,
    pub id: &'static str,
    pub label: String,
    pub is_primary: bool,
    pub is_danger: bool,
    pub is_disabled: bool,
}

/// Model representasi opsi lokasi penyimpanan pintasan shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutOptionsModel {
    pub default_folder_label: &'static str,
    pub current_folder: Option<PathBuf>,
    pub is_desktop_default: bool,
}

impl ShortcutOptionsModel {
    /// Membuat model opsi shortcut dari jalur folder yang diberikan.
    pub fn new(custom_folder: Option<PathBuf>) -> Self {
        let is_desktop = custom_folder
            .as_ref()
            .map(|p| p.as_os_str().is_empty())
            .unwrap_or(true);
        Self {
            default_folder_label: "Desktop",
            current_folder: if is_desktop { None } else { custom_folder },
            is_desktop_default: is_desktop,
        }
    }

    /// Label tampilan tujuan penyimpanan shortcut.
    pub fn destination_display(&self) -> String {
        if let Some(ref path) = self.current_folder {
            path.display().to_string()
        } else {
            "Desktop (Default)".to_string()
        }
    }
}

/// Model murni representasi hierarki tombol pada sticky action bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionBarModel {
    pub primary_action: ActionBarAction,
    pub secondary_actions: Vec<ActionBarAction>,
}

impl ActionBarModel {
    /// Mengevaluasi hierarki aksi primer dan sekunder berdasarkan status lifecycle profil.
    pub fn evaluate(
        lifecycle_state: Option<&ProfileLifecycleState>,
        has_running_changes: bool,
        is_selected: bool,
        is_busy: bool,
    ) -> Self {
        if !is_selected {
            // Profil baru (belum disimpan / dipilih)
            let primary_action = ActionBarAction {
                kind: ActionKind::Save,
                id: "btn-save-profile",
                label: if is_busy {
                    "Menyimpan...".to_string()
                } else {
                    "Simpan Profil".to_string()
                },
                is_primary: true,
                is_danger: false,
                is_disabled: is_busy,
            };
            let secondary_actions = vec![ActionBarAction {
                kind: ActionKind::Reset,
                id: "btn-reset-form",
                label: "Reset".to_string(),
                is_primary: false,
                is_danger: false,
                is_disabled: is_busy,
            }];
            return Self {
                primary_action,
                secondary_actions,
            };
        }

        // Profil terpilih
        let primary_action = match lifecycle_state {
            Some(ProfileLifecycleState::Running { .. }) => {
                if has_running_changes {
                    ActionBarAction {
                        kind: ActionKind::ApplyRestart,
                        id: "btn-restart-profile",
                        label: if is_busy {
                            "Memproses...".to_string()
                        } else {
                            "Terapkan & Muat Ulang".to_string()
                        },
                        is_primary: true,
                        is_danger: false,
                        is_disabled: is_busy,
                    }
                } else {
                    ActionBarAction {
                        kind: ActionKind::OpenWindow,
                        id: "btn-open-window-profile",
                        label: if is_busy {
                            "Memproses...".to_string()
                        } else {
                            "Buka Window Profil".to_string()
                        },
                        is_primary: true,
                        is_danger: false,
                        is_disabled: is_busy,
                    }
                }
            }
            Some(ProfileLifecycleState::Disconnected { .. }) => ActionBarAction {
                kind: ActionKind::Reconnect,
                id: "btn-reconnect-profile",
                label: if is_busy {
                    "Memproses...".to_string()
                } else {
                    "Coba Hubungkan Kembali".to_string()
                },
                is_primary: true,
                is_danger: false,
                is_disabled: is_busy,
            },
            _ => ActionBarAction {
                kind: ActionKind::Activate,
                id: "btn-activate-profile",
                label: if is_busy {
                    "Memproses...".to_string()
                } else {
                    "Aktifkan Profil".to_string()
                },
                is_primary: true,
                is_danger: false,
                is_disabled: is_busy,
            },
        };

        let is_running = matches!(lifecycle_state, Some(ProfileLifecycleState::Running { .. }));

        let mut secondary_actions = Vec::new();

        if is_running {
            secondary_actions.push(ActionBarAction {
                kind: ActionKind::Stop,
                id: "btn-stop-profile",
                label: if is_busy {
                    "Menghentikan...".to_string()
                } else {
                    "Hentikan Profil".to_string()
                },
                is_primary: false,
                is_danger: true,
                is_disabled: is_busy,
            });
        }

        secondary_actions.push(ActionBarAction {
            kind: ActionKind::Save,
            id: "btn-save-profile",
            label: if is_busy {
                "Menyimpan...".to_string()
            } else {
                "Simpan Profil".to_string()
            },
            is_primary: false,
            is_danger: false,
            is_disabled: is_busy,
        });

        secondary_actions.push(ActionBarAction {
            kind: ActionKind::CreateDesktopShortcut,
            id: "btn-create-shortcut",
            label: if is_busy {
                "Membuat Shortcut...".to_string()
            } else {
                "Buat Pintasan di Desktop".to_string()
            },
            is_primary: false,
            is_danger: false,
            is_disabled: is_busy,
        });

        secondary_actions.push(ActionBarAction {
            kind: ActionKind::Delete,
            id: "btn-delete-profile",
            label: "Hapus Profil".to_string(),
            is_primary: false,
            is_danger: true,
            is_disabled: is_busy,
        });

        if !is_running {
            secondary_actions.push(ActionBarAction {
                kind: ActionKind::Reset,
                id: "btn-reset-form",
                label: "Reset".to_string(),
                is_primary: false,
                is_danger: false,
                is_disabled: is_busy,
            });
        }

        Self {
            primary_action,
            secondary_actions,
        }
    }
}

impl WindowHopView {
    /// Renders the action buttons at the bottom of the profile detail form as a sticky action bar.
    pub(crate) fn render_action_bar(
        &self,
        shadcn: &ShadcnTokens,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_busy = self.is_busy;
        let is_selected = self.selected_profile_id.is_some();
        let is_confirming_delete = self.confirming_delete_profile.is_some();

        let lifecycle_state = self
            .selected_profile_id
            .as_ref()
            .and_then(|id| self.cached_lifecycle_states.get(id));

        let has_changes = self
            .selected_profile_id
            .as_ref()
            .map(|pid| self.cached_has_running_changes.contains(pid))
            .unwrap_or(false);

        let model = ActionBarModel::evaluate(lifecycle_state, has_changes, is_selected, is_busy);

        h_flex()
            .id("sticky-action-bar")
            .w_full()
            .justify_between()
            .items_center()
            .p_4()
            .border_t_1()
            .border_color(shadcn.border)
            .bg(shadcn.card)
            // Sisi Kiri: Tombol aksi sekunder
            .child(h_flex().gap_2().items_center().children(
                model.secondary_actions.into_iter().map(|action| {
                    let action_id = action.id;
                    let is_disabled = action.is_disabled || is_confirming_delete;
                    let mut btn = Button::new(action_id)
                        .label(action.label.clone())
                        .disabled(is_disabled);

                    if action.is_danger {
                        btn = btn.danger();
                    }

                    match action.kind {
                        ActionKind::Save => btn.on_click(cx.listener(|this, _, window, cx| {
                            this.save_profile(window, cx);
                        })).into_any_element(),
                        ActionKind::CreateDesktopShortcut => {
                            let custom_folder_str = self
                                .shortcut_folder_input
                                .read(cx)
                                .value()
                                .trim()
                                .to_string();
                            let custom_folder = if custom_folder_str.is_empty() {
                                None
                            } else {
                                Some(std::path::PathBuf::from(custom_folder_str.clone()))
                            };
                            let options_model = ShortcutOptionsModel::new(custom_folder);
                            let theme = cx.theme().clone();

                            h_flex()
                                .gap_1()
                                .items_center()
                                .relative()
                                .child(
                                    btn.on_click(cx.listener(|this, _, _, cx| {
                                        this.create_desktop_shortcut_for_selected_profile(cx);
                                    })),
                                )
                                .child(
                                    Button::new("btn-shortcut-options-toggle")
                                        .label("▾")
                                        .compact()
                                        .disabled(is_disabled)
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.is_shortcut_popover_open =
                                                !this.is_shortcut_popover_open;
                                            cx.notify();
                                        })),
                                )
                                .when(self.is_shortcut_popover_open, |container| {
                                    container.child(
                                        v_flex()
                                            .id("shortcut-options-popover")
                                            .absolute()
                                            .bottom(px(42.0))
                                            .left_0()
                                            .w(px(320.0))
                                            .p_3()
                                            .rounded_lg()
                                            .border_1()
                                            .border_color(shadcn.border)
                                            .bg(shadcn.card)
                                            .shadow_lg()
                                            .gap_2()
                                            .child(
                                                h_flex()
                                                    .justify_between()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .child("Opsi Folder Pintasan"),
                                                    )
                                                    .child(
                                                        Button::new("btn-close-shortcut-popover")
                                                            .label("✕")
                                                            .compact()
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.is_shortcut_popover_open = false;
                                                                cx.notify();
                                                            })),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .child(format!(
                                                        "Lokasi target: {}",
                                                        options_model.destination_display()
                                                    )),
                                            )
                                            .child(
                                                Button::new("btn-create-desktop-popover")
                                                    .label("Buat di Desktop (Default)")
                                                    .disabled(is_disabled)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.is_shortcut_popover_open = false;
                                                        this.create_desktop_shortcut_for_selected_profile(cx);
                                                    })),
                                            )
                                            .child(
                                                Button::new("btn-browse-popover-folder")
                                                    .label("Pilih Folder Lain...")
                                                    .disabled(is_disabled)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.browse_shortcut_folder(cx);
                                                    })),
                                            )
                                            .when(!custom_folder_str.is_empty(), |popover| {
                                                popover.child(
                                                    Button::new("btn-create-custom-shortcut")
                                                        .label("Buat di Folder Kustom")
                                                        .disabled(is_disabled)
                                                        .on_click(cx.listener(|this, _, window, cx| {
                                                            this.is_shortcut_popover_open = false;
                                                            this.create_shortcut_for_selected_profile(window, cx);
                                                        })),
                                                )
                                            }),
                                    )
                                })
                                .into_any_element()
                        }
                        ActionKind::Stop => btn.on_click(cx.listener(|this, _, _, cx| {
                            this.stop_selected_profile(cx);
                        })).into_any_element(),
                        ActionKind::Delete => btn.on_click(cx.listener(|this, _, _, cx| {
                            this.request_delete_selected_profile(cx);
                        })).into_any_element(),
                        ActionKind::Reset => btn.on_click(cx.listener(|this, _, window, cx| {
                            let id = this.selected_profile_id.clone();
                            this.select_profile(id, window, cx);
                        })).into_any_element(),
                        _ => btn.into_any_element(),
                    }
                }),
            ))
            // Sisi Kanan: Tombol aksi primer kontekstual
            .child(h_flex().gap_2().items_center().child({
                let action = model.primary_action;
                let action_id = action.id;
                let is_disabled = action.is_disabled || is_confirming_delete;
                let btn = Button::new(action_id)
                    .primary()
                    .label(action.label)
                    .disabled(is_disabled);

                match action.kind {
                    ActionKind::Save => btn.on_click(cx.listener(|this, _, window, cx| {
                        this.save_profile(window, cx);
                    })),
                    ActionKind::Activate | ActionKind::OpenWindow | ActionKind::Reconnect => btn
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.activate_selected_profile(cx);
                        })),
                    ActionKind::ApplyRestart => btn.on_click(cx.listener(|this, _, _, cx| {
                        this.restart_selected_profile(cx);
                    })),
                    _ => btn,
                }
            }))
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
