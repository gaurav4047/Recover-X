//! Shared application state managed by Tauri.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use recoverx_core::error::{RecoverXError, Result};
use recoverx_orchestrator::RecoveryOrchestrator;

/// Application-wide state stored in Tauri's managed state.
pub struct AppState {
    orchestrator: OnceLock<RecoveryOrchestrator>,
    data_dir: Mutex<Option<PathBuf>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            orchestrator: OnceLock::new(),
            data_dir: Mutex::new(None),
        }
    }

    /// Initialise the state with the application data directory.
    /// Called once from `app.setup()`.
    pub fn initialise(&self, data_dir: PathBuf) -> Result<()> {
        *self.data_dir.lock().expect("data_dir lock poisoned") = Some(data_dir.clone());

        let orchestrator = RecoveryOrchestrator::new(data_dir)?;
        self.orchestrator
            .set(orchestrator)
            .map_err(|_| RecoverXError::Other("Orchestrator already initialised".to_string()))?;

        Ok(())
    }

    /// Return a reference to the orchestrator, or an error if not initialised.
    pub fn orchestrator(&self) -> Result<&RecoveryOrchestrator> {
        self.orchestrator
            .get()
            .ok_or_else(|| RecoverXError::Other("Orchestrator not yet initialised".to_string()))
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
