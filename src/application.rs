//! Application coordination, lifecycle management, and profile operations.

use crate::domain::{AppConfig, DraftProfileInput, Profile, ProfileId, ProfileStatus};
use crate::persistence::{PersistenceError, ProfileStore};
use std::sync::Arc;

/// Kesalahan pada lapisan koordinasi aplikasi.
#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    #[error("Kesalahan penyimpanan: {0}")]
    Persistence(#[from] PersistenceError),

    #[error("Validasi input gagal: {0}")]
    ValidationError(String),

    #[error("Profil {id} masih berupa draft dan belum memiliki monitor tujuan sehingga belum siap diaktifkan")]
    DraftNotReady { id: ProfileId },

    #[error("Profil dengan ID {id} tidak ditemukan")]
    ProfileNotFound { id: ProfileId },

    #[error("Penyimpanan dicegah karena data konfigurasi sebelumnya rusak atau gagal dimuat: {0}")]
    BlockedByCorruptState(String),
}

/// Koordinator siklus hidup dan konfigurasi Window Hop.
pub struct LifecycleCoordinator {
    store: Arc<dyn ProfileStore>,
    config: AppConfig,
    last_error: Option<String>,
    load_failed: bool,
}

impl LifecycleCoordinator {
    /// Membuat instance baru LifecycleCoordinator dengan ProfileStore yang ditentukan.
    pub fn new(store: Arc<dyn ProfileStore>) -> Self {
        Self {
            store,
            config: AppConfig::default(),
            last_error: None,
            load_failed: false,
        }
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
                existing_id.clone()
            }
            None => {
                let new_id = ProfileId::generate();
                let profile = Profile::new_draft(
                    new_id.clone(),
                    trimmed_name,
                    input.browser_path,
                    input.custom_parameters,
                );
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
                existing_id.clone()
            }
            None => {
                let new_id = ProfileId::generate();
                let profile = Profile::new_draft(
                    new_id.clone(),
                    trimmed_name,
                    input.browser_path.clone(),
                    input.custom_parameters.clone(),
                );
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
        let profile = Profile::new_draft(
            id.clone(),
            trimmed_name,
            input.browser_path,
            input.custom_parameters,
        );
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

    /// Memeriksa apakah suatu profil dapat diaktifkan.
    /// Profil draft tanpa monitor tujuan tidak dapat diaktifkan.
    pub fn can_activate(&self, id: &ProfileId) -> Result<bool, ApplicationError> {
        let profile = self
            .get_profile(id)
            .ok_or_else(|| ApplicationError::ProfileNotFound { id: id.clone() })?;

        if profile.status() == ProfileStatus::DraftNotReady {
            return Err(ApplicationError::DraftNotReady { id: id.clone() });
        }

        Ok(true)
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
