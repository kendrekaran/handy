use log::debug;
use tauri::{AppHandle, Manager};

#[cfg(not(target_os = "macos"))]
use tauri::WebviewWindowBuilder;

#[cfg(target_os = "macos")]
use tauri::WebviewUrl;

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelBuilder, PanelLevel};

const HELPER_WIDTH: f64 = 280.0;
const HELPER_HEIGHT: f64 = 420.0;

// Offset from right edge of screen
const HELPER_RIGHT_MARGIN: f64 = 20.0;
const HELPER_TOP_MARGIN: f64 = 60.0;

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(CommandsHelperPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

fn calculate_helper_position(app_handle: &AppHandle) -> Option<(f64, f64)> {
    let monitor = app_handle.primary_monitor().ok().flatten()?;
    let scale = monitor.scale_factor();
    let monitor_x = monitor.position().x as f64 / scale;
    let monitor_y = monitor.position().y as f64 / scale;
    let monitor_width = monitor.size().width as f64 / scale;

    let x = monitor_x + monitor_width - HELPER_WIDTH - HELPER_RIGHT_MARGIN;
    let y = monitor_y + HELPER_TOP_MARGIN;

    Some((x, y))
}

/// Creates the commands helper window (macOS)
#[cfg(target_os = "macos")]
pub fn create_commands_helper(app_handle: &AppHandle) {
    if let Some((x, y)) = calculate_helper_position(app_handle) {
        match PanelBuilder::<_, CommandsHelperPanel>::new(app_handle, "commands_helper")
            .url(WebviewUrl::App("src/commands-helper/index.html".into()))
            .title("Voice Commands")
            .position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
            .level(PanelLevel::Floating)
            .size(tauri::Size::Logical(tauri::LogicalSize {
                width: HELPER_WIDTH,
                height: HELPER_HEIGHT,
            }))
            .has_shadow(true)
            .transparent(true)
            .no_activate(true)
            .corner_radius(12.0)
            .with_window(|w| w.decorations(false).transparent(true))
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary(),
            )
            .build()
        {
            Ok(panel) => {
                let _ = panel.show();
                debug!("Commands helper panel created and shown (macOS)");
            }
            Err(e) => {
                log::error!("Failed to create commands helper panel: {}", e);
            }
        }
    }
}

/// Creates the commands helper window (Windows/Linux)
#[cfg(not(target_os = "macos"))]
pub fn create_commands_helper(app_handle: &AppHandle) {
    let position = calculate_helper_position(app_handle);

    let mut builder = WebviewWindowBuilder::new(
        app_handle,
        "commands_helper",
        tauri::WebviewUrl::App("src/commands-helper/index.html".into()),
    )
    .title("Voice Commands")
    .resizable(false)
    .inner_size(HELPER_WIDTH, HELPER_HEIGHT)
    .shadow(true)
    .maximizable(false)
    .minimizable(false)
    .closable(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focused(false)
    .visible(true);

    if let Some((x, y)) = position {
        builder = builder.position(x, y);
    }

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    match builder.build() {
        Ok(_window) => {
            debug!("Commands helper window created and shown");
        }
        Err(e) => {
            debug!("Failed to create commands helper window: {}", e);
        }
    }
}

/// Toggle the commands helper window visibility.
/// If it doesn't exist, create it. If it exists and is visible, hide/close it.
pub fn toggle_commands_helper(app_handle: &AppHandle) {
    if let Some(window) = app_handle.get_webview_window("commands_helper") {
        // Window exists — check if visible
        if window.is_visible().unwrap_or(false) {
            let _ = window.close();
        } else {
            let _ = window.show();
        }
    } else {
        // Window doesn't exist — create it
        create_commands_helper(app_handle);
    }
}
