//! Application coordination, lifecycle management, and profile operations.

use crate::domain::{
    AppConfig, DraftProfileInput, MonitorDescriptor, MonitorIdentity, Profile, ProfileId,
    ProfileStatus, TargetMonitorStatus,
};
use crate::persistence::{PersistenceError, ProfileStore};
use crate::platform::{default_monitor_provider, MonitorProvider, PlatformError};
use std::sync::Arc;

/// Kesalahan pada lapisan koordinasi aplikasi.
#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    #[error("Kesalahan penyimpanan: {0}")]
    Persistence(#[from] PersistenceError),

    #[error("Kesalahan platform display: {0}")]
    Platform(#[from] PlatformError),

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

/// Koordinator siklus hidup dan konfigurasi Window Hop.
pub struct LifecycleCoordinator {
    store: Arc<dyn ProfileStore>,
    monitor_provider: Arc<dyn MonitorProvider>,
    config: AppConfig,
    last_error: Option<String>,
    load_failed: bool,
}

impl LifecycleCoordinator {
    /// Membuat instance baru LifecycleCoordinator dengan ProfileStore dan MonitorProvider yang ditentukan.
    pub fn new(store: Arc<dyn ProfileStore>, monitor_provider: Arc<dyn MonitorProvider>) -> Self {
        Self {
            store,
            monitor_provider,
            config: AppConfig::default(),
            last_error: None,
            load_failed: false,
        }
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

    /// Memeriksa apakah suatu profil dapat diaktifkan.
    /// Profil belum dapat diaktifkan jika monitor tujuan belum dipilih, tidak tersedia,
    /// atau ambigu. Tidak ada fallback otomatis ke primary atau monitor lain.
    pub fn can_activate(&self, id: &ProfileId) -> Result<bool, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?;

        if profile.target_monitor().is_none() {
            return Err(ApplicationError::DraftNotReady { id: id.clone() });
        }

        let monitors = self.available_monitors()?;
        match profile.evaluate_target_monitor(&monitors) {
            TargetMonitorStatus::Unset => Err(ApplicationError::DraftNotReady { id: id.clone() }),
            TargetMonitorStatus::Unavailable { recorded_identity } => {
                Err(ApplicationError::TargetMonitorUnavailable {
                    id: id.clone(),
                    recorded_identity,
                })
            }
            TargetMonitorStatus::Ambiguous {
                recorded_identity,
                match_count,
            } => Err(ApplicationError::TargetMonitorAmbiguous {
                id: id.clone(),
                recorded_identity,
                match_count,
            }),
            TargetMonitorStatus::Available(_) => Ok(true),
        }
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
