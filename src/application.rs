//! Application coordination, lifecycle management, and profile operations.

use crate::browsers::{BrowserError, LoginRetentionNote};
use crate::domain::{
    AppConfig, DraftProfileInput, ExitReason, MonitorDescriptor, MonitorIdentity, ProcessId,
    Profile, ProfileId, ProfileLifecycleState, ProfileStatus, TargetMonitorStatus,
};
use crate::persistence::{PersistenceError, ProfileStore};
use crate::platform::{
    default_clock, default_monitor_provider, default_process_launcher, Clock, ManagedProcess,
    MonitorProvider, PlatformError, ProcessLauncher,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Hasil aktivasi profil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationResult {
    /// Profil baru diluncurkan sebagai instance proses baru.
    Launched {
        pid: ProcessId,
        target_monitor: MonitorDescriptor,
        login_retention: LoginRetentionNote,
    },
    /// Profil yang sudah berjalan digunakan kembali (reuse window) dan penempatannya dipastikan ulang.
    Reused {
        pid: ProcessId,
        target_monitor: MonitorDescriptor,
        login_retention: LoginRetentionNote,
    },
    /// Profil diminta aktif namun monitor tujuannya belum tersedia;
    /// masuk status menunggu tanpa meluncurkan browser.
    WaitingForMonitor { target_monitor: MonitorIdentity },
}

/// Informasi instance profil yang sedang aktif.
#[derive(Clone)]
struct ActiveProfileInstance {
    process: Arc<dyn ManagedProcess>,
    launch_browser_path: PathBuf,
    launch_parameters: String,
    target_monitor: MonitorDescriptor,
    login_retention: LoginRetentionNote,
    stopping: bool,
}

/// Hasil operasi penghentian profil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopOutcome {
    /// Ditutup secara normal (graceful) sebelum batas tenggat 3 detik.
    Graceful { pid: ProcessId },
    /// Dihentikan secara paksa (force stop) setelah batas tenggat 3 detik terlampaui.
    Forced { pid: ProcessId },
    /// Profil tidak sedang berjalan.
    NotRunning,
}

/// Hasil transisi siklus hidup profil yang teramati akibat perubahan status monitor atau proses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleTransitionOutcome {
    /// Profil ditutup karena monitor tujuannya terputus.
    Disconnected {
        profile_id: ProfileId,
        pid: ProcessId,
        forced: bool,
    },
    /// Profil dipulihkan secara otomatis (diluncurkan) karena monitor tujuannya kembali tersambung.
    Restored {
        profile_id: ProfileId,
        pid: ProcessId,
        target_monitor: MonitorDescriptor,
    },
    /// Profil terdeteksi keluar (crash atau ditutup manual oleh pengguna).
    Exited {
        profile_id: ProfileId,
        pid: ProcessId,
        reason: ExitReason,
    },
}

/// Kesalahan pada lapisan koordinasi aplikasi.
#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    #[error("Kesalahan penyimpanan: {0}")]
    Persistence(#[from] PersistenceError),

    #[error("Kesalahan platform OS: {0}")]
    Platform(#[from] PlatformError),

    #[error("Kesalahan browser: {0}")]
    Browser(#[from] BrowserError),

    #[error("Dukungan pemisahan sesi dan kepemilikan instance untuk browser '{0}' belum diverifikasi: {1}")]
    BrowserNotVerified(String, String),

    #[error("Validasi input gagal: {0}")]
    ValidationError(String),

    #[error("Profil {id} masih berupa draft dan belum memiliki monitor tujuan sehingga belum siap diaktifkan")]
    DraftNotReady { id: ProfileId },

    #[error("Monitor tujuan '{recorded_identity}' untuk profil {id} tidak tersedia")]
    TargetMonitorUnavailable {
        id: ProfileId,
        recorded_identity: MonitorIdentity,
    },

    #[error("Monitor tujuan '{recorded_identity}' untuk profil {id} ambigu ({match_count} monitor cocok); perlu pemilihan ulang")]
    TargetMonitorAmbiguous {
        id: ProfileId,
        recorded_identity: MonitorIdentity,
        match_count: usize,
    },

    #[error("Profil dengan ID {id} tidak ditemukan")]
    ProfileNotFound { id: ProfileId },

    #[error("Penyimpanan dicegah karena data konfigurasi sebelumnya rusak atau gagal dimuat: {0}")]
    BlockedByCorruptState(String),
}

/// Mengembalikan lokasi direktori dasar data sesi Window Hop.
fn default_base_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local_app_data).join("WindowHop");
        }
    }
    std::env::temp_dir().join("window_hop")
}

