//! Persistence for profiles and application settings.

use crate::domain::AppConfig;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Kesalahan yang dapat terjadi pada proses penyimpanan atau pemuatan profil.
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("Kesalahan I/O: {0}")]
    Io(#[from] std::io::Error),

    #[error("Data konfigurasi rusak: {0}")]
    CorruptData(String),

    #[error("Versi skema tidak didukung: {version}")]
    UnsupportedSchemaVersion { version: u32 },
}

/// Mengembalikan jalur file konfigurasi default berdasarkan platform OS.
pub fn default_config_path() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata)
                .join("WindowHop")
                .join("profiles.json");
        }
    }

    #[cfg(not(windows))]
    {
        if let Ok(config_home) = std::env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(config_home)
                .join("window-hop")
                .join("profiles.json");
        } else if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".config")
                .join("window-hop")
                .join("profiles.json");
        }
    }

    PathBuf::from("window-hop-profiles.json")
}

/// Seam antarmuka penyimpanan profil.
pub trait ProfileStore: Send + Sync {
    /// Memuat konfigurasi profil yang tersimpan.
    fn load(&self) -> Result<AppConfig, PersistenceError>;

    /// Menyimpan konfigurasi profil secara atomik.
    fn save(&self, config: &AppConfig) -> Result<(), PersistenceError>;
}

/// Implementasi penyimpanan berbasis file JSON dengan penulisan atomik.
pub struct FileProfileStore {
    path: PathBuf,
}

impl FileProfileStore {
    /// Membuat FileProfileStore dengan jalur file target.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Mengambil referensi ke jalur file penyimpanan.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl ProfileStore for FileProfileStore {
    fn load(&self) -> Result<AppConfig, PersistenceError> {
        if !self.path.exists() {
            return Ok(AppConfig::default());
        }

        let content = match fs::read_to_string(&self.path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AppConfig::default());
            }
            Err(e) => return Err(PersistenceError::Io(e)),
        };

        let config: AppConfig = serde_json::from_str(&content)
            .map_err(|e| PersistenceError::CorruptData(e.to_string()))?;

        if config.schema_version() != 1 {
            return Err(PersistenceError::UnsupportedSchemaVersion {
                version: config.schema_version(),
            });
        }

        Ok(config)
    }

    fn save(&self, config: &AppConfig) -> Result<(), PersistenceError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let serialized = serde_json::to_string_pretty(config)
            .map_err(|e| PersistenceError::CorruptData(e.to_string()))?;

        // Gunakan file sementara di direktori yang sama agar rename bersifat atomik pada filesystem yang sama.
        let unique_suffix = uuid::Uuid::new_v4().to_string();
        let tmp_file_name = format!(
            ".{}.tmp.{}",
            self.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("profiles"),
            unique_suffix
        );
        let tmp_path = match self.path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(tmp_file_name),
            _ => PathBuf::from(tmp_file_name),
        };

        // Tulis dan flush ke disk sebelum rename
        let write_res = (|| -> Result<(), std::io::Error> {
            let mut file = File::create(&tmp_path)?;
            file.write_all(serialized.as_bytes())?;
            file.sync_data()?;
            Ok(())
        })();

        if let Err(err) = write_res {
            let _ = fs::remove_file(&tmp_path);
            return Err(PersistenceError::Io(err));
        }

        // Rename file sementara menggantikan file tujuan secara atomik
        if let Err(err) = fs::rename(&tmp_path, &self.path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(PersistenceError::Io(err));
        }

        Ok(())
    }
}

/// Implementasi in-memory untuk testing cepat tanpa akses filesystem nyata.
pub struct MemoryProfileStore {
    config: std::sync::Mutex<AppConfig>,
    simulate_io_error: std::sync::atomic::AtomicBool,
}

impl MemoryProfileStore {
    pub fn new() -> Self {
        Self {
            config: std::sync::Mutex::new(AppConfig::default()),
            simulate_io_error: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn with_config(config: AppConfig) -> Self {
        Self {
            config: std::sync::Mutex::new(config),
            simulate_io_error: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn set_simulate_io_error(&self, simulate: bool) {
        self.simulate_io_error
            .store(simulate, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Default for MemoryProfileStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ProfileStore for MemoryProfileStore {
    fn load(&self) -> Result<AppConfig, PersistenceError> {
        if self
            .simulate_io_error
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(PersistenceError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Simulated I/O read failure",
            )));
        }
        let config = self.config.lock().unwrap();
        Ok(config.clone())
    }

    fn save(&self, config: &AppConfig) -> Result<(), PersistenceError> {
        if self
            .simulate_io_error
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(PersistenceError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Simulated I/O write failure",
            )));
        }
        let mut target = self.config.lock().unwrap();
        *target = config.clone();
        Ok(())
    }
}
