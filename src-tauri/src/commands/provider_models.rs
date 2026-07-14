//! Usage: Provider model discovery and on-demand model probe Tauri commands.

use crate::app_state::{ensure_db_ready, DbInitState};
use crate::domain::provider_models;

#[tauri::command]
#[specta::specta]
pub(crate) async fn provider_models_list(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    provider_id: i64,
    base_url: Option<String>,
) -> Result<provider_models::ProviderModelsResult, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    provider_models::list_provider_models(db, provider_id, base_url)
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn provider_model_probe(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    provider_id: i64,
    model: String,
    base_url: Option<String>,
) -> Result<provider_models::ProviderModelProbeResult, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    provider_models::probe_provider_model(db, provider_id, model, base_url)
        .await
        .map_err(Into::into)
}