/// Koordinator siklus hidup dan konfigurasi Window Hop.
pub struct LifecycleCoordinator {
    store: Arc<dyn ProfileStore>,
    monitor_provider: Arc<dyn MonitorProvider>,
    process_launcher: Arc<dyn ProcessLauncher>,
    clock: Arc<dyn Clock>,
    base_data_dir: PathBuf,
    config: AppConfig,
    active_instances: HashMap<ProfileId, ActiveProfileInstance>,
    lifecycle_states: HashMap<ProfileId, ProfileLifecycleState>,
    last_error: Option<String>,
    load_failed: bool,
}

impl LifecycleCoordinator {
    /// Membuat instance baru LifecycleCoordinator dengan dependensi layanan yang dapat disubstitusi.
    pub fn with_services(
        store: Arc<dyn ProfileStore>,
        monitor_provider: Arc<dyn MonitorProvider>,
        process_launcher: Arc<dyn ProcessLauncher>,
        clock: Arc<dyn Clock>,
        base_data_dir: PathBuf,
    ) -> Self {
        Self {
            store,
            monitor_provider,
            process_launcher,
            clock,
            base_data_dir,
            config: AppConfig::default(),
            active_instances: HashMap::new(),
            lifecycle_states: HashMap::new(),
            last_error: None,
            load_failed: false,
        }
    }

    /// Membuat instance baru LifecycleCoordinator dengan ProfileStore dan MonitorProvider yang ditentukan.
    pub fn new(store: Arc<dyn ProfileStore>, monitor_provider: Arc<dyn MonitorProvider>) -> Self {
        Self::with_services(
            store,
            monitor_provider,
            default_process_launcher(),
            default_clock(),
            default_base_data_dir(),
        )
    }

    /// Membuat instance baru LifecycleCoordinator dengan MonitorProvider default sistem.
    pub fn from_store(store: Arc<dyn ProfileStore>) -> Self {
        Self::new(store, default_monitor_provider())
    }

    /// Memvalidasi data input profil draft.
    fn validate_input(input: &DraftProfileInput) -> Result<String, ApplicationError> {
        let trimmed_name = input.name.trim();
        if trimmed_name.is_empty() {
            return Err(ApplicationError::ValidationError(
                "Nama profil tidak boleh kosong".to_string(),
            ));
        }

        if input.browser_path.as_os_str().is_empty() {
            return Err(ApplicationError::ValidationError(
                "Jalur executable browser tidak boleh kosong".to_string(),
            ));
        }

        Ok(trimmed_name.to_string())
    }

    /// Memuat konfigurasi dari store.
    pub fn load(&mut self) -> Result<(), ApplicationError> {
        match self.store.load() {
            Ok(config) => {
                self.config = config;
                self.last_error = None;
                self.load_failed = false;
                Ok(())
            }
            Err(err) => {
                let msg = err.to_string();
                self.last_error = Some(msg);
                self.load_failed = true;
                Err(ApplicationError::Persistence(err))
            }
        }
    }

