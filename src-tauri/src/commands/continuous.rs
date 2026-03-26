//! Tauri commands for the 24/7 continuous listening feature.

use crate::managers::continuous::ContinuousListeningManager;
use crate::settings::{get_settings, write_settings};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// Enable or disable 24/7 continuous listening at runtime.
/// Also persists the preference in settings so it survives restarts.
#[tauri::command]
#[specta::specta]
pub fn set_continuous_listening(app: AppHandle, enabled: bool) -> Result<(), String> {
    let cm = app
        .try_state::<Arc<ContinuousListeningManager>>()
        .ok_or_else(|| "ContinuousListeningManager not initialised".to_string())?;

    if enabled {
        cm.start().map_err(|e| e.to_string())?;
    } else {
        cm.stop();
    }

    // Persist the preference
    let mut settings = get_settings(&app);
    settings.continuous_listening = enabled;
    write_settings(&app, settings);

    Ok(())
}

/// Returns `true` if continuous listening is currently active.
#[tauri::command]
#[specta::specta]
pub fn get_continuous_listening_status(app: AppHandle) -> bool {
    app.try_state::<Arc<ContinuousListeningManager>>()
        .map_or(false, |cm| cm.is_active())
}
