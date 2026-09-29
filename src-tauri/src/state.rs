//! Shared application state.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use imago_core::config::AppConfig;
use imago_core::workers::JobControl;

/// Paths the user has added (inputs) or Imago has written (outputs) in this
/// session. Previews, "show in folder" and exports are limited to them, so the
/// webview can never read arbitrary files.
#[derive(Debug, Default)]
pub struct PathRegistry {
    paths: Mutex<HashSet<PathBuf>>,
}

impl PathRegistry {
    /// Registers a path.
    pub fn register(&self, path: &Path) {
        if let Ok(mut paths) = self.paths.lock() {
            paths.insert(path.to_path_buf());
        }
    }

    /// Whether `path` itself was registered.
    pub fn contains(&self, path: &Path) -> bool {
        self.paths.lock().is_ok_and(|paths| paths.contains(path))
    }

    /// Whether `directory` contains (at any depth) a registered path.
    pub fn contains_under(&self, directory: &Path) -> bool {
        self.paths
            .lock()
            .is_ok_and(|paths| paths.iter().any(|path| path.starts_with(directory)))
    }
}

/// Everything the commands share.
pub struct AppState {
    /// Where the configuration is stored (`None` if the platform has no config dir).
    pub config_path: Option<PathBuf>,
    /// The current configuration.
    pub config: Mutex<AppConfig>,
    /// A problem found while loading the configuration, reported once to the UI.
    pub config_warning: Mutex<Option<String>>,
    /// Folder with log files.
    pub log_dir: Option<PathBuf>,
    /// Paths the webview may refer to.
    pub paths: Arc<PathRegistry>,
    /// Running jobs.
    pub jobs: Arc<Mutex<HashMap<u64, Arc<JobControl>>>>,
    next_job_id: AtomicU64,
}

impl AppState {
    /// Creates the state.
    pub fn new(
        config_path: Option<PathBuf>,
        config: AppConfig,
        config_warning: Option<String>,
        log_dir: Option<PathBuf>,
    ) -> Self {
        AppState {
            config_path,
            config: Mutex::new(config),
            config_warning: Mutex::new(config_warning),
            log_dir,
            paths: Arc::new(PathRegistry::default()),
            jobs: Arc::new(Mutex::new(HashMap::new())),
            next_job_id: AtomicU64::new(1),
        }
    }

    /// A new job identifier.
    pub fn next_job_id(&self) -> u64 {
        self.next_job_id.fetch_add(1, Ordering::Relaxed)
    }

    /// The controller of a running job.
    pub fn job(&self, id: u64) -> Option<Arc<JobControl>> {
        self.jobs
            .lock()
            .ok()
            .and_then(|jobs| jobs.get(&id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_allows_only_known_paths_and_their_folders() {
        let registry = PathRegistry::default();
        let file = Path::new("/photos/optimized/a.webp");
        registry.register(file);
        assert!(registry.contains(file));
        assert!(!registry.contains(Path::new("/etc/passwd")));
        assert!(registry.contains_under(Path::new("/photos/optimized")));
        assert!(registry.contains_under(Path::new("/photos")));
        assert!(!registry.contains_under(Path::new("/other")));
    }
}
