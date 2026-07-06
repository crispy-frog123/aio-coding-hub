//! Usage: Remote sub2api usage snapshot related Tauri commands.

use crate::app_state::{ensure_db_ready, DbInitState};
use crate::{blocking, domain::remote_usage};

#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteUsageCustomSourceDeleteInput {
    pub id: i64,
}

#[derive(Debug, Clone, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoteUsageCustomSourceEnabledInput {
    pub id: i64,
    pub enabled: bool,
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn remote_usage_sources_list(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    cli_key: Option<String>,
) -> Result<Vec<remote_usage::RemoteUsageSourceSummary>, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    blocking::run("remote_usage_sources_list", move || {
        remote_usage::list_sources(&db, cli_key.as_deref())
    })
    .await
    .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn remote_usage_snapshots_refresh(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    input: remote_usage::RemoteUsageRefreshInput,
) -> Result<Vec<remote_usage::RemoteUsageSnapshotRow>, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    remote_usage::refresh_snapshots(db, input)
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn remote_usage_custom_source_upsert(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    input: remote_usage::RemoteUsageCustomSourceUpsertInput,
) -> Result<remote_usage::RemoteUsageSourceSummary, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    blocking::run("remote_usage_custom_source_upsert", move || {
        remote_usage::upsert_custom_source(&db, input)
    })
    .await
    .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn remote_usage_custom_source_delete(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    input: RemoteUsageCustomSourceDeleteInput,
) -> Result<bool, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    blocking::run("remote_usage_custom_source_delete", move || {
        remote_usage::delete_custom_source(&db, input.id)?;
        Ok::<bool, crate::shared::error::AppError>(true)
    })
    .await
    .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn remote_usage_custom_source_set_enabled(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    input: RemoteUsageCustomSourceEnabledInput,
) -> Result<remote_usage::RemoteUsageSourceSummary, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    blocking::run("remote_usage_custom_source_set_enabled", move || {
        remote_usage::set_custom_source_enabled(&db, input.id, input.enabled)
    })
    .await
    .map_err(Into::into)
}
