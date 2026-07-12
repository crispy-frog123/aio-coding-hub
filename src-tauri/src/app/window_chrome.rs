//! Usage: Apply the bundled app icon to the native Windows title bar.

pub(crate) fn apply_main_window_chrome(app: &tauri::AppHandle, window: &tauri::WebviewWindow) {
    #[cfg(windows)]
    match app.default_window_icon().cloned() {
        Some(icon) => {
            if let Err(err) = window.set_icon(icon) {
                tracing::warn!(error = %err, "failed to apply Windows titlebar icon");
            }
        }
        None => tracing::warn!("bundled app icon is unavailable for Windows titlebar"),
    }

    #[cfg(not(windows))]
    let _ = (app, window);
}