    /// Menyimpan profil draft secara transaksional: data disimpan ke store terlebih dahulu,
    /// dan in-memory state hanya diperbarui jika penyimpanan berhasil (tidak merusak konfigurasi
    /// yang valid jika terjadi kegagalan I/O).
    pub fn save_draft_profile(
        &mut self,
        id: Option<&ProfileId>,
        input: DraftProfileInput,
    ) -> Result<ProfileId, ApplicationError> {
        if self.load_failed {
            let reason = self
                .last_error
                .clone()
                .unwrap_or_else(|| "Gagal memuat konfigurasi awal".to_string());
            return Err(ApplicationError::BlockedByCorruptState(reason));
        }

        let trimmed_name = Self::validate_input(&input)?;
        let mut candidate_config = self.config.clone();

        let profile_id = match id {
            Some(existing_id) => {
                let profile = candidate_config
                    .profiles_mut()
                    .iter_mut()
                    .find(|p| p.id() == existing_id)
                    .ok_or_else(|| ApplicationError::ProfileNotFound {
                        id: existing_id.clone(),
                    })?;
                profile.set_name(trimmed_name);
                profile.set_browser_path(input.browser_path);
                profile.set_custom_parameters(input.custom_parameters);
                profile.set_target_monitor(input.target_monitor);
                existing_id.clone()
            }
            None => {
                let new_id = ProfileId::generate();
                let mut profile = Profile::new_draft(
                    new_id.clone(),
                    trimmed_name,
                    input.browser_path,
                    input.custom_parameters,
                );
                profile.set_target_monitor(input.target_monitor);
                candidate_config.profiles_mut().push(profile);
                new_id
            }
        };

        // Simpan ke underlying store terlebih dahulu
        match self.store.save(&candidate_config) {
            Ok(()) => {
                // Komit ke in-memory state hanya setelah I/O berhasil
                self.config = candidate_config;
                self.last_error = None;
                Ok(profile_id)
            }
            Err(err) => {
                let msg = err.to_string();
                self.last_error = Some(msg);
                Err(ApplicationError::Persistence(err))
            }
        }
    }

    /// Menyiapkan konfigurasi kandidat untuk penyimpanan asinkron tanpa memblokir thread UI.
    pub fn prepare_draft_save(
        &self,
        id: Option<&ProfileId>,
        input: &DraftProfileInput,
    ) -> Result<(AppConfig, ProfileId), ApplicationError> {
        if self.load_failed {
            let reason = self
                .last_error
                .clone()
                .unwrap_or_else(|| "Gagal memuat konfigurasi awal".to_string());
            return Err(ApplicationError::BlockedByCorruptState(reason));
        }

        let trimmed_name = Self::validate_input(input)?;
        let mut candidate_config = self.config.clone();

        let profile_id = match id {
            Some(existing_id) => {
                let profile = candidate_config
                    .profiles_mut()
                    .iter_mut()
                    .find(|p| p.id() == existing_id)
                    .ok_or_else(|| ApplicationError::ProfileNotFound {
                        id: existing_id.clone(),
                    })?;
                profile.set_name(trimmed_name);
                profile.set_browser_path(input.browser_path.clone());
                profile.set_custom_parameters(input.custom_parameters.clone());
                profile.set_target_monitor(input.target_monitor.clone());
                existing_id.clone()
            }
            None => {
                let new_id = ProfileId::generate();
                let mut profile = Profile::new_draft(
                    new_id.clone(),
                    trimmed_name,
                    input.browser_path.clone(),
                    input.custom_parameters.clone(),
                );
                profile.set_target_monitor(input.target_monitor.clone());
                candidate_config.profiles_mut().push(profile);
                new_id
            }
        };

        Ok((candidate_config, profile_id))
    }

    /// Melakukan komit konfigurasi yang telah berhasil disimpan ke disk ke state memori.
    pub fn commit_saved_config(&mut self, config: AppConfig) {
        self.config = config;
        self.last_error = None;
    }

    /// Membuat profil draft baru secara langsung dalam memori (untuk backward-compatibility atau staging).
    pub fn create_draft_profile(
        &mut self,
        input: DraftProfileInput,
    ) -> Result<ProfileId, ApplicationError> {
        let trimmed_name = Self::validate_input(&input)?;
        let id = ProfileId::generate();
        let mut profile = Profile::new_draft(
            id.clone(),
            trimmed_name,
            input.browser_path,
            input.custom_parameters,
        );
        profile.set_target_monitor(input.target_monitor);
        self.config.profiles_mut().push(profile);
        Ok(id)
    }

