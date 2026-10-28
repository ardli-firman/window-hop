//! Domain models and pure rules for Window Hop.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// Identitas konsisten untuk suatu profil Window Hop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProfileId(String);

impl ProfileId {
    /// Membuat ProfileId dari nilai string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Menghasilkan ProfileId unik baru menggunakan UUID v4.
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Mengambil representasi string dari ProfileId.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identitas konsisten untuk monitor tujuan.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MonitorIdentity(String);

impl MonitorIdentity {
    /// Membuat MonitorIdentity dari nilai string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Mengambil representasi string dari MonitorIdentity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MonitorIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Status kesiapan atau siklus hidup suatu profil.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileStatus {
    /// Profil draft belum memiliki monitor tujuan sehingga belum siap diaktifkan
    /// dan tidak boleh meluncurkan browser.
    DraftNotReady,
}

impl fmt::Display for ProfileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DraftNotReady => write!(f, "Draft (Belum Siap Diaktifkan)"),
        }
    }
}

/// Data input untuk pembuatan atau pembaruan profil draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftProfileInput {
    pub name: String,
    pub browser_path: PathBuf,
    pub custom_parameters: String,
}

impl DraftProfileInput {
    /// Membuat data input baru untuk profil draft.
    pub fn new(
        name: impl Into<String>,
        browser_path: PathBuf,
        custom_parameters: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            browser_path,
            custom_parameters: custom_parameters.into(),
        }
    }
}

/// Konfigurasi profil satu aplikasi antrian.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    id: ProfileId,
    name: String,
    browser_path: PathBuf,
    custom_parameters: String,
    target_monitor: Option<MonitorIdentity>,
}

impl Profile {
    /// Membuat profil draft baru (tanpa monitor tujuan).
    pub fn new_draft(
        id: ProfileId,
        name: impl Into<String>,
        browser_path: PathBuf,
        custom_parameters: impl Into<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            browser_path,
            custom_parameters: custom_parameters.into(),
            target_monitor: None,
        }
    }

    /// Mengambil ID profil.
    pub fn id(&self) -> &ProfileId {
        &self.id
    }

    /// Mengambil nama profil.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Mengambil jalur file executable browser.
    pub fn browser_path(&self) -> &Path {
        &self.browser_path
    }

    /// Mengambil parameter peluncuran kustom.
    pub fn custom_parameters(&self) -> &str {
        &self.custom_parameters
    }

    /// Mengambil monitor tujuan jika sudah dipilih.
    pub fn target_monitor(&self) -> Option<&MonitorIdentity> {
        self.target_monitor.as_ref()
    }

    /// Status profil: profil draft tanpa monitor tujuan selalu `DraftNotReady`.
    pub fn status(&self) -> ProfileStatus {
        if self.target_monitor.is_none() {
            ProfileStatus::DraftNotReady
        } else {
            // Monitor assignment reserved for future tickets
            ProfileStatus::DraftNotReady
        }
    }

    /// Memperbarui nama profil.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Memperbarui jalur browser.
    pub fn set_browser_path(&mut self, path: PathBuf) {
        self.browser_path = path;
    }

    /// Memperbarui parameter kustom.
    pub fn set_custom_parameters(&mut self, params: impl Into<String>) {
        self.custom_parameters = params.into();
    }
}

/// Konfigurasi aplikasi Window Hop yang dipersistensikan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppConfig {
    schema_version: u32,
    profiles: Vec<Profile>,
}

impl AppConfig {
    /// Membuat konfigurasi baru dengan versi skema dan daftar profil.
    pub fn new(schema_version: u32, profiles: Vec<Profile>) -> Self {
        Self {
            schema_version,
            profiles,
        }
    }

    /// Mengambil versi skema konfigurasi.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Mengambil referensi ke daftar profil.
    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    /// Mengambil referensi mutable ke daftar profil untuk modul internal.
    pub(crate) fn profiles_mut(&mut self) -> &mut Vec<Profile> {
        &mut self.profiles
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            profiles: Vec::new(),
        }
    }
}
