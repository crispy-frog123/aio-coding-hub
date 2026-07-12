//! Usage: Dynamic status.input.im model service status Tauri command.

use crate::domain::service_status;

#[tauri::command]
#[specta::specta]
pub(crate) async fn service_status_fetch() -> Result<service_status::ServiceStatusResult, String> {
    service_status::fetch_status().await.map_err(Into::into)
}
