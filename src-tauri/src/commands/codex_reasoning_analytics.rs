//! Usage: Codex reasoning analytics storage and snapshot commands.

use crate::app_state::{ensure_db_ready, DbInitState};
use crate::{blocking, codex_reasoning_analytics};

#[tauri::command]
#[specta::specta]
pub(crate) async fn codex_reasoning_analytics_backfill_from_request_logs(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    input: codex_reasoning_analytics::CodexReasoningAnalyticsBackfillInput,
) -> Result<codex_reasoning_analytics::CodexReasoningAnalyticsBackfillReport, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    blocking::run("codex_reasoning_analytics_backfill", move || {
        codex_reasoning_analytics::backfill_from_request_logs(&db, input)
    })
    .await
    .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn codex_reasoning_analytics_snapshot(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    input: codex_reasoning_analytics::CodexReasoningAnalyticsSnapshotInput,
) -> Result<codex_reasoning_analytics::CodexReasoningAnalyticsSnapshot, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    blocking::run("codex_reasoning_analytics_snapshot", move || {
        codex_reasoning_analytics::snapshot(&db, input)
    })
    .await
    .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn codex_reasoning_analytics_import_json(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    input: codex_reasoning_analytics::CodexReasoningAnalyticsImportJsonInput,
) -> Result<codex_reasoning_analytics::CodexReasoningAnalyticsImportReport, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    blocking::run("codex_reasoning_analytics_import_json", move || {
        codex_reasoning_analytics::import_json(&db, input)
    })
    .await
    .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn codex_reasoning_analytics_export(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    input: codex_reasoning_analytics::CodexReasoningAnalyticsExportInput,
) -> Result<codex_reasoning_analytics::CodexReasoningAnalyticsExport, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    blocking::run("codex_reasoning_analytics_export", move || {
        codex_reasoning_analytics::export(&db, input)
    })
    .await
    .map_err(Into::into)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn codex_reasoning_analytics_analyze(
    app: tauri::AppHandle,
    db_state: tauri::State<'_, DbInitState>,
    input: codex_reasoning_analytics::CodexReasoningAnalyticsAnalyzeInput,
) -> Result<codex_reasoning_analytics::CodexReasoningAnalysisResult, String> {
    let db = ensure_db_ready(app, db_state.inner()).await?;
    blocking::run("codex_reasoning_analytics_analyze", move || {
        codex_reasoning_analytics::analyze(&db, input)
    })
    .await
    .map_err(Into::into)
}