    /// Menyimpan konfigurasi profil saat ini ke store.
    pub fn save(&mut self) -> Result<(), ApplicationError> {
        if self.load_failed {
            let reason = self
                .last_error
                .clone()
                .unwrap_or_else(|| "Gagal memuat konfigurasi awal".to_string());
            return Err(ApplicationError::BlockedByCorruptState(reason));
        }

        match self.store.save(&self.config) {
            Ok(()) => {
                self.last_error = None;
                Ok(())
            }
            Err(err) => {
                let msg = err.to_string();
                self.last_error = Some(msg);
                Err(ApplicationError::Persistence(err))
            }
        }
    }

    /// Mengambil daftar semua profil.
    pub fn list_profiles(&self) -> Vec<Profile> {
        self.config.profiles().to_vec()
    }

    /// Mengambil profil berdasarkan ID.
    pub fn get_profile(&self, id: &ProfileId) -> Option<&Profile> {
        self.config.profiles().iter().find(|p| p.id() == id)
    }

    /// Mengambil referensi penyedia monitor.
    pub fn monitor_provider(&self) -> &Arc<dyn MonitorProvider> {
        &self.monitor_provider
    }

    /// Mengambil daftar semua monitor yang terdeteksi dan aktif saat ini.
    pub fn available_monitors(&self) -> Result<Vec<MonitorDescriptor>, ApplicationError> {
        self.monitor_provider
            .available_monitors()
            .map_err(ApplicationError::Platform)
    }

    /// Mengevaluasi status kecocokan monitor tujuan untuk profil tertentu.
    pub fn get_target_monitor_status(
        &self,
        id: &ProfileId,
    ) -> Result<TargetMonitorStatus, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?;

        if profile.target_monitor().is_none() {
            return Ok(TargetMonitorStatus::Unset);
        }

