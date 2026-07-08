//! Usage: Remote sub2api usage snapshots.

use crate::db;
use crate::providers::is_cx2cc_bridge;
use crate::shared::error::{db_err, AppError};
use crate::shared::sqlite::enabled_to_int;
use crate::shared::time::now_unix_seconds;
use reqwest::StatusCode;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::time::Duration;

const MAX_CUSTOM_SOURCE_NAME_CHARS: usize = 120;
const MAX_BASE_URL_CHARS: usize = 2048;
const MAX_API_KEY_CHARS: usize = 4096;
const MAX_SOURCE_IDS: usize = 300;
const SNAPSHOT_TIMEOUT_SECS: u64 = 20;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteUsageSourceType {
    Provider,
    Custom,
}

impl RemoteUsageSourceType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Provider => "provider",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteUsageSnapshotStatus {
    Fresh,
    Stale,
    Unauthorized,
    NotConfigured,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
pub struct RemoteUsageBucket {
    pub cost: Option<f64>,
    pub tokens: Option<f64>,
    pub requests: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
pub struct RemoteUsageUsage {
    pub today: Option<RemoteUsageBucket>,
    pub week: Option<RemoteUsageBucket>,
    pub month: Option<RemoteUsageBucket>,
    pub total: Option<RemoteUsageBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
pub struct RemoteUsageModelStat {
    pub model: String,
    pub cost: Option<f64>,
    pub tokens: Option<f64>,
    pub requests: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
pub struct RemoteUsageSnapshot {
    pub plan_name: Option<String>,
    pub remaining: Option<f64>,
    pub unit: Option<String>,
    pub subscription: Option<String>,
    pub usage: RemoteUsageUsage,
    pub model_stats: Vec<RemoteUsageModelStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct RemoteUsageSourceSummary {
    pub source_id: String,
    pub source_type: RemoteUsageSourceType,
    pub cli_key: String,
    pub name: String,
    pub base_url: String,
    pub endpoint_url: String,
    pub enabled: bool,
    pub provider_id: Option<i64>,
    pub custom_source_id: Option<i64>,
    pub api_key_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct RemoteUsageSnapshotRow {
    pub source: RemoteUsageSourceSummary,
    pub status: RemoteUsageSnapshotStatus,
    pub last_error: Option<String>,
    pub last_successful_refresh_at: Option<i64>,
    pub snapshot: Option<RemoteUsageSnapshot>,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteUsageRefreshInput {
    pub cli_key: Option<String>,
    pub source_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteUsageCustomSourceUpsertInput {
    pub id: Option<i64>,
    pub cli_key: String,
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
struct RemoteUsageCandidate {
    summary: RemoteUsageSourceSummary,
    api_key: Option<String>,
}

#[derive(Debug, Clone)]
struct CachedSnapshot {
    snapshot: RemoteUsageSnapshot,
    last_successful_refresh_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FetchFailureKind {
    Unauthorized,
    Failed,
}

#[derive(Debug, Clone)]
struct FetchFailure {
    kind: FetchFailureKind,
    message: String,
}

pub fn normalize_usage_endpoint(base_url: &str) -> crate::shared::error::AppResult<String> {
    let trimmed = base_url.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err("SEC_INVALID_INPUT: base_url is required".into());
    }
    if trimmed.chars().count() > MAX_BASE_URL_CHARS {
        return Err("SEC_INVALID_INPUT: base_url is too long".into());
    }
    let endpoint = if trimmed.ends_with("/v1/usage") {
        trimmed
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/usage")
    } else {
        format!("{trimmed}/v1/usage")
    };
    reqwest::Url::parse(&endpoint)
        .map_err(|e| AppError::new("SEC_INVALID_INPUT", format!("invalid usage endpoint: {e}")))?;
    Ok(endpoint)
}

fn normalize_cli_filter(cli_key: Option<&str>) -> crate::shared::error::AppResult<Option<String>> {
    let Some(raw) = cli_key else {
        return Ok(None);
    };
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Ok(None);
    }
    crate::shared::cli_key::validate_cli_key(normalized)?;
    Ok(Some(normalized.to_string()))
}

fn normalize_name(raw: &str) -> crate::shared::error::AppResult<String> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err("SEC_INVALID_INPUT: name is required".into());
    }
    if normalized.chars().count() > MAX_CUSTOM_SOURCE_NAME_CHARS {
        return Err("SEC_INVALID_INPUT: name is too long".into());
    }
    Ok(normalized.to_string())
}

fn normalize_api_key(
    raw: Option<String>,
    required: bool,
) -> crate::shared::error::AppResult<Option<String>> {
    let value = raw.unwrap_or_default().trim().to_string();
    if value.is_empty() {
        if required {
            return Err("SEC_INVALID_INPUT: apiKey is required".into());
        }
        return Ok(None);
    }
    if value.chars().count() > MAX_API_KEY_CHARS {
        return Err("SEC_INVALID_INPUT: apiKey is too long".into());
    }
    Ok(Some(value))
}

fn fingerprint(endpoint_url: &str, api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(endpoint_url.as_bytes());
    hasher.update(b"\n");
    hasher.update(api_key.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn source_id(source_type: RemoteUsageSourceType, id: i64) -> String {
    format!("{}:{id}", source_type.as_str())
}

fn parse_source_id(raw: &str) -> Option<(RemoteUsageSourceType, i64)> {
    let (kind, id_raw) = raw.trim().split_once(':')?;
    let id = id_raw.parse::<i64>().ok()?;
    if id <= 0 {
        return None;
    }
    match kind {
        "provider" => Some((RemoteUsageSourceType::Provider, id)),
        "custom" => Some((RemoteUsageSourceType::Custom, id)),
        _ => None,
    }
}

fn source_id_filter(
    ids: Option<Vec<String>>,
) -> crate::shared::error::AppResult<Option<HashSet<String>>> {
    let Some(ids) = ids else {
        return Ok(None);
    };
    if ids.len() > MAX_SOURCE_IDS {
        return Err("SEC_INVALID_INPUT: too many sourceIds".into());
    }
    let mut out = HashSet::new();
    for raw in ids {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if parse_source_id(trimmed).is_none() {
            return Err(format!("SEC_INVALID_INPUT: invalid sourceId={trimmed}").into());
        }
        out.insert(trimmed.to_string());
    }
    Ok(Some(out))
}

fn base_urls_from_row(base_url: &str, base_urls_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(base_urls_json)
        .ok()
        .unwrap_or_else(|| vec![base_url.to_string()])
        .into_iter()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

fn provider_candidates(
    conn: &Connection,
    cli_filter: Option<&str>,
) -> crate::shared::error::AppResult<Vec<RemoteUsageCandidate>> {
    let mut stmt = conn
        .prepare(
            r#"
SELECT
  id,
  cli_key,
  name,
  base_url,
  base_urls_json,
  enabled,
  api_key_plaintext,
  auth_mode,
  source_provider_id,
  bridge_type
FROM providers
WHERE (?1 IS NULL OR cli_key = ?1)
ORDER BY cli_key ASC, sort_order ASC, id DESC
"#,
        )
        .map_err(|e| db_err!("failed to prepare provider source query: {e}"))?;
    let rows = stmt
        .query_map(params![cli_filter], |row| {
            Ok((
                row.get::<_, i64>("id")?,
                row.get::<_, String>("cli_key")?,
                row.get::<_, String>("name")?,
                row.get::<_, String>("base_url")?,
                row.get::<_, String>("base_urls_json")?,
                row.get::<_, i64>("enabled")? != 0,
                row.get::<_, String>("api_key_plaintext")?,
                row.get::<_, String>("auth_mode")?,
                row.get::<_, Option<i64>>("source_provider_id")?,
                row.get::<_, Option<String>>("bridge_type")?,
            ))
        })
        .map_err(|e| db_err!("failed to query provider sources: {e}"))?;

    let mut out = Vec::new();
    for row in rows {
        let (
            id,
            cli_key,
            name,
            base_url,
            base_urls_json,
            enabled,
            api_key_plaintext,
            auth_mode,
            source_provider_id,
            bridge_type,
        ) = row.map_err(|e| db_err!("failed to read provider source row: {e}"))?;

        let key = api_key_plaintext.trim().to_string();
        let base_url = base_urls_from_row(&base_url, &base_urls_json)
            .into_iter()
            .next()
            .unwrap_or_default();
        let configured = auth_mode == "api_key"
            && !is_cx2cc_bridge(source_provider_id, bridge_type.as_deref())
            && !base_url.trim().is_empty()
            && !key.is_empty();
        if !configured {
            continue;
        }

        let endpoint_url = normalize_usage_endpoint(&base_url)?;
        out.push(RemoteUsageCandidate {
            summary: RemoteUsageSourceSummary {
                source_id: source_id(RemoteUsageSourceType::Provider, id),
                source_type: RemoteUsageSourceType::Provider,
                cli_key,
                name,
                base_url,
                endpoint_url,
                enabled,
                provider_id: Some(id),
                custom_source_id: None,
                api_key_configured: true,
            },
            api_key: Some(key),
        });
    }
    Ok(out)
}

fn custom_candidates(
    conn: &Connection,
    cli_filter: Option<&str>,
) -> crate::shared::error::AppResult<Vec<RemoteUsageCandidate>> {
    let mut stmt = conn
        .prepare(
            r#"
SELECT id, cli_key, name, base_url, api_key_plaintext, enabled
FROM remote_usage_custom_sources
WHERE (?1 IS NULL OR cli_key = ?1)
ORDER BY cli_key ASC, updated_at DESC, id DESC
"#,
        )
        .map_err(|e| db_err!("failed to prepare custom source query: {e}"))?;
    let rows = stmt
        .query_map(params![cli_filter], |row| {
            Ok((
                row.get::<_, i64>("id")?,
                row.get::<_, String>("cli_key")?,
                row.get::<_, String>("name")?,
                row.get::<_, String>("base_url")?,
                row.get::<_, String>("api_key_plaintext")?,
                row.get::<_, i64>("enabled")? != 0,
            ))
        })
        .map_err(|e| db_err!("failed to query custom sources: {e}"))?;

    let mut out = Vec::new();
    for row in rows {
        let (id, cli_key, name, base_url, api_key_plaintext, enabled) =
            row.map_err(|e| db_err!("failed to read custom source row: {e}"))?;
        let api_key = api_key_plaintext.trim().to_string();
        let endpoint_url = normalize_usage_endpoint(&base_url)?;
        out.push(RemoteUsageCandidate {
            summary: RemoteUsageSourceSummary {
                source_id: source_id(RemoteUsageSourceType::Custom, id),
                source_type: RemoteUsageSourceType::Custom,
                cli_key,
                name,
                base_url,
                endpoint_url,
                enabled,
                provider_id: None,
                custom_source_id: Some(id),
                api_key_configured: !api_key.is_empty(),
            },
            api_key: if api_key.is_empty() {
                None
            } else {
                Some(api_key)
            },
        });
    }
    Ok(out)
}

fn all_candidates(
    conn: &Connection,
    cli_key: Option<&str>,
) -> crate::shared::error::AppResult<Vec<RemoteUsageCandidate>> {
    let mut items = provider_candidates(conn, cli_key)?;
    items.extend(custom_candidates(conn, cli_key)?);
    Ok(items)
}

pub fn list_sources(
    db: &db::Db,
    cli_key: Option<&str>,
) -> crate::shared::error::AppResult<Vec<RemoteUsageSourceSummary>> {
    let cli_filter = normalize_cli_filter(cli_key)?;
    let conn = db.open_connection()?;
    all_candidates(&conn, cli_filter.as_deref())
        .map(|items| items.into_iter().map(|item| item.summary).collect())
}

pub fn upsert_custom_source(
    db: &db::Db,
    input: RemoteUsageCustomSourceUpsertInput,
) -> crate::shared::error::AppResult<RemoteUsageSourceSummary> {
    let cli_key = input.cli_key.trim().to_string();
    crate::shared::cli_key::validate_cli_key(&cli_key)?;
    let name = normalize_name(&input.name)?;
    let base_url = input.base_url.trim().trim_end_matches('/').to_string();
    let endpoint_url = normalize_usage_endpoint(&base_url)?;
    let conn = db.open_connection()?;
    let now = now_unix_seconds();

    let id = match input.id {
        Some(id) => {
            if id <= 0 {
                return Err("SEC_INVALID_INPUT: invalid custom source id".into());
            }
            let existing_key: Option<String> = conn
                .query_row(
                    "SELECT api_key_plaintext FROM remote_usage_custom_sources WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| db_err!("failed to query custom source: {e}"))?;
            let Some(existing_key) = existing_key else {
                return Err("DB_NOT_FOUND: custom source not found".into());
            };
            let next_key = normalize_api_key(input.api_key, false)?.unwrap_or(existing_key);
            conn.execute(
                r#"
UPDATE remote_usage_custom_sources
SET cli_key = ?1, name = ?2, base_url = ?3, api_key_plaintext = ?4, enabled = ?5, updated_at = ?6
WHERE id = ?7
"#,
                params![
                    cli_key,
                    name,
                    base_url,
                    next_key,
                    enabled_to_int(input.enabled),
                    now,
                    id
                ],
            )
            .map_err(|e| db_err!("failed to update custom source: {e}"))?;
            id
        }
        None => {
            let api_key = normalize_api_key(input.api_key, true)?.unwrap_or_default();
            conn.execute(
                r#"
INSERT INTO remote_usage_custom_sources(
  cli_key, name, base_url, api_key_plaintext, enabled, created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
"#,
                params![
                    cli_key,
                    name,
                    base_url,
                    api_key,
                    enabled_to_int(input.enabled),
                    now
                ],
            )
            .map_err(|e| db_err!("failed to insert custom source: {e}"))?;
            conn.last_insert_rowid()
        }
    };

    load_custom_summary(&conn, id).map(|mut summary| {
        summary.endpoint_url = endpoint_url;
        summary
    })
}

fn load_custom_summary(
    conn: &Connection,
    id: i64,
) -> crate::shared::error::AppResult<RemoteUsageSourceSummary> {
    conn.query_row(
        r#"
SELECT id, cli_key, name, base_url, api_key_plaintext, enabled
FROM remote_usage_custom_sources
WHERE id = ?1
"#,
        params![id],
        |row| {
            let id = row.get::<_, i64>("id")?;
            let base_url = row.get::<_, String>("base_url")?;
            let api_key = row.get::<_, String>("api_key_plaintext")?;
            Ok(RemoteUsageSourceSummary {
                source_id: source_id(RemoteUsageSourceType::Custom, id),
                source_type: RemoteUsageSourceType::Custom,
                cli_key: row.get("cli_key")?,
                name: row.get("name")?,
                endpoint_url: normalize_usage_endpoint(&base_url).unwrap_or_default(),
                base_url,
                enabled: row.get::<_, i64>("enabled")? != 0,
                provider_id: None,
                custom_source_id: Some(id),
                api_key_configured: !api_key.trim().is_empty(),
            })
        },
    )
    .optional()
    .map_err(|e| db_err!("failed to load custom source: {e}"))?
    .ok_or_else(|| "DB_NOT_FOUND: custom source not found".into())
}

pub fn delete_custom_source(db: &db::Db, id: i64) -> crate::shared::error::AppResult<()> {
    if id <= 0 {
        return Err("SEC_INVALID_INPUT: invalid custom source id".into());
    }
    let conn = db.open_connection()?;
    let changed = conn
        .execute(
            "DELETE FROM remote_usage_custom_sources WHERE id = ?1",
            params![id],
        )
        .map_err(|e| db_err!("failed to delete custom source: {e}"))?;
    if changed == 0 {
        return Err("DB_NOT_FOUND: custom source not found".into());
    }
    Ok(())
}

pub fn set_custom_source_enabled(
    db: &db::Db,
    id: i64,
    enabled: bool,
) -> crate::shared::error::AppResult<RemoteUsageSourceSummary> {
    if id <= 0 {
        return Err("SEC_INVALID_INPUT: invalid custom source id".into());
    }
    let conn = db.open_connection()?;
    let changed = conn
        .execute(
            "UPDATE remote_usage_custom_sources SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            params![enabled_to_int(enabled), now_unix_seconds(), id],
        )
        .map_err(|e| db_err!("failed to update custom source enabled: {e}"))?;
    if changed == 0 {
        return Err("DB_NOT_FOUND: custom source not found".into());
    }
    load_custom_summary(&conn, id)
}

fn as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn first_value<'a>(obj: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| obj.get(*key))
}

fn optional_text(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    first_value(obj, keys).and_then(|value| match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

fn optional_number(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    first_value(obj, keys).and_then(as_f64)
}

fn parse_bucket(value: &Value) -> Option<RemoteUsageBucket> {
    match value {
        Value::Number(_) | Value::String(_) => Some(RemoteUsageBucket {
            cost: as_f64(value),
            tokens: None,
            requests: None,
        }),
        Value::Object(obj) => Some(RemoteUsageBucket {
            cost: optional_number(obj, &["cost", "amount", "usage", "total", "value"]),
            tokens: optional_number(obj, &["tokens", "tokenCount", "token_count"]),
            requests: optional_number(obj, &["requests", "requestCount", "request_count", "count"]),
        }),
        _ => None,
    }
}

fn parse_usage(root: &Value) -> RemoteUsageUsage {
    let usage = root.get("usage").unwrap_or(root);
    let obj = usage.as_object();
    let bucket = |keys: &[&str]| -> Option<RemoteUsageBucket> {
        obj.and_then(|o| first_value(o, keys))
            .and_then(parse_bucket)
    };
    RemoteUsageUsage {
        today: bucket(&["today", "daily", "day"]),
        week: bucket(&["week", "weekly"]),
        month: bucket(&["month", "monthly"]),
        total: bucket(&["total", "all", "allTime", "all_time"]),
    }
}

fn parse_model_stats(root: &Value) -> Vec<RemoteUsageModelStat> {
    let candidates = [
        root.get("modelStats"),
        root.get("model_stats"),
        root.get("models"),
        root.get("usage").and_then(|v| v.get("models")),
    ];
    let Some(value) = candidates.into_iter().flatten().next() else {
        return Vec::new();
    };
    match value {
        Value::Array(items) => items
            .iter()
            .filter_map(|item| {
                let obj = item.as_object()?;
                let model = optional_text(obj, &["model", "name", "modelName", "model_name"])?;
                Some(RemoteUsageModelStat {
                    model,
                    cost: optional_number(obj, &["cost", "amount", "usage", "total"]),
                    tokens: optional_number(obj, &["tokens", "tokenCount", "token_count"]),
                    requests: optional_number(
                        obj,
                        &["requests", "requestCount", "request_count", "count"],
                    ),
                })
            })
            .collect(),
        Value::Object(map) => map
            .iter()
            .filter_map(|(model, value)| {
                parse_bucket(value).map(|bucket| RemoteUsageModelStat {
                    model: model.clone(),
                    cost: bucket.cost,
                    tokens: bucket.tokens,
                    requests: bucket.requests,
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_usage_snapshot(root: &Value) -> Result<RemoteUsageSnapshot, FetchFailure> {
    let obj = root.as_object().ok_or_else(|| FetchFailure {
        kind: FetchFailureKind::Failed,
        message: "usage response must be a JSON object".to_string(),
    })?;

    if first_value(obj, &["isValid", "is_valid"]).and_then(Value::as_bool) == Some(false) {
        return Err(FetchFailure {
            kind: FetchFailureKind::Unauthorized,
            message: "remote usage reported isValid=false".to_string(),
        });
    }

    let data = first_value(obj, &["data", "result"])
        .filter(|value| value.is_object())
        .unwrap_or(root);
    let data_obj = data.as_object().unwrap_or(obj);

    Ok(RemoteUsageSnapshot {
        plan_name: optional_text(data_obj, &["planName", "plan_name", "plan"]),
        remaining: optional_number(
            data_obj,
            &["remaining", "balance", "quotaRemaining", "quota_remaining"],
        ),
        unit: optional_text(data_obj, &["unit", "currency"]),
        subscription: optional_text(
            data_obj,
            &["subscription", "subscriptionName", "subscription_name"],
        ),
        usage: parse_usage(data),
        model_stats: parse_model_stats(data),
    })
}

async fn fetch_snapshot(
    client: &reqwest::Client,
    endpoint_url: &str,
    api_key: &str,
) -> Result<RemoteUsageSnapshot, FetchFailure> {
    let response = client
        .get(endpoint_url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| FetchFailure {
            kind: FetchFailureKind::Failed,
            message: format!("network error: {e}"),
        })?;
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(FetchFailure {
            kind: FetchFailureKind::Unauthorized,
            message: format!("remote returned HTTP {}", status.as_u16()),
        });
    }
    if !status.is_success() {
        return Err(FetchFailure {
            kind: FetchFailureKind::Failed,
            message: format!("remote returned HTTP {}", status.as_u16()),
        });
    }
    let value = response.json::<Value>().await.map_err(|e| FetchFailure {
        kind: FetchFailureKind::Failed,
        message: format!("decode error: {e}"),
    })?;
    parse_usage_snapshot(&value)
}

fn write_cache(
    conn: &Connection,
    source: &RemoteUsageSourceSummary,
    fp: &str,
    snapshot: &RemoteUsageSnapshot,
    refreshed_at: i64,
) -> crate::shared::error::AppResult<()> {
    let snapshot_json =
        serde_json::to_string(snapshot).map_err(|e| format!("SYSTEM_ERROR: {e}"))?;
    conn.execute(
        r#"
INSERT INTO remote_usage_snapshot_cache(
  source_id, source_type, provider_id, custom_source_id, endpoint_url, api_key_fingerprint,
  snapshot_json, last_successful_refresh_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
ON CONFLICT(source_id) DO UPDATE SET
  source_type = excluded.source_type,
  provider_id = excluded.provider_id,
  custom_source_id = excluded.custom_source_id,
  endpoint_url = excluded.endpoint_url,
  api_key_fingerprint = excluded.api_key_fingerprint,
  snapshot_json = excluded.snapshot_json,
  last_successful_refresh_at = excluded.last_successful_refresh_at,
  updated_at = excluded.updated_at
"#,
        params![
            source.source_id,
            source.source_type.as_str(),
            source.provider_id,
            source.custom_source_id,
            source.endpoint_url,
            fp,
            snapshot_json,
            refreshed_at
        ],
    )
    .map_err(|e| db_err!("failed to write remote usage cache: {e}"))?;
    Ok(())
}

fn read_cache(
    conn: &Connection,
    source_id: &str,
    fp: &str,
) -> crate::shared::error::AppResult<Option<CachedSnapshot>> {
    let row: Option<(String, i64)> = conn
        .query_row(
            r#"
SELECT snapshot_json, last_successful_refresh_at
FROM remote_usage_snapshot_cache
WHERE source_id = ?1 AND api_key_fingerprint = ?2
"#,
            params![source_id, fp],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| db_err!("failed to read remote usage cache: {e}"))?;
    let Some((snapshot_json, last_successful_refresh_at)) = row else {
        return Ok(None);
    };
    let snapshot = serde_json::from_str::<RemoteUsageSnapshot>(&snapshot_json)
        .map_err(|e| db_err!("failed to decode remote usage cache: {e}"))?;
    Ok(Some(CachedSnapshot {
        snapshot,
        last_successful_refresh_at,
    }))
}

async fn row_for_candidate(
    client: &reqwest::Client,
    db: &db::Db,
    candidate: RemoteUsageCandidate,
) -> RemoteUsageSnapshotRow {
    let Some(api_key) = candidate
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    else {
        return RemoteUsageSnapshotRow {
            source: candidate.summary,
            status: RemoteUsageSnapshotStatus::NotConfigured,
            last_error: Some("API Key 未配置".to_string()),
            last_successful_refresh_at: None,
            snapshot: None,
        };
    };
    if !candidate.summary.enabled {
        return RemoteUsageSnapshotRow {
            source: candidate.summary,
            status: RemoteUsageSnapshotStatus::NotConfigured,
            last_error: Some("来源已禁用".to_string()),
            last_successful_refresh_at: None,
            snapshot: None,
        };
    }

    match fetch_snapshot(client, &candidate.summary.endpoint_url, api_key).await {
        Ok(snapshot) => {
            let refreshed_at = now_unix_seconds();
            let fp = fingerprint(&candidate.summary.endpoint_url, api_key);
            if let Ok(conn) = db.open_connection() {
                if let Err(err) =
                    write_cache(&conn, &candidate.summary, &fp, &snapshot, refreshed_at)
                {
                    tracing::warn!("failed to cache remote usage snapshot: {err}");
                }
            }
            RemoteUsageSnapshotRow {
                source: candidate.summary,
                status: RemoteUsageSnapshotStatus::Fresh,
                last_error: None,
                last_successful_refresh_at: Some(refreshed_at),
                snapshot: Some(snapshot),
            }
        }
        Err(error) => {
            let status = if error.kind == FetchFailureKind::Unauthorized {
                RemoteUsageSnapshotStatus::Unauthorized
            } else {
                RemoteUsageSnapshotStatus::Failed
            };
            let fp = fingerprint(&candidate.summary.endpoint_url, api_key);
            let cached = db.open_connection().ok().and_then(|conn| {
                read_cache(&conn, &candidate.summary.source_id, &fp)
                    .ok()
                    .flatten()
            });
            if error.kind == FetchFailureKind::Failed {
                if let Some(cached) = cached {
                    return RemoteUsageSnapshotRow {
                        source: candidate.summary,
                        status: RemoteUsageSnapshotStatus::Stale,
                        last_error: Some(error.message),
                        last_successful_refresh_at: Some(cached.last_successful_refresh_at),
                        snapshot: Some(cached.snapshot),
                    };
                }
            }
            RemoteUsageSnapshotRow {
                source: candidate.summary,
                status,
                last_error: Some(error.message),
                last_successful_refresh_at: cached.map(|c| c.last_successful_refresh_at),
                snapshot: None,
            }
        }
    }
}

pub async fn refresh_snapshots(
    db: db::Db,
    input: RemoteUsageRefreshInput,
) -> crate::shared::error::AppResult<Vec<RemoteUsageSnapshotRow>> {
    let cli_filter = normalize_cli_filter(input.cli_key.as_deref())?;
    let id_filter = source_id_filter(input.source_ids)?;
    let candidates = {
        let conn = db.open_connection()?;
        all_candidates(&conn, cli_filter.as_deref())?
    };
    let candidates = candidates
        .into_iter()
        .filter(|candidate| {
            id_filter
                .as_ref()
                .is_none_or(|ids| ids.contains(&candidate.summary.source_id))
        })
        .collect::<Vec<_>>();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(SNAPSHOT_TIMEOUT_SECS))
        .build()
        .map_err(|e| {
            AppError::new(
                "NETWORK_ERROR",
                format!("failed to create HTTP client: {e}"),
            )
        })?;

    let mut rows = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        rows.push(row_for_candidate(&client, &db, candidate).await);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::providers;
    use crate::providers::{ProviderBaseUrlMode, ProviderUpsertParams};

    #[test]
    fn normalizes_usage_endpoint_variants() {
        assert_eq!(
            normalize_usage_endpoint("https://api.example.com").unwrap(),
            "https://api.example.com/v1/usage"
        );
        assert_eq!(
            normalize_usage_endpoint("https://api.example.com/v1/").unwrap(),
            "https://api.example.com/v1/usage"
        );
        assert_eq!(
            normalize_usage_endpoint(" https://api.example.com/v1/usage/ ").unwrap(),
            "https://api.example.com/v1/usage"
        );
    }

    #[test]
    fn parses_flexible_usage_response() {
        let value = serde_json::json!({
            "isValid": true,
            "planName": "Pro",
            "remaining": "12.5",
            "unit": "USD",
            "usage": {
                "today": "1.25",
                "total": { "cost": "9.5", "tokens": 1000, "requests": "3" }
            },
            "model_stats": {
                "gpt-test": { "cost": "2.5", "tokens": "44" }
            }
        });
        let snapshot = parse_usage_snapshot(&value).unwrap();
        assert_eq!(snapshot.plan_name.as_deref(), Some("Pro"));
        assert_eq!(snapshot.remaining, Some(12.5));
        assert_eq!(
            snapshot.usage.today.as_ref().and_then(|v| v.cost),
            Some(1.25)
        );
        assert_eq!(
            snapshot.usage.total.as_ref().and_then(|v| v.tokens),
            Some(1000.0)
        );
        assert_eq!(snapshot.model_stats[0].model, "gpt-test");
    }

    #[test]
    fn treats_is_valid_false_as_unauthorized() {
        let err = parse_usage_snapshot(&serde_json::json!({ "isValid": false })).unwrap_err();
        assert_eq!(err.kind, FetchFailureKind::Unauthorized);
    }

    fn create_provider(
        db: &db::Db,
        cli_key: &str,
        name: &str,
        auth_mode: Option<providers::ProviderAuthMode>,
        base_urls: Vec<String>,
        api_key: Option<String>,
        bridge_type: Option<String>,
    ) {
        providers::upsert(
            db,
            ProviderUpsertParams {
                provider_id: None,
                cli_key: cli_key.to_string(),
                name: name.to_string(),
                base_urls,
                base_url_mode: ProviderBaseUrlMode::Order,
                auth_mode,
                api_key,
                enabled: true,
                cost_multiplier: 1.0,
                priority: None,
                claude_models: None,
                limit_5h_usd: None,
                limit_daily_usd: None,
                daily_reset_mode: None,
                daily_reset_time: None,
                limit_weekly_usd: None,
                limit_monthly_usd: None,
                limit_total_usd: None,
                tags: None,
                note: None,
                source_provider_id: None,
                bridge_type,
                stream_idle_timeout_seconds: None,
                extension_values: None,
            },
        )
        .expect("create provider");
    }

    #[test]
    fn provider_source_selection_excludes_unusable_rows() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = db::init_for_tests(&dir.path().join("test.db")).expect("init db");
        create_provider(
            &db,
            "codex",
            "ok",
            Some(providers::ProviderAuthMode::ApiKey),
            vec!["https://ok.example.com".to_string()],
            Some("sk-ok".to_string()),
            None,
        );
        create_provider(
            &db,
            "codex",
            "oauth",
            Some(providers::ProviderAuthMode::Oauth),
            Vec::new(),
            None,
            None,
        );
        create_provider(
            &db,
            "codex",
            "no-key",
            Some(providers::ProviderAuthMode::ApiKey),
            vec!["https://nokey.example.com".to_string()],
            Some("".to_string()),
            None,
        );
        create_provider(
            &db,
            "claude",
            "cx2cc",
            Some(providers::ProviderAuthMode::ApiKey),
            Vec::new(),
            Some("sk-cx".to_string()),
            Some(providers::CX2CC_BRIDGE_TYPE.to_string()),
        );

        let sources = list_sources(&db, None).expect("list sources");
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].name, "ok");
    }

    #[test]
    fn custom_source_edit_empty_key_preserves_old_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = db::init_for_tests(&dir.path().join("test.db")).expect("init db");
        assert!(upsert_custom_source(
            &db,
            RemoteUsageCustomSourceUpsertInput {
                id: None,
                cli_key: "codex".to_string(),
                name: "bad".to_string(),
                base_url: "https://example.com".to_string(),
                api_key: None,
                enabled: true,
            }
        )
        .is_err());

        let created = upsert_custom_source(
            &db,
            RemoteUsageCustomSourceUpsertInput {
                id: None,
                cli_key: "codex".to_string(),
                name: "one".to_string(),
                base_url: "https://example.com".to_string(),
                api_key: Some("sk-old".to_string()),
                enabled: true,
            },
        )
        .expect("create");
        upsert_custom_source(
            &db,
            RemoteUsageCustomSourceUpsertInput {
                id: created.custom_source_id,
                cli_key: "codex".to_string(),
                name: "one-edit".to_string(),
                base_url: "https://example.com/v1".to_string(),
                api_key: Some(" ".to_string()),
                enabled: true,
            },
        )
        .expect("edit");
        let conn = db.open_connection().unwrap();
        let key: String = conn
            .query_row(
                "SELECT api_key_plaintext FROM remote_usage_custom_sources WHERE id = ?1",
                params![created.custom_source_id.unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(key, "sk-old");
    }

    #[test]
    fn cache_requires_matching_fingerprint() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = db::init_for_tests(&dir.path().join("test.db")).expect("init db");
        let conn = db.open_connection().unwrap();
        let source = RemoteUsageSourceSummary {
            source_id: "custom:1".to_string(),
            source_type: RemoteUsageSourceType::Custom,
            cli_key: "codex".to_string(),
            name: "one".to_string(),
            base_url: "https://example.com".to_string(),
            endpoint_url: "https://example.com/v1/usage".to_string(),
            enabled: true,
            provider_id: None,
            custom_source_id: Some(1),
            api_key_configured: true,
        };
        let snapshot = RemoteUsageSnapshot {
            plan_name: Some("Pro".to_string()),
            remaining: Some(1.0),
            unit: Some("USD".to_string()),
            subscription: None,
            usage: RemoteUsageUsage {
                today: None,
                week: None,
                month: None,
                total: None,
            },
            model_stats: Vec::new(),
        };
        let fp = fingerprint(&source.endpoint_url, "sk-one");
        write_cache(&conn, &source, &fp, &snapshot, 123).unwrap();
        assert!(read_cache(&conn, &source.source_id, &fp).unwrap().is_some());
        let other = fingerprint(&source.endpoint_url, "sk-two");
        assert!(read_cache(&conn, &source.source_id, &other)
            .unwrap()
            .is_none());
    }
}
