//! GPUI presentation for Window Hop profile management.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme, Disableable as _, TitleBar};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::application::{ActivationResult, LifecycleCoordinator, StopOutcome};
use crate::browsers::{BrowserCompatibilityStatus, DetectedBrowser};
use crate::domain::{
    DraftProfileInput, ExitReason, MonitorDescriptor, MonitorIdentity, Profile, ProfileId,
    ProfileLifecycleState, ProfileStatus, TargetMonitorStatus,
};

/// Tampilan utama pengaturan profil Window Hop.
pub struct WindowHopView {
    coordinator: Arc<Mutex<LifecycleCoordinator>>,
    cached_profiles: Vec<Profile>,
    cached_lifecycle_states: HashMap<ProfileId, ProfileLifecycleState>,
    cached_running_ids: HashSet<ProfileId>,
    cached_has_running_changes: HashSet<ProfileId>,
    cached_startup_enabled: bool,
    cached_startup_profile_id: Option<ProfileId>,
    is_busy: bool,
    selected_profile_id: Option<ProfileId>,
    selected_target_monitor: Option<MonitorIdentity>,
    cached_monitors: Vec<MonitorDescriptor>,
    monitors_error: Option<String>,
    cached_detected_browsers: Vec<DetectedBrowser>,
    detected_browsers_error: Option<String>,
    window_handle: AnyWindowHandle,
    name_input: Entity<InputState>,
    path_input: Entity<InputState>,
    params_input: Entity<InputState>,
    shortcut_folder_input: Entity<InputState>,
    status_message: Option<String>,
    is_error: bool,
    confirming_delete_profile: Option<ProfileId>,
}

impl WindowHopView {
    /// Membuat instance baru WindowHopView dari koordinator yang dimiliki.
    pub fn new(
        coordinator: LifecycleCoordinator,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::with_shared_coordinator(Arc::new(Mutex::new(coordinator)), window, cx)
    }