        let monitors = self.available_monitors()?;
        Ok(profile.evaluate_target_monitor(&monitors))
    }

    /// Mengevaluasi status kesiapan profil tertentu berdasarkan ketersediaan monitor tujuan saat ini.
    pub fn get_profile_status(&self, id: &ProfileId) -> Result<ProfileStatus, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?;

        if profile.target_monitor().is_none() {
            return Ok(ProfileStatus::DraftNotReady);
        }

        let monitors = self.available_monitors()?;
        Ok(profile.evaluate_status(&monitors))
    }

    /// Mengambil status siklus hidup runtime suatu profil.
    pub fn get_lifecycle_state(
        &self,
        id: &ProfileId,
    ) -> Result<ProfileLifecycleState, ApplicationError> {
        if self.get_profile(id).is_none() {
            return Err(ApplicationError::ProfileNotFound { id: id.clone() });
        }

        if let Some(active) = self.active_instances.get(id) {
            if active.process.is_alive() {
                return Ok(ProfileLifecycleState::Running {
                    pid: active.process.pid(),
                    target_monitor: active.target_monitor.clone(),
                });
            } else if !active.stopping {
                let reason = active
                    .process
                    .exit_reason()
                    .unwrap_or(ExitReason::ManualClose);
                return Ok(ProfileLifecycleState::Exited { reason });
            }
        }

        Ok(self
            .lifecycle_states
            .get(id)
            .cloned()
            .unwrap_or(ProfileLifecycleState::Idle))
    }

    /// Memeriksa apakah suatu profil dapat diaktifkan.
    /// Profil hanya diizinkan untuk diaktifkan jika monitor tujuan cocok dan tersedia (tanpa fallback),
    /// adapter browser memiliki bukti pemisahan sesi dan penanganan kepemilikan,
    /// serta parameter peluncuran kustom tidak berbenturan dengan isolasi sesi profil.
    pub fn can_activate(&self, id: &ProfileId) -> Result<bool, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?;

        if profile.target_monitor().is_none() {
            return Err(ApplicationError::DraftNotReady { id: id.clone() });
        }

        let monitors = self.available_monitors()?;
        match profile.evaluate_target_monitor(&monitors) {
            TargetMonitorStatus::Unset => {
                return Err(ApplicationError::DraftNotReady { id: id.clone() })
            }
            TargetMonitorStatus::Unavailable { recorded_identity } => {
                return Err(ApplicationError::TargetMonitorUnavailable {
                    id: id.clone(),
                    recorded_identity,
                });
            }
            TargetMonitorStatus::Ambiguous {
                recorded_identity,
                match_count,
            } => {
                return Err(ApplicationError::TargetMonitorAmbiguous {
                    id: id.clone(),
                    recorded_identity,
                    match_count,
                });
            }
            TargetMonitorStatus::Available(_) => {}
        }

        // Verifikasi kapabilitas browser
        let adapter = match crate::browsers::detect_browser_support(profile.browser_path()) {
            crate::browsers::BrowserSupportStatus::Verified(adapter) => adapter,
            crate::browsers::BrowserSupportStatus::Unverified => {
                return Err(ApplicationError::BrowserNotVerified(
                    profile.browser_path().display().to_string(),
                    format!(
                        "Dukungan pemisahan sesi dan kepemilikan instance untuk browser '{}' belum diverifikasi.",
                        profile.browser_path().display()
                    ),
                ));
            }
        };

        // Verifikasi parameter kustom: tolak argumen yang berbenturan sebelum aktivasi
        adapter.validate_custom_parameters(profile.custom_parameters())?;

        Ok(true)
    }

    /// Mengaktifkan profil Window Hop pada monitor tujuannya dengan pemisahan sesi browser.
    ///
    /// Aturan:
    /// - Hanya diizinkan jika monitor tujuan cocok dan browser terverifikasi.
    /// - Memeriksa argumen kustom; menolak argumen yang berbenturan dengan isolasi sesi.
    /// - Jika profil sudah memiliki window/instance yang berjalan dengan konfigurasi yang sama,
    ///   window tersebut digunakan kembali (reuse window) dan penempatannya dipastikan ulang ke
    ///   monitor tujuan tanpa meluncurkan proses baru.
    /// - Jika konfigurasi telah diubah dan disimpan, aktivasi menerapkan konfigurasi terbaru
    ///   dengan menghentikan browser lama sebelum menjalankan yang baru.
    /// - Jika belum berjalan, meluncurkan browser terisolasi dengan direktori data sesi khusus profil.
    pub fn activate_profile(
        &mut self,
        id: &ProfileId,
    ) -> Result<ActivationResult, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?
            .clone();

        if profile.target_monitor().is_none() {
            return Err(ApplicationError::DraftNotReady { id: id.clone() });
        }

        // Verifikasi kapabilitas browser
        let adapter = match crate::browsers::detect_browser_support(profile.browser_path()) {
            crate::browsers::BrowserSupportStatus::Verified(adapter) => adapter,
            crate::browsers::BrowserSupportStatus::Unverified => {
                return Err(ApplicationError::BrowserNotVerified(
                    profile.browser_path().display().to_string(),
                    format!(
                        "Dukungan pemisahan sesi dan kepemilikan instance untuk browser '{}' belum diverifikasi.",
                        profile.browser_path().display()
                    ),
                ));
            }
        };

        // Verifikasi parameter kustom: tolak argumen yang berbenturan sebelum aktivasi
        adapter.validate_custom_parameters(profile.custom_parameters())?;

        let monitors = self.available_monitors()?;
        let target_desc = match profile.evaluate_target_monitor(&monitors) {
            TargetMonitorStatus::Unset => {
                return Err(ApplicationError::DraftNotReady { id: id.clone() });
            }
            TargetMonitorStatus::Unavailable { recorded_identity } => {
                // Monitor tujuan belum tersedia: masuk status menunggu tanpa meluncurkan browser
                self.lifecycle_states.insert(
                    id.clone(),
                    ProfileLifecycleState::WaitingForMonitor {
                        target_monitor: recorded_identity.clone(),
                    },
                );
                return Ok(ActivationResult::WaitingForMonitor {
                    target_monitor: recorded_identity,
                });
            }
            TargetMonitorStatus::Ambiguous {
                recorded_identity,
                match_count,
            } => {
                return Err(ApplicationError::TargetMonitorAmbiguous {
                    id: id.clone(),
                    recorded_identity,
                    match_count,
                });
            }
            TargetMonitorStatus::Available(desc) => desc,
        };

        // Periksa apakah instance profil sudah berjalan (reuse window)
        if let Some(existing) = self.active_instances.get(id) {
            if existing.process.is_alive() {
                let is_same_config = existing.launch_browser_path == *profile.browser_path()
                    && existing.launch_parameters == profile.custom_parameters()
                    && existing.target_monitor == target_desc;

                if is_same_config {
                    existing
                        .process
                        .reposition(&target_desc)
                        .map_err(ApplicationError::Platform)?;
                    self.lifecycle_states.insert(
                        id.clone(),
                        ProfileLifecycleState::Running {
                            pid: existing.process.pid(),
                            target_monitor: target_desc.clone(),
                        },
                    );
                    return Ok(ActivationResult::Reused {
                        pid: existing.process.pid(),
                        target_monitor: target_desc,
                        login_retention: existing.login_retention.clone(),
                    });
                } else {
                    // Konfigurasi profil telah berubah: hentikan instance lama terlebih dahulu
                    self.stop_profile(id)?;
                }
            } else {
                self.active_instances.remove(id);
            }
        }

        let (pid, login_retention) = self.launch_profile_instance(&profile, &target_desc)?;

        Ok(ActivationResult::Launched {
            pid,
            target_monitor: target_desc,
            login_retention,
        })
    }

    /// Meluncurkan instance proses baru untuk profil pada monitor tujuan yang diberikan.
    fn launch_profile_instance(
        &mut self,
        profile: &Profile,
        target_desc: &MonitorDescriptor,
    ) -> Result<(ProcessId, LoginRetentionNote), ApplicationError> {
        let adapter = match crate::browsers::detect_browser_support(profile.browser_path()) {
            crate::browsers::BrowserSupportStatus::Verified(adapter) => adapter,
            crate::browsers::BrowserSupportStatus::Unverified => {
                return Err(ApplicationError::BrowserNotVerified(
                    profile.browser_path().display().to_string(),
                    format!(
                        "Dukungan pemisahan sesi dan kepemilikan instance untuk browser '{}' belum diverifikasi.",
                        profile.browser_path().display()
                    ),
                ));
            }
        };

        adapter.validate_custom_parameters(profile.custom_parameters())?;

        let profile_data_dir = self
            .base_data_dir
            .join("profiles")
            .join(profile.id().as_str())
            .join("browser_session");

        let launch_plan = adapter.build_launch_plan(
            profile.browser_path(),
            &profile_data_dir,
            profile.custom_parameters(),
        )?;

        let managed_proc = self
            .process_launcher
            .launch(&launch_plan, target_desc)
            .map_err(ApplicationError::Platform)?;

        let pid = managed_proc.pid();
        let login_retention = launch_plan.login_retention.clone();

        self.active_instances.insert(
            profile.id().clone(),
            ActiveProfileInstance {
                process: managed_proc,
                launch_browser_path: profile.browser_path().to_path_buf(),
                launch_parameters: profile.custom_parameters().to_string(),
                target_monitor: target_desc.clone(),
                login_retention: login_retention.clone(),
                stopping: false,
            },
        );

        self.lifecycle_states.insert(
            profile.id().clone(),
            ProfileLifecycleState::Running {
                pid,
                target_monitor: target_desc.clone(),
            },
        );

        Ok((pid, login_retention))
    }

    /// Menangani perubahan daftar monitor yang terhubung ke sistem.
    pub fn handle_monitors_changed(
        &mut self,
    ) -> Result<Vec<LifecycleTransitionOutcome>, ApplicationError> {
        let monitors = self.available_monitors()?;
        self.process_monitors_changed(&monitors)
    }

    /// Memproses daftar monitor aktif untuk memulihkan profil yang menunggu / terputus
    /// atau menghentikan profil yang kehilangan koneksi monitor.
    pub fn process_monitors_changed(
        &mut self,
        available_monitors: &[MonitorDescriptor],
    ) -> Result<Vec<LifecycleTransitionOutcome>, ApplicationError> {
        let mut transitions = Vec::new();

        // 0. Periksa instance aktif yang keluar secara spontan (crash atau ditutup manual)
        let active_keys: Vec<ProfileId> = self.active_instances.keys().cloned().collect();
        for profile_id in active_keys {
            if let Some(instance) = self.active_instances.get(&profile_id) {
                if !instance.process.is_alive() && !instance.stopping {
                    let pid = instance.process.pid();
                    let reason = instance
                        .process
                        .exit_reason()
                        .unwrap_or(ExitReason::ManualClose);
                    self.active_instances.remove(&profile_id);
                    self.lifecycle_states
                        .insert(profile_id.clone(), ProfileLifecycleState::Exited { reason });
                    transitions.push(LifecycleTransitionOutcome::Exited {
                        profile_id,
                        pid,
                        reason,
                    });
                }
            }
        }

        // 1. Periksa instance aktif yang monitor tujuannya terputus atau menjadi ambigu
        let active_keys: Vec<ProfileId> = self.active_instances.keys().cloned().collect();
        for profile_id in active_keys {
            let Some(mut instance) = self.active_instances.remove(&profile_id) else {
                continue;
            };

            let target_id = instance.target_monitor.identity.clone();
            let eval = TargetMonitorStatus::evaluate(Some(&target_id), available_monitors);

            match eval {
                TargetMonitorStatus::Available(desc) => {
                    // Monitor masih tersedia normal; perbarui descriptor jika ada pembaruan posisi
                    instance.target_monitor = desc;
                    self.active_instances.insert(profile_id, instance);
                }
                TargetMonitorStatus::Unavailable { .. } | TargetMonitorStatus::Ambiguous { .. } => {
                    // Monitor terputus atau ambigu: minta tutup normal dan terapkan deadline 3 detik
                    instance.stopping = true;
                    let outcome = Self::stop_active_instance_with_deadline(&self.clock, &instance)?;

                    let (pid, forced) = match outcome {
                        StopOutcome::Graceful { pid } => (pid, false),
                        StopOutcome::Forced { pid } => (pid, true),
                        StopOutcome::NotRunning => (instance.process.pid(), false),
                    };

                    self.lifecycle_states.insert(
                        profile_id.clone(),
                        ProfileLifecycleState::Disconnected {
                            target_monitor: target_id,
                        },
                    );

                    transitions.push(LifecycleTransitionOutcome::Disconnected {
                        profile_id,
                        pid,
                        forced,
                    });
                }
                TargetMonitorStatus::Unset => {
                    self.active_instances.insert(profile_id, instance);
                }
            }
        }

        // 2. Pulihkan profil yang berstatus WaitingForMonitor atau Disconnected
        let profiles = self.config.profiles().to_vec();
        for profile in profiles {
            let pid = profile.id().clone();
            let state = self
                .lifecycle_states
                .get(&pid)
                .cloned()
                .unwrap_or(ProfileLifecycleState::Idle);

            match state {
                ProfileLifecycleState::WaitingForMonitor { target_monitor }
                | ProfileLifecycleState::Disconnected { target_monitor } => {
                    let eval =
                        TargetMonitorStatus::evaluate(Some(&target_monitor), available_monitors);
                    if let TargetMonitorStatus::Available(desc) = eval {
                        let (launched_pid, _) = self.launch_profile_instance(&profile, &desc)?;
                        transitions.push(LifecycleTransitionOutcome::Restored {
                            profile_id: pid,
                            pid: launched_pid,
                            target_monitor: desc,
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(transitions)
    }

    /// Menghentikan instance proses yang sedang berjalan dengan penutupan anggun dan batas tenggat 3 detik.
    fn stop_active_instance_with_deadline(
        clock: &Arc<dyn Clock>,
        instance: &ActiveProfileInstance,
    ) -> Result<StopOutcome, ApplicationError> {
        let pid = instance.process.pid();
        if !instance.process.is_alive() {
            return Ok(StopOutcome::NotRunning);
        }

        // 1. Minta penutupan anggun (graceful close)
        instance
            .process
            .request_close()
            .map_err(ApplicationError::Platform)?;

        // 2. Tunggu hingga proses berhenti atau batas 3 detik terlampaui menggunakan Clock
        let start_time = clock.now();
        let timeout = std::time::Duration::from_secs(3);
        let poll_interval = std::time::Duration::from_millis(100);

        while instance.process.is_alive() && clock.now().duration_since(start_time) < timeout {
            clock.sleep(poll_interval);
        }

        // 3. Evaluasi apakah sudah berhenti atau perlu force stop
        if !instance.process.is_alive() {
            Ok(StopOutcome::Graceful { pid })
        } else {
            // Force stop hanya pada instance profil terverifikasi
            instance
                .process
                .force_stop()
                .map_err(ApplicationError::Platform)?;
            Ok(StopOutcome::Forced { pid })
        }
    }

    /// Memeriksa apakah profil sedang berjalan aktif dengan instance proses yang masih hidup.
    pub fn is_profile_running(&self, id: &ProfileId) -> bool {
        self.active_instances
            .get(id)
            .map(|inst| inst.process.is_alive())
            .unwrap_or(false)
    }

    /// Mengambil referensi ke instance proses yang sedang aktif untuk profil tertentu (jika ada).
    pub fn active_instance(&self, id: &ProfileId) -> Option<Arc<dyn ManagedProcess>> {
        self.active_instances
            .get(id)
            .map(|inst| inst.process.clone())
    }

    /// Menghentikan profil yang sedang berjalan.
    ///
    /// Aturan:
    /// - Mengirimkan permintaan penutupan normal (graceful close) terlebih dahulu.
    /// - Menunggu hingga proses berhenti dengan batas tenggat 3 detik.
    /// - Jika belum berhenti setelah 3 detik, menghentikan paksa (force stop) hanya instance milik profil yang terverifikasi.
    /// - Proses lain (termasuk browser kerja perawat) tidak disentuh sama sekali.
    pub fn stop_profile(&mut self, id: &ProfileId) -> Result<StopOutcome, ApplicationError> {
        let instance = match self.active_instances.remove(id) {
            Some(mut inst) => {
                if !inst.process.is_alive() {
                    return Ok(StopOutcome::NotRunning);
                }
                inst.stopping = true;
                inst
            }
            None => return Ok(StopOutcome::NotRunning),
        };

        let outcome = Self::stop_active_instance_with_deadline(&self.clock, &instance)?;
        self.lifecycle_states
            .insert(id.clone(), ProfileLifecycleState::Idle);
        Ok(outcome)
    }

    /// Menangani keluarnya proses browser yang dikelola.
    ///
    /// Jika proses keluar secara spontan (bukan karena permintaan penutupan oleh aplikasi),
    /// profil beralih ke status `Exited { reason }`, menonaktifkan pemulihan reconnect otomatis
    /// tanpa retry loop.
    /// Event terlambat dari instance lama (stale PID) diabaikan dengan aman tanpa mengubah status profil.
    pub fn handle_process_exit(
        &mut self,
        pid: ProcessId,
        reason: ExitReason,
    ) -> Option<LifecycleTransitionOutcome> {
        let matching_profile_id = self
            .active_instances
            .iter()
            .find(|(_, inst)| inst.process.pid() == pid)
            .map(|(id, _)| id.clone());

        let Some(profile_id) = matching_profile_id else {
            // Event terlambat dari instance lama atau PID tidak dikenal: abaikan dengan aman
            return None;
        };

        let instance = self.active_instances.remove(&profile_id)?;
        if instance.stopping {
            // Proses keluar karena permintaan penutupan aplikasi (misal monitor terputus);
            // penanganannya sudah/sedang diurus oleh koordinator.
            return None;
        }

        self.lifecycle_states
            .insert(profile_id.clone(), ProfileLifecycleState::Exited { reason });

        Some(LifecycleTransitionOutcome::Exited {
            profile_id,
            pid,
            reason,
        })
    }

    /// Mengambil pesan error terakhir yang terjadi (jika ada) untuk ditampilkan pada UI.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Membersihkan status pesan error.
    pub fn clear_error(&mut self) {
        self.last_error = None;
    }

    /// Mengambil konfigurasi penuh saat ini.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Mengambil referensi store yang mendasari.
    pub fn store(&self) -> &Arc<dyn ProfileStore> {
        &self.store
    }
}
