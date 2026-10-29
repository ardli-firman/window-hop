//! GPUI presentation for Window Hop profile management.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::application::{ActivationResult, LifecycleCoordinator, StopOutcome};
use crate::domain::{
    DraftProfileInput, ExitReason, MonitorDescriptor, MonitorIdentity, ProfileId,
    ProfileLifecycleState, ProfileStatus, TargetMonitorStatus,
};

/// Tampilan utama pengaturan profil Window Hop.
pub struct WindowHopView {
    coordinator: Arc<Mutex<LifecycleCoordinator>>,
    selected_profile_id: Option<ProfileId>,
    selected_target_monitor: Option<MonitorIdentity>,
    cached_monitors: Vec<MonitorDescriptor>,
    monitors_error: Option<String>,
    window_handle: AnyWindowHandle,
    name_input: Entity<InputState>,
    path_input: Entity<InputState>,
    params_input: Entity<InputState>,
    status_message: Option<String>,
    is_error: bool,
}

impl WindowHopView {
    /// Membuat instance baru WindowHopView.
    pub fn new(
        coordinator: LifecycleCoordinator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("misal: Antrian Poli Rawat Jalan"));
        let path_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
        });
        let params_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("--app=https://antrian.local --kiosk")
        });

        // Tampilkan pesan error startup langsung jika pembacaan file konfigurasi awal gagal
        let initial_error = coordinator.last_error().map(|s| s.to_string());
        let is_error = initial_error.is_some();

        let (cached_monitors, monitors_error) = match coordinator.available_monitors() {
            Ok(m) => (m, None),
            Err(e) => (Vec::new(), Some(e.to_string())),
        };

        Self {
            coordinator: Arc::new(Mutex::new(coordinator)),
            selected_profile_id: None,
            selected_target_monitor: None,
            cached_monitors,
            monitors_error,
            window_handle: window.window_handle(),
            name_input,
            path_input,
            params_input,
            status_message: initial_error,
            is_error,
        }
    }

    /// Memperbarui cache daftar monitor yang terdeteksi secara manual atau saat ada perubahan.
    pub fn refresh_monitors(&mut self, cx: &mut Context<Self>) {
        match self.coordinator.lock().unwrap().available_monitors() {
            Ok(monitors) => {
                self.cached_monitors = monitors;
                self.monitors_error = None;
            }
            Err(err) => {
                let err_msg = err.to_string();
                self.monitors_error = Some(err_msg.clone());
                self.status_message = Some(format!("Gagal mendeteksi monitor: {}", err_msg));
                self.is_error = true;
            }
        }
        cx.notify();
    }

    /// Memilih profil untuk diedit atau beralih ke form profil baru.
    pub fn select_profile(
        &mut self,
        id: Option<ProfileId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_profile_id = id.clone();
        self.status_message = None;
        self.is_error = false;

        // Segarkan cache monitor saat memilih profil agar informasi layar selalu mutakhir
        if let Ok(monitors) = self.coordinator.lock().unwrap().available_monitors() {
            self.cached_monitors = monitors;
            self.monitors_error = None;
        }

        if let Some(ref pid) = id {
            let profile_opt = self.coordinator.lock().unwrap().get_profile(pid).cloned();
            if let Some(profile) = profile_opt {
                let name = profile.name().to_string();
                let path = profile.browser_path().to_string_lossy().to_string();
                let params = profile.custom_parameters().to_string();
                self.selected_target_monitor = profile.target_monitor().cloned();

                self.name_input.update(cx, |input, cx| {
                    input.set_value(name, window, cx);
                });
                self.path_input.update(cx, |input, cx| {
                    input.set_value(path, window, cx);
                });
                self.params_input.update(cx, |input, cx| {
                    input.set_value(params, window, cx);
                });
            }
        } else {
            // Reset input form untuk profil baru
            self.selected_target_monitor = None;
            self.name_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            self.path_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            self.params_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
        }

        cx.notify();
    }

    /// Membuka dialog pemilih file asli OS untuk memilih executable browser secara manual.
    pub fn browse_browser_executable(&mut self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let path_input = self.path_input.clone();

        let prompt_task = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Pilih File Executable Browser".into()),
        });

        cx.spawn(move |_this, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                if let Ok(Ok(Some(paths))) = prompt_task.await {
                    if let Some(path) = paths.into_iter().next() {
                        let path_str = path.to_string_lossy().to_string();
                        cx.update(|cx| {
                            let _ = window_handle.update(cx, |_view, window, cx| {
                                path_input.update(cx, |input, cx| {
                                    input.set_value(path_str, window, cx);
                                });
                            });
                        });
                    }
                }
            }
        })
        .detach();
    }

    /// Menyimpan profil draft atau profil dengan monitor tujuan secara asinkron tanpa memblokir thread UI.
    /// Operasi disk dijalankan di background executor, dan state memori hanya dikomit
    /// setelah persistensi berhasil.
    pub fn save_profile(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name_input.read(cx).value().to_string();
        let path_str = self.path_input.read(cx).value().to_string();
        let params = self.params_input.read(cx).value().to_string();

        let browser_path = PathBuf::from(path_str.trim());
        let input = DraftProfileInput::new(name, browser_path, params)
            .with_target_monitor(self.selected_target_monitor.clone());

        let prepare_result = self
            .coordinator
            .lock()
            .unwrap()
            .prepare_draft_save(self.selected_profile_id.as_ref(), &input);

        let (candidate_config, target_id) = match prepare_result {
            Ok(res) => res,
            Err(err) => {
                self.status_message = Some(err.to_string());
                self.is_error = true;
                cx.notify();
                return;
            }
        };

        let store = self.coordinator.lock().unwrap().store().clone();
        self.status_message = Some("Menyimpan profil...".to_string());
        self.is_error = false;
        cx.notify();

        let coordinator = self.coordinator.clone();
        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                // Jalankan penulisan disk atomik di background executor
                let to_save = candidate_config.clone();
                let save_result = cx
                    .background_executor()
                    .spawn(async move { store.save(&to_save) })
                    .await;

                cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        match save_result {
                            Ok(()) => {
                                coordinator
                                    .lock()
                                    .unwrap()
                                    .commit_saved_config(candidate_config);
                                view.selected_profile_id = Some(target_id);
                                view.status_message =
                                    Some("Profil berhasil disimpan secara atomik.".to_string());
                                view.is_error = false;
                            }
                            Err(err) => {
                                view.status_message =
                                    Some(format!("Gagal menyimpan ke penyimpanan: {}", err));
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

    /// Mengaktifkan profil yang sedang dipilih melalui LifecycleCoordinator.
    /// Operasi penutupan dan peluncuran dijalankan di background executor
    /// agar tidak memblokir event loop GPUI selama deadline 3 detik.
    pub fn activate_selected_profile(&mut self, cx: &mut Context<Self>) {
        let Some(ref pid) = self.selected_profile_id else {
            return;
        };

        let pid = pid.clone();
        let coordinator = self.coordinator.clone();
        self.status_message = Some("Mengaktifkan profil...".to_string());
        self.is_error = false;
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let result = cx
                    .background_executor()
                    .spawn(async move { coordinator.lock().unwrap().activate_profile(&pid) })
                    .await;

                cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
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
        let Some(ref pid) = self.selected_profile_id else {
            return;
        };

        let pid = pid.clone();
        let coordinator = self.coordinator.clone();
        self.status_message = Some("Menghentikan profil...".to_string());
        self.is_error = false;
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let result = cx
                    .background_executor()
                    .spawn(async move { coordinator.lock().unwrap().stop_profile(&pid) })
                    .await;

                cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
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
}

impl Render for WindowHopView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let coord = self.coordinator.lock().unwrap();
        let profiles = coord.list_profiles();
        let selected_id = self.selected_profile_id.clone();
        let available_monitors = &self.cached_monitors;

        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .p_6()
            .gap_4()
            // Header
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .child(
                                div()
                                    .text_xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child("Window Hop - Profil Antrian"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("Sesi Windows: Tanpa PIN/Akun Tambahan"),
                            ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Konfigurasi satu aplikasi browser dan parameter kustom untuk tampilan antrian poliklinik."),
                    ),
            )
            // Banner notifikasi / status error
            .when_some(self.status_message.as_ref(), |this, msg| {
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
                this.child(
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
                                .child(
                                    Button::new("btn-dismiss-status")
                                        .label("Tutup")
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.status_message = None;
                                             cx.notify();
                                        })),
                                ),
                        ),
                )
            })
            // Konten Utama: 2 Kolom
            .child(
                h_flex()
                    .flex_1()
                    .gap_6()
                    // Kolom Kiri: Daftar Profil
                    .child(
                        v_flex()
                            .w_1_3()
                            .gap_3()
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border)
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
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.select_profile(None, window, cx);
                                            })),
                                    ),
                            )
                            .child(
                                if profiles.is_empty() {
                                    div()
                                        .p_4()
                                        .text_sm()
                                        .text_color(theme.muted_foreground)
                                        .child("Belum ada profil. Klik '+ Profil Baru' untuk membuat.")
                                        .into_any_element()
                                } else {
                                    v_flex()
                                        .gap_2()
                                        .children(profiles.into_iter().map(|p| {
                                            let is_selected = selected_id.as_ref() == Some(p.id());
                                            let pid = p.id().clone();
                                            let p_status = p.evaluate_status(available_monitors);
                                            let p_lifecycle = coord
                                                .get_lifecycle_state(&pid)
                                                .unwrap_or(ProfileLifecycleState::Idle);

                                            let bg = if is_selected {
                                                theme.accent.opacity(0.3)
                                            } else {
                                                theme.muted.opacity(0.4)
                                            };
                                            let border = if is_selected {
                                                theme.primary
                                            } else {
                                                theme.border
                                            };
                                            let item_id =
                                                SharedString::from(format!("profile-item-{}", pid));

                                            let (status_text, status_color) = match &p_lifecycle {
                                                ProfileLifecycleState::Running { .. } => {
                                                    ("● Aktif", theme.primary)
                                                }
                                                ProfileLifecycleState::WaitingForMonitor {
                                                    ..
                                                } => ("Menunggu", theme.warning),
                                                ProfileLifecycleState::Disconnected { .. } => {
                                                    ("Terputus", theme.warning)
                                                }
                                                ProfileLifecycleState::Exited { reason } => {
                                                    match reason {
                                                        ExitReason::Crash => ("Crash", theme.danger),
                                                        ExitReason::ManualClose => {
                                                            ("Berhenti", theme.muted_foreground)
                                                        }
                                                    }
                                                }
                                                ProfileLifecycleState::Idle => match p_status {
                                                    ProfileStatus::DraftNotReady => {
                                                        ("Draft", theme.muted_foreground)
                                                    }
                                                    ProfileStatus::Ready => ("Siap", theme.primary),
                                                    ProfileStatus::TargetUnavailable => {
                                                        ("Tidak Tersedia", theme.warning)
                                                    }
                                                    ProfileStatus::TargetAmbiguous => {
                                                        ("Ambigu", theme.danger)
                                                    }
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
                                                    this.select_profile(
                                                        Some(pid.clone()),
                                                        window,
                                                        cx,
                                                    );
                                                }))
                                                .child(
                                                    v_flex()
                                                        .gap_1()
                                                        .child(
                                                            h_flex()
                                                                .justify_between()
                                                                .items_center()
                                                                .child(
                                                                    div()
                                                                        .font_weight(
                                                                            FontWeight::MEDIUM,
                                                                        )
                                                                        .child(p.name().to_string()),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_xs()
                                                                        .px_2()
                                                                        .py_0p5()
                                                                        .rounded_sm()
                                                                        .bg(theme.muted)
                                                                        .text_color(status_color)
                                                                        .child(status_text),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(theme.muted_foreground)
                                                                .child(
                                                                    p.browser_path()
                                                                        .to_string_lossy()
                                                                        .to_string(),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(theme.muted_foreground)
                                                                .child(match p.target_monitor() {
                                                                    Some(m) => format!("Monitor: {}", m),
                                                                    None => "Monitor: (Belum Dipilih)".to_string(),
                                                                }),
                                                        ),
                                                )
                                        }))
                                        .into_any_element()
                                },
                            ),
                    )
                    // Kolom Kanan: Formulir Detail Profil
                    .child(
                        v_flex()
                            .w_2_3()
                            .gap_4()
                            .p_4()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(if selected_id.is_some() {
                                        "Edit Profil"
                                    } else {
                                        "Tambah Profil Baru"
                                    }),
                            )
                            // Field Nama Profil
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(div().text_sm().child("Nama Profil:"))
                                    .child(Input::new(&self.name_input)),
                            )
                            // Field Executable Browser + Tombol Browse Manual
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(div().text_sm().child("Jalur Executable Browser:"))
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .child(Input::new(&self.path_input)),
                                            )
                                            .child(
                                                Button::new("btn-browse-browser")
                                                    .label("Pilih File .exe...")
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.browse_browser_executable(cx);
                                                    })),
                                            ),
                                    ),
                            )
                            // Field Parameter Peluncuran Kustom
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(div().text_sm().child("Parameter Peluncuran Kustom:"))
                                    .child(Input::new(&self.params_input)),
                            )
                            // Field Monitor Tujuan
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Monitor Tujuan (Direkam Berdasarkan Identitas):"))
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.muted_foreground)
                                                            .child(format!("{} monitor aktif", available_monitors.len())),
                                                    )
                                                    .child(
                                                        Button::new("btn-refresh-monitors")
                                                            .label("Segarkan Monitor")
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.refresh_monitors(cx);
                                                            })),
                                                    ),
                                            ),
                                    )
                                    .when_some(self.monitors_error.as_ref(), |this, err| {
                                        this.child(
                                            div()
                                                .p_2()
                                                .rounded_md()
                                                .bg(theme.danger.opacity(0.1))
                                                .border_1()
                                                .border_color(theme.danger)
                                                .text_xs()
                                                .text_color(theme.danger)
                                                .child(format!("Peringatan sistem display: {}", err)),
                                        )
                                    })
                                    .child(
                                        v_flex()
                                            .gap_2()
                                            // Pilihan 1: Tanpa Monitor Tujuan (Draft)
                                            .child({
                                                let is_draft_selected = self.selected_target_monitor.is_none();
                                                let (bg, border) = if is_draft_selected {
                                                    (theme.accent.opacity(0.3), theme.primary)
                                                } else {
                                                    (theme.muted.opacity(0.3), theme.border)
                                                };
                                                div()
                                                    .id("monitor-option-draft")
                                                    .p_2()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(border)
                                                    .bg(bg)
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.selected_target_monitor = None;
                                                        cx.notify();
                                                    }))
                                                    .child(
                                                        h_flex()
                                                            .justify_between()
                                                            .items_center()
                                                            .child(div().text_xs().font_weight(FontWeight::MEDIUM).child("Draft (Belum Memilih Monitor)"))
                                                            .child(div().text_xs().text_color(theme.primary).child(if is_draft_selected { "✓ Terpilih" } else { "" })),
                                                    )
                                            })
                                            // Pilihan untuk setiap monitor yang terhubung
                                            .children(available_monitors.iter().map(|m| {
                                                let is_this_selected = self.selected_target_monitor.as_ref() == Some(&m.identity);
                                                let (bg, border) = if is_this_selected {
                                                    (theme.accent.opacity(0.3), theme.primary)
                                                } else {
                                                    (theme.muted.opacity(0.3), theme.border)
                                                };
                                                let m_identity = m.identity.clone();
                                                let item_id = SharedString::from(format!("monitor-item-{}", m.identity));

                                                div()
                                                    .id(item_id)
                                                    .p_2()
                                                    .rounded_md()
                                                    .border_1()
                                                    .border_color(border)
                                                    .bg(bg)
                                                    .cursor_pointer()
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.selected_target_monitor = Some(m_identity.clone());
                                                        cx.notify();
                                                    }))
                                                    .child(
                                                        v_flex()
                                                            .gap_0p5()
                                                            .child(
                                                                h_flex()
                                                                    .justify_between()
                                                                    .items_center()
                                                                    .child(
                                                                        div()
                                                                            .text_xs()
                                                                            .font_weight(FontWeight::MEDIUM)
                                                                            .child(format!("{}{}", m.name, if m.is_primary { " (Primary)" } else { "" })),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .text_xs()
                                                                            .text_color(theme.primary)
                                                                            .child(if is_this_selected { "✓ Terpilih" } else { "" }),
                                                                    ),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(theme.muted_foreground)
                                                                    .child(format!("Identitas: {}", m.identity)),
                                                            ),
                                                    )
                                            }))
                                            // Indikator jika monitor yang tersimpan sedang terputus atau diganti
                                            .when_some(
                                                self.selected_target_monitor.as_ref().filter(|id| !available_monitors.iter().any(|m| &m.identity == *id)),
                                                |this, missing_id| {
                                                    this.child(
                                                        div()
                                                            .p_2()
                                                            .rounded_md()
                                                            .border_1()
                                                            .border_color(theme.warning)
                                                            .bg(theme.warning.opacity(0.1))
                                                            .child(
                                                                v_flex()
                                                                    .gap_0p5()
                                                                    .child(
                                                                        div()
                                                                            .text_xs()
                                                                            .font_weight(FontWeight::MEDIUM)
                                                                            .text_color(theme.warning)
                                                                            .child("⚠ Monitor Terpilih Sedang Tidak Terhubung / Telah Diganti:"),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .text_xs()
                                                                            .text_color(theme.muted_foreground)
                                                                            .child(format!("Identitas: {}", missing_id)),
                                                                    ),
                                                            ),
                                                    )
                                                },
                                            ),
                                    ),
                            )
                            // Keterangan Status Kesiapan
                            .child({
                                let target_eval = TargetMonitorStatus::evaluate(
                                    self.selected_target_monitor.as_ref(),
                                    available_monitors,
                                );

                                let (status_text, status_color, status_bg) = match target_eval {
                                    TargetMonitorStatus::Unset => (
                                        "ℹ Status Kesiapan: Draft (Belum Siap Diaktifkan). Monitor tujuan belum dipilih; aplikasi tidak meluncurkan browser.".to_string(),
                                        theme.muted_foreground,
                                        theme.muted,
                                    ),
                                    TargetMonitorStatus::Available(desc) => (
                                        format!("✓ Status Kesiapan: Siap Diaktifkan. Monitor tujuan terhubung: {} (Identitas: {}).", desc.name, desc.identity),
                                        theme.primary,
                                        theme.primary.opacity(0.12),
                                    ),
                                    TargetMonitorStatus::Unavailable { recorded_identity } => (
                                        format!("⚠ Status Kesiapan: Monitor Tujuan Tidak Tersedia. Monitor '{}' tidak terhubung atau telah diganti. Silakan sambungkan kembali monitor yang cocok atau pilih ulang monitor tujuan dari daftar. Tidak ada fallback otomatis ke primary; profil belum dapat diaktifkan.", recorded_identity),
                                        theme.warning,
                                        theme.warning.opacity(0.12),
                                    ),
                                    TargetMonitorStatus::Ambiguous { recorded_identity, match_count } => (
                                        format!("⚠ Status Kesiapan: Monitor Ambigu. Ditemukan {} monitor aktif dengan identitas '{}'. Silakan pilih ulang monitor tujuan yang benar; tidak ada fallback otomatis ke primary.", match_count, recorded_identity),
                                        theme.danger,
                                        theme.danger.opacity(0.12),
                                    ),
                                };

                                div()
                                    .p_3()
                                    .rounded_md()
                                    .bg(status_bg)
                                    .border_1()
                                    .border_color(status_color)
                                    .text_xs()
                                    .text_color(status_color)
                                    .child(status_text)
                            })
                            // Keterangan Status Pergantian / Runtime
                            .when_some(self.selected_profile_id.as_ref(), |this, pid| {
                                let is_active = coord.is_profile_running(pid);
                                if is_active {
                                    this.child(
                                        div()
                                            .p_3()
                                            .rounded_md()
                                            .bg(theme.primary.opacity(0.12))
                                            .border_1()
                                            .border_color(theme.primary)
                                            .text_xs()
                                            .text_color(theme.primary)
                                            .child("● Profil ini sedang aktif berjalan pada monitor tujuan."),
                                    )
                                } else {
                                    // Periksa apakah ada profil lain yang sedang aktif di monitor tujuan yang sama
                                    let conflicting_profile = if let Some(ref target_id) = self.selected_target_monitor {
                                        coord.find_conflicting_active_profile(Some(pid), target_id)
                                    } else {
                                        None
                                    };

                                    if let Some(other) = conflicting_profile {
                                        this.child(
                                            div()
                                                .p_3()
                                                .rounded_md()
                                                .bg(theme.warning.opacity(0.12))
                                                .border_1()
                                                .border_color(theme.warning)
                                                .text_xs()
                                                .text_color(theme.warning)
                                                .child(format!(
                                                    "ℹ Monitor tujuan saat ini sedang aktif digunakan oleh profil '{}'. Mengaktifkan profil ini akan menutup profil tersebut terlebih dahulu.",
                                                    other.name()
                                                )),
                                        )
                                    } else {
                                        this
                                    }
                                }
                            })
                            // Tombol Aksi
                            .child({
                                let is_running = self
                                    .selected_profile_id
                                    .as_ref()
                                    .map(|id| coord.is_profile_running(id))
                                    .unwrap_or(false);

                                h_flex()
                                    .gap_3()
                                    .child(
                                        Button::new("btn-save-profile")
                                            .label("Simpan Profil")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.save_profile(window, cx);
                                            })),
                                    )
                                    .when(self.selected_profile_id.is_some() && !is_running, |this| {
                                        this.child(
                                            Button::new("btn-activate-profile")
                                                .primary()
                                                .label("Aktifkan Profil")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.activate_selected_profile(cx);
                                                })),
                                        )
                                    })
                                    .when(self.selected_profile_id.is_some() && is_running, |this| {
                                        this.child(
                                            Button::new("btn-stop-profile")
                                                .danger()
                                                .label("Hentikan Profil")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.stop_selected_profile(cx);
                                                })),
                                        )
                                    })
                                    .child(
                                        Button::new("btn-reset-form")
                                            .label("Reset")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                let id = this.selected_profile_id.clone();
                                                this.select_profile(id, window, cx);
                                            })),
                                    )
                            }),
                    ),
            )
    }
}