    /// Membuat instance baru WindowHopView dengan koordinator yang dibagikan (shared Arc<Mutex>).
    pub fn with_shared_coordinator(
        coordinator: Arc<Mutex<LifecycleCoordinator>>,
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
        let shortcut_folder_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Default: Desktop (atau masukkan jalur folder alternatif)")
        });

        let (
            initial_error,
            cached_monitors,
            monitors_error,
            cached_profiles,
            cached_startup_enabled,
            cached_startup_profile_id,
            cached_detected_browsers,
            detected_browsers_error,
        ) = {
            let coord = coordinator.lock().unwrap();
            let initial_error = coord.last_error().map(|s| s.to_string());
            let (monitors, m_err) = match coord.available_monitors() {
                Ok(m) => (m, None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            };
            let (browsers, b_err) = match coord.detected_browsers() {
                Ok(b) => (b, None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            };
            let profiles = coord.list_profiles();
            let startup_enabled = coord.startup_enabled();
            let startup_profile_id = coord.startup_profile_id().cloned();
            (
                initial_error,
                monitors,
                m_err,
                profiles,
                startup_enabled,
                startup_profile_id,
                browsers,
                b_err,
            )
        };

        let is_error = initial_error.is_some();

        let (cached_lifecycle_states, cached_running_ids, cached_has_running_changes) = {
            let coord = coordinator.lock().unwrap();
            let states = cached_profiles
                .iter()
                .map(|p| {
                    (
                        p.id().clone(),
                        coord
                            .get_lifecycle_state(p.id())
                            .unwrap_or(ProfileLifecycleState::Idle),
                    )
                })
                .collect();
            let running = cached_profiles
                .iter()
                .filter(|p| coord.is_profile_running(p.id()))
                .map(|p| p.id().clone())
                .collect();
            let changes = cached_profiles
                .iter()
                .filter(|p| coord.has_running_config_changes(p.id()))
                .map(|p| p.id().clone())
                .collect();
            (states, running, changes)
        };

        Self {
            coordinator,
            cached_profiles,
            cached_lifecycle_states,
            cached_running_ids,
            cached_has_running_changes,
            cached_startup_enabled,
            cached_startup_profile_id,
            is_busy: false,
            selected_profile_id: None,
            selected_target_monitor: None,
            cached_monitors,
            monitors_error,
            cached_detected_browsers,
            detected_browsers_error,
            window_handle: window.window_handle(),
            name_input,
            path_input,
            params_input,
            shortcut_folder_input,
            status_message: initial_error,
            is_error,
            confirming_delete_profile: None,
        }
    }

    /// Menyinkronkan cache tampilan lokal dari instance koordinator.
    fn sync_cache_from_coordinator(&mut self, coord: &LifecycleCoordinator) {
        self.cached_profiles = coord.list_profiles();
        self.cached_startup_enabled = coord.startup_enabled();
        self.cached_startup_profile_id = coord.startup_profile_id().cloned();
        self.cached_lifecycle_states = self
            .cached_profiles
            .iter()
            .map(|p| {
                (
                    p.id().clone(),
                    coord
                        .get_lifecycle_state(p.id())
                        .unwrap_or(ProfileLifecycleState::Idle),
                )
            })
            .collect();
        self.cached_running_ids = self
            .cached_profiles
            .iter()
            .filter(|p| coord.is_profile_running(p.id()))
            .map(|p| p.id().clone())
            .collect();
        self.cached_has_running_changes = self
            .cached_profiles
            .iter()
            .filter(|p| coord.has_running_config_changes(p.id()))
            .map(|p| p.id().clone())
            .collect();

        // Jika tidak ada pesan status aktif dari aksi pengguna dan koordinator mencatat kesalahan otomatis,
        // tampilkan pesan kesalahan otomatis pada UI detail tanpa popup atau mencuri fokus
        if self.status_message.is_none() {
            if let Some(err) = coord.last_error() {
                self.status_message = Some(err.to_string());
                self.is_error = true;
            }
        }
    }

    /// Memperbarui cache daftar monitor yang terdeteksi secara manual atau saat ada perubahan.
    pub fn refresh_monitors(&mut self, cx: &mut Context<Self>) {
        let coordinator_arc = self.coordinator.clone();
        if let Ok(coord) = coordinator_arc.try_lock() {
            match coord.available_monitors() {
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
        }
        cx.notify();
    }

    /// Memperbarui cache daftar browser yang terdeteksi secara manual atau saat ada perubahan.
    pub fn refresh_detected_browsers(&mut self, cx: &mut Context<Self>) {
        let coordinator_arc = self.coordinator.clone();
        if let Ok(coord) = coordinator_arc.try_lock() {
            match coord.detected_browsers() {
                Ok(browsers) => {
                    self.cached_detected_browsers = browsers;
                    self.detected_browsers_error = None;
                }
                Err(err) => {
                    self.detected_browsers_error = Some(err.to_string());
                }
            }
        }
        cx.notify();
    }

    /// Memilih browser yang terdeteksi, mengisi form jalur executable dan nama secara cerdas.
    pub fn select_detected_browser(
        &mut self,
        browser: &DetectedBrowser,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path_str = browser.executable_path.to_string_lossy().to_string();
        let browser_name = browser.name.clone();

        self.path_input.update(cx, |input, cx| {
            input.set_value(path_str, window, cx);
        });
        self.name_input.update(cx, |input, cx| {
            if input.value().trim().is_empty() {
                input.set_value(format!("Antrian {}", browser_name), window, cx);
            }
        });
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
        self.confirming_delete_profile = None;

        // Segarkan cache monitor dan profil saat memilih profil jika koordinator tidak terkunci
        let coordinator_arc = self.coordinator.clone();
        if let Ok(coord) = coordinator_arc.try_lock() {
            if let Ok(monitors) = coord.available_monitors() {
                self.cached_monitors = monitors;
                self.monitors_error = None;
            }
            self.sync_cache_from_coordinator(&coord);
        }

        if let Some(ref pid) = id {
            let profile_opt = self.cached_profiles.iter().find(|p| p.id() == pid).cloned();
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
                self.shortcut_folder_input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
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
            self.shortcut_folder_input.update(cx, |input, cx| {
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

    /// Membuka dialog pemilih folder alternatif untuk penempatan shortcut.
    pub fn browse_shortcut_folder(&mut self, cx: &mut Context<Self>) {
        let window_handle = self.window_handle;
        let shortcut_folder_input = self.shortcut_folder_input.clone();

        let prompt_task = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Pilih Folder Alternatif Shortcut".into()),
        });

        cx.spawn(move |_this, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                if let Ok(Ok(Some(paths))) = prompt_task.await {
                    if let Some(path) = paths.into_iter().next() {
                        let path_str = path.to_string_lossy().to_string();
                        cx.update(|cx| {
                            let _ = window_handle.update(cx, |_view, window, cx| {
                                shortcut_folder_input.update(cx, |input, cx| {
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

    /// Membuat shortcut untuk profil yang dipilih (default Desktop atau folder alternatif).
    pub fn create_shortcut_for_selected_profile(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy {
            return;
        }
        let Some(ref pid) = self.selected_profile_id else {
            return;
        };

        let raw_dir = self.shortcut_folder_input.read(cx).value().to_string();
        let custom_dir = if raw_dir.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(raw_dir.trim()))
        };

        self.is_busy = true;
        let pid = pid.clone();
        let coordinator = self.coordinator.clone();
        self.status_message = Some("Membuat shortcut...".to_string());
        self.is_error = false;
        cx.notify();

        cx.spawn(move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let cx = cx.clone();
            async move {
                let coord_for_task = coordinator.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        coord_for_task
                            .lock()
                            .unwrap()
                            .create_profile_shortcut(&pid, custom_dir)
                    })
                    .await;

                cx.update(|cx| {
                    let _ = this.update(cx, |view, cx| {
                        view.is_busy = false;
                        match result {
                            Ok(shortcut_path) => {
                                view.status_message = Some(format!(
                                    "Shortcut berhasil dibuat di: {}",
                                    shortcut_path.display()
                                ));
                                view.is_error = false;
                            }
                            Err(err) => {
                                view.status_message =
                                    Some(format!("Gagal membuat shortcut: {}", err));
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

    /// Menyimpan profil draft atau profil dengan monitor tujuan secara asinkron tanpa memblokir thread UI.
    /// Operasi disk dijalankan di background executor, dan state memori hanya dikomit
    /// setelah persistensi berhasil.
    pub fn save_profile(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy {
            return;
        }

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
        self.is_busy = true;
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
                        view.is_busy = false;
                        match save_result {
                            Ok(()) => {
                                let mut coord = coordinator.lock().unwrap();
                                coord.commit_saved_config(candidate_config);
                                view.sync_cache_from_coordinator(&coord);
                                drop(coord);
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

    /// Memulai permintaan penghapusan profil yang sedang dipilih.
    /// Jika profil sedang berjalan aktif, meminta konfirmasi terlebih dahulu (User Story 36).
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
    fn cancel_delete_profile(&mut self, cx: &mut Context<Self>) {
        self.confirming_delete_profile = None;
        cx.notify();
    }

    /// Menjalankan penghapusan profil di background thread dengan penutupan instance 3 detik.
    fn execute_delete_profile(&mut self, pid: ProfileId, cx: &mut Context<Self>) {
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

impl Render for WindowHopView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        // Non-blocking try_lock: jangan memblokir render loop GPUI jika koordinator sedang di-lock oleh task background
        let coordinator_arc = self.coordinator.clone();
        if let Ok(coord) = coordinator_arc.try_lock() {
            self.sync_cache_from_coordinator(&coord);
        }
        let profiles = self.cached_profiles.clone();
        let selected_id = self.selected_profile_id.clone();
        let available_monitors = &self.cached_monitors;
        let is_busy = self.is_busy;

        v_flex()
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Window Hop - Profil Antrian"),
                        ),
                ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .p_6()
                    .gap_4()
                    .overflow_hidden()
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
                                            .child("Pengaturan Profil Antrian"),
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
                                            .disabled(is_busy)
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
                                            let p_lifecycle = self
                                                .cached_lifecycle_states
                                                .get(&pid)
                                                .cloned()
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
                                                                    h_flex()
                                                                        .gap_1p5()
                                                                        .items_center()
                                                                        .child(
                                                                            div()
                                                                                .font_weight(
                                                                                    FontWeight::MEDIUM,
                                                                                )
                                                                                .child(p.name().to_string()),
                                                                        )
                                                                        .when(Some(p.id()) == self.cached_startup_profile_id.as_ref(), |this| {
                                                                            this.child(
                                                                                div()
                                                                                    .text_xs()
                                                                                    .px_1p5()
                                                                                    .py_0p5()
                                                                                    .rounded_sm()
                                                                                    .bg(theme.warning.opacity(0.18))
                                                                                    .text_color(theme.warning)
                                                                                    .font_weight(FontWeight::SEMIBOLD)
                                                                                    .child("★ Startup"),
                                                                            )
                                                                        }),
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
                            )
                            // Bagian Pengaturan Mulai Otomatis (Startup Windows)
                            .child(
                                v_flex()
                                    .p_3()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.muted.opacity(0.3))
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
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(theme.muted_foreground)
                                            .child(match &self.cached_startup_profile_id {
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
                                            }),
                                    ),
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
                            // Pilihan Browser Terdeteksi pada Sistem
                            .child(
                                v_flex()
                                    .gap_2()
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .child("Browser Terdeteksi pada Sistem:"),
                                            )
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.muted_foreground)
                                                            .child(format!(
                                                                "{} terdeteksi",
                                                                self.cached_detected_browsers.len()
                                                            )),
                                                    )
                                                    .child(
                                                        Button::new("btn-refresh-browsers")
                                                            .label("Segarkan Browser")
                                                            .disabled(is_busy)
                                                            .on_click(cx.listener(|this, _, _, cx| {
                                                                this.refresh_detected_browsers(cx);
                                                            })),
                                                    ),
                                            ),
                                    )
                                    .when_some(self.detected_browsers_error.as_ref(), |this, err| {
                                        this.child(
                                            div()
                                                .p_2()
                                                .rounded_md()
                                                .bg(theme.danger.opacity(0.12))
                                                .border_1()
                                                .border_color(theme.danger)
                                                .text_xs()
                                                .text_color(theme.danger)
                                                .child(format!("Peringatan deteksi browser: {}", err)),
                                        )
                                    })
                                    .child({
                                        if self.cached_detected_browsers.is_empty() {
                                            div()
                                                .p_2()
                                                .rounded_md()
                                                .bg(theme.muted)
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .child("Tidak ada instalasi browser yang terdeteksi otomatis. Silakan gunakan pemilihan file executable manual di bawah.")
                                        } else {
                                            let current_path = self.path_input.read(cx).value().trim().to_ascii_lowercase();
                                            let detected_list = self.cached_detected_browsers.clone();

                                            div().child(
                                                h_flex()
                                                    .gap_2()
                                                    .flex_wrap()
                                                    .children(detected_list.into_iter().enumerate().map(|(idx, b)| {
                                                        let is_selected = !current_path.is_empty()
                                                            && b.executable_path.to_string_lossy().to_ascii_lowercase() == current_path;
                                                        let (bg, border) = if is_selected {
                                                            (theme.accent.opacity(0.3), theme.primary)
                                                        } else {
                                                            (theme.muted.opacity(0.3), theme.border)
                                                        };
                                                        let is_edge = b.name.contains("Edge")
                                                            || b.executable_path.to_string_lossy().to_ascii_lowercase().contains("msedge");
                                                        let status_badge = if is_edge {
                                                            ("Terverifikasi", theme.primary)
                                                        } else {
                                                            ("Belum Diverifikasi", theme.warning)
                                                        };

                                                        let item_id = SharedString::from(format!("detected-browser-item-{}", idx));
                                                        let b_clone = b.clone();

                                                        div()
                                                            .id(item_id)
                                                            .p_2()
                                                            .rounded_md()
                                                            .border_1()
                                                            .border_color(border)
                                                            .bg(bg)
                                                            .cursor_pointer()
                                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                                this.select_detected_browser(&b_clone, window, cx);
                                                            }))
                                                            .child(
                                                                v_flex()
                                                                    .gap_0p5()
                                                                    .child(
                                                                        h_flex()
                                                                            .gap_2()
                                                                            .items_center()
                                                                            .justify_between()
                                                                            .child(
                                                                                div()
                                                                                    .text_xs()
                                                                                    .font_weight(FontWeight::MEDIUM)
                                                                                    .child(b.name.clone()),
                                                                            )
                                                                            .child(
                                                                                div()
                                                                                    .text_xs()
                                                                                    .text_color(theme.primary)
                                                                                    .child(if is_selected { "✓ Terpilih" } else { "" }),
                                                                            ),
                                                                    )
                                                                    .child(
                                                                        h_flex()
                                                                            .gap_2()
                                                                            .items_center()
                                                                            .child(
                                                                                div()
                                                                                    .text_xs()
                                                                                    .text_color(theme.muted_foreground)
                                                                                    .child(format!("{}", b.architecture)),
                                                                            )
                                                                            .child(
                                                                                div()
                                                                                    .text_xs()
                                                                                    .text_color(status_badge.1)
                                                                                    .child(format!("[{}]", status_badge.0)),
                                                                            ),
                                                                    ),
                                                            )
                                                    })),
                                            )
                                        }
                                    }),
                            )
                            // Field Executable Browser + Tombol Browse Manual
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        h_flex()
                                            .justify_between()
                                            .items_center()
                                            .child(div().text_sm().child("Jalur Executable Browser (Manual / Portable):"))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(theme.muted_foreground)
                                                    .child("Dapat memilih file .exe portabel atau lokasi instalasi khusus"),
                                            ),
                                    )
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
                                                    .disabled(is_busy)
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.browse_browser_executable(cx);
                                                    })),
                                            ),
                                    ),
                            )
                            // Kartu Evaluasi Kompatibilitas Browser
                            .child({
                                let raw_path = self.path_input.read(cx).value().trim().to_string();
                                if raw_path.is_empty() {
                                    div()
                                        .p_2()
                                        .rounded_md()
                                        .bg(theme.muted)
                                        .border_1()
                                        .border_color(theme.border)
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("ℹ Masukkan jalur file executable atau pilih browser terdeteksi di atas untuk memeriksa status kompatibilitas.")
                                } else {
                                    let compat = crate::browsers::evaluate_browser_compatibility(&PathBuf::from(&raw_path));
                                    match compat {
                                        BrowserCompatibilityStatus::SafeToActivate { adapter: _, evidence } => {
                                            let limit_text = evidence.limitations.unwrap_or_default();
                                            div()
                                                .p_3()
                                                .rounded_md()
                                                .bg(theme.primary.opacity(0.12))
                                                .border_1()
                                                .border_color(theme.primary)
                                                .child(
                                                    v_flex()
                                                        .gap_1()
                                                        .child(
                                                            h_flex()
                                                                .gap_2()
                                                                .items_center()
                                                                .child(
                                                                    div()
                                                                        .font_weight(FontWeight::SEMIBOLD)
                                                                        .text_xs()
                                                                        .text_color(theme.primary)
                                                                        .child("✓ Status Dukungan: Aman Diaktifkan (Terverifikasi)"),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_xs()
                                                                        .text_color(theme.muted_foreground)
                                                                        .child(format!("{} [{}]", evidence.browser_name, evidence.architecture)),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .child(format!("Bukti: {}", evidence.evidence_text)),
                                                        )
                                                        .when(!limit_text.is_empty(), |this| {
                                                            this.child(
                                                                div()
                                                                    .text_xs()
                                                                    .text_color(theme.muted_foreground)
                                                                    .child(format!("Batasan: {}", limit_text)),
                                                            )
                                                        }),
                                                )
                                        }
                                        BrowserCompatibilityStatus::Unverified { evidence } => {
                                            div()
                                                .p_3()
                                                .rounded_md()
                                                .bg(theme.warning.opacity(0.12))
                                                .border_1()
                                                .border_color(theme.warning)
                                                .child(
                                                    v_flex()
                                                        .gap_1()
                                                        .child(
                                                            h_flex()
                                                                .gap_2()
                                                                .items_center()
                                                                .child(
                                                                    div()
                                                                        .font_weight(FontWeight::SEMIBOLD)
                                                                        .text_xs()
                                                                        .text_color(theme.warning)
                                                                        .child("⚠ Status Dukungan: Ditemukan (Belum Diverifikasi)"),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_xs()
                                                                        .text_color(theme.muted_foreground)
                                                                        .child(format!("{} [{}]", evidence.browser_name, evidence.architecture)),
                                                                ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .child(format!("Bukti: {}", evidence.evidence_text)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("ℹ Profil dengan browser ini tetap dapat disimpan untuk persiapan konfigurasi, namun gate aktivasi diblokir demi keamanan data perawat."),
                            ),
                                                )
                                        }
                                        BrowserCompatibilityStatus::Unavailable { path, reason } => {
                                            div()
                                                .p_3()
                                                .rounded_md()
                                                .bg(theme.danger.opacity(0.12))
                                                .border_1()
                                                .border_color(theme.danger)
                                                .child(
                                                    v_flex()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .text_xs()
                                                                .text_color(theme.danger)
                                                                .child("✕ Status Dukungan: Executable Tidak Tersedia / Tidak Ditemukan"),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .child(format!("Kesalahan: {}", reason)),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .text_color(theme.muted_foreground)
                                                                .child(format!("Jalur: {}", path.display())),
                                                        ),
                                                )
                                        }
                                    }
                                }
                            })
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
                                                            .disabled(is_busy)
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
                                let is_active = self.cached_running_ids.contains(pid);
                                if is_active {
                                    let has_changes = self.cached_has_running_changes.contains(pid);

                                    let mut container = v_flex().gap_2().child(
                                        div()
                                            .p_3()
                                            .rounded_md()
                                            .bg(theme.primary.opacity(0.12))
                                            .border_1()
                                            .border_color(theme.primary)
                                            .text_xs()
                                            .text_color(theme.primary)
                                            .child("● Profil ini sedang aktif berjalan pada monitor tujuan."),
                                    );

                                    if has_changes {
                                        container = container.child(
                                            div()
                                                .id("notice-unapplied-changes")
                                                .p_3()
                                                .rounded_md()
                                                .bg(theme.warning.opacity(0.12))
                                                .border_1()
                                                .border_color(theme.warning)
                                                .child(
                                                    v_flex()
                                                        .gap_1()
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .font_weight(FontWeight::BOLD)
                                                                .text_color(theme.warning)
                                                                .child("ℹ Perubahan Konfigurasi Belum Diterapkan"),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_xs()
                                                                .child("Konfigurasi profil telah diubah dan disimpan, tetapi tayangan antrian saat ini masih menggunakan konfigurasi lama. Klik 'Jalankan Ulang' untuk menerapkan konfigurasi terbaru ke monitor tujuan."),
                                                        ),
                                                ),
                                        );
                                    }

                                    this.child(container)
                                } else {
                                    // Periksa apakah ada profil lain yang sedang aktif di monitor tujuan yang sama
                                    let conflicting_profile = if let Some(ref target_id) = self.selected_target_monitor {
                                        self.cached_profiles.iter().find(|other| {
                                            other.id() != pid
                                                && other.target_monitor() == Some(target_id)
                                                && self.cached_running_ids.contains(other.id())
                                        })
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
                            // Field Folder Shortcut (Pilihan Folder Alternatif)
                            .when(self.selected_profile_id.is_some(), |this| {
                                this.child(
                                    v_flex()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::MEDIUM)
                                                .child("Folder Shortcut (Default: Desktop):"),
                                        )
                                        .child(
                                            h_flex()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .flex_1()
                                                        .child(Input::new(&self.shortcut_folder_input)),
                                                )
                                                .child(
                                                    Button::new("btn-browse-shortcut-folder")
                                                        .label("Pilih Folder...")
                                                        .disabled(is_busy)
                                                        .on_click(cx.listener(|this, _, _, cx| {
                                                            this.browse_shortcut_folder(cx);
                                                        })),
                                                ),
                                        ),
                                )
                            })
                            // Dialog Konfirmasi Penghapusan Profil Aktif
                            .when_some(self.confirming_delete_profile.clone(), |this, confirm_pid| {
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

                                this.child(
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
                            })
                            // Pilihan Profil Startup untuk Profil ini
                            .when_some(self.selected_profile_id.as_ref(), |this, pid| {
                                let is_this_startup = self.cached_startup_profile_id.as_ref() == Some(pid);
                                let pid_clone = pid.clone();
                                this.child(
                                    h_flex()
                                        .justify_between()
                                        .items_center()
                                        .p_2p5()
                                        .rounded_md()
                                        .bg(if is_this_startup {
                                            theme.warning.opacity(0.12)
                                        } else {
                                            theme.muted.opacity(0.2)
                                        })
                                        .border_1()
                                        .border_color(if is_this_startup {
                                            theme.warning
                                        } else {
                                            theme.border
                                        })
                                        .child(
                                            v_flex()
                                                .gap_0p5()
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child("Pengaturan Profil Startup:"),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(if is_this_startup {
                                                            theme.warning
                                                        } else {
                                                            theme.muted_foreground
                                                        })
                                                        .child(if is_this_startup {
                                                            "★ Profil ini dipilih untuk berjalan otomatis setelah login Windows"
                                                        } else {
                                                            "Bukan profil startup saat ini"
                                                        }),
                                                ),
                                        )
                                        .child(
                                            Button::new(if is_this_startup {
                                                "btn-unset-startup-profile"
                                            } else {
                                                "btn-set-startup-profile"
                                            })
                                            .label(if is_this_startup {
                                                "Batalkan Profil Startup"
                                            } else {
                                                "Jadikan Profil Startup"
                                            })
                                            .disabled(is_busy)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if is_this_startup {
                                                    this.set_startup_profile(None, cx);
                                                } else {
                                                    this.set_startup_profile(Some(pid_clone.clone()), cx);
                                                }
                                            })),
                                        ),
                                )
                            })
                            // Tombol Aksi
                            .child({
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
                            }),
                    ),
            ),
        )
    }
}
