//! Usage: Discover models exposed by a provider and probe one model on demand.

use crate::shared::error::AppResult;
use crate::shared::http_body::read_text_with_limit;
use crate::{blocking, db};
use axum::http::{header, HeaderMap, HeaderValue};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::collections::HashSet;
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const LIST_TIMEOUT: Duration = Duration::from_secs(20);
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
const RESPONSE_BODY_LIMIT: usize = 512 * 1024;
const ERROR_PREVIEW_CHARS: usize = 800;
const MAX_MODELS: usize = 2048;
const MAX_MODEL_ID_CHARS: usize = 200;

#[derive(Debug, Clone, Serialize, specta::Type, PartialEq, Eq)]
pub struct ProviderModelInfo {
    pub id: String,
    pub display_name: Option<String>,
    pub owned_by: Option<String>,
    pub model_type: Option<String>,
    pub supported_methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ProviderModelsResult {
    pub ok: bool,
    pub provider_id: i64,
    pub provider_name: String,
    pub cli_key: String,
    pub auth_mode: String,
    pub base_url: String,
    pub endpoint: String,
    pub status: Option<u16>,
    pub latency_ms: i64,
    pub models: Vec<ProviderModelInfo>,
    pub error: Option<String>,
    pub response_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ProviderModelProbeResult {
    pub ok: bool,
    pub provider_id: i64,
    pub provider_name: String,
    pub model: String,
    pub protocol: String,
    pub endpoint: String,
    pub status: Option<u16>,
    pub latency_ms: i64,
    pub outcome: String,
    pub error: Option<String>,
    pub response_preview: Option<String>,
}

#[derive(Debug, Clone)]
struct LoadedProvider {
    id: i64,
    cli_key: String,
    name: String,
    base_urls: Vec<String>,
    api_key_plaintext: String,
    auth_mode: String,
    source_provider_id: Option<i64>,
    bridge_type: Option<String>,
}

struct ProviderRequestContext {
    base_url: String,
    headers: HeaderMap,
    oauth_access_token: Option<String>,
}

struct ResolvedOAuthCredential {
    adapter: &'static dyn crate::gateway::oauth::provider_trait::OAuthProvider,
    access_token: String,
    id_token: Option<String>,
}

struct ProbeRequest {
    protocol: &'static str,
    endpoint: String,
    headers: HeaderMap,
    body: serde_json::Value,
}

async fn load_provider(db: db::Db, provider_id: i64) -> AppResult<LoadedProvider> {
    blocking::run(
        "provider_models_load",
        move || -> AppResult<LoadedProvider> {
            if provider_id <= 0 {
                return Err(format!("SEC_INVALID_INPUT: invalid provider_id={provider_id}").into());
            }

            let conn = db.open_connection()?;
            #[allow(clippy::type_complexity)]
            let row: Option<(
                i64,
                String,
                String,
                String,
                String,
                String,
                String,
                Option<i64>,
                Option<String>,
            )> = conn
                .query_row(
                    r#"
SELECT id, cli_key, name, base_url, base_urls_json, api_key_plaintext,
       auth_mode, source_provider_id, bridge_type
FROM providers
WHERE id = ?1
"#,
                    rusqlite::params![provider_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                            row.get(8)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| format!("DB_ERROR: {e}"))?;

            let Some((
                id,
                cli_key,
                name,
                base_url_fallback,
                base_urls_json,
                api_key_plaintext,
                auth_mode,
                source_provider_id,
                bridge_type,
            )) = row
            else {
                return Err("DB_NOT_FOUND: provider not found".into());
            };

            let mut base_urls: Vec<String> = serde_json::from_str::<Vec<String>>(&base_urls_json)
                .unwrap_or_default()
                .into_iter()
                .map(|value| value.trim().trim_end_matches('/').to_string())
                .filter(|value| !value.is_empty())
                .collect();
            if base_urls.is_empty() {
                let fallback = base_url_fallback.trim().trim_end_matches('/').to_string();
                if !fallback.is_empty() {
                    base_urls.push(fallback);
                }
            }

            Ok(LoadedProvider {
                id,
                cli_key,
                name,
                base_urls,
                api_key_plaintext,
                auth_mode,
                source_provider_id,
                bridge_type,
            })
        },
    )
    .await
}

fn validate_model_id(model: &str) -> AppResult<String> {
    let model = model.trim();
    if model.is_empty() {
        return Err("SEC_INVALID_INPUT: model is required".into());
    }
    if model.chars().nth(MAX_MODEL_ID_CHARS).is_some() {
        return Err(format!(
            "SEC_INVALID_INPUT: model must contain at most {MAX_MODEL_ID_CHARS} characters"
        )
        .into());
    }
    if model.chars().any(char::is_control) {
        return Err("SEC_INVALID_INPUT: model contains control characters".into());
    }
    Ok(model.to_string())
}

fn validate_base_url_override(provider: &LoadedProvider, requested: Option<&str>) -> AppResult<()> {
    let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let normalized = requested.trim_end_matches('/');
    if !provider
        .base_urls
        .iter()
        .any(|value| value.trim_end_matches('/') == normalized)
    {
        return Err("SEC_INVALID_INPUT: base_url is not configured for provider".into());
    }
    Ok(())
}

fn parse_codex_account_id(id_token: Option<&str>) -> Option<String> {
    let payload = id_token?.trim().split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload).ok().or_else(|| {
        let mut padded = payload.to_string();
        while padded.len() % 4 != 0 {
            padded.push('=');
        }
        base64::engine::general_purpose::URL_SAFE
            .decode(padded)
            .ok()
    })?;
    let root: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    root.get("https://api.openai.com/auth")
        .and_then(|value| value.get("chatgpt_account_id"))
        .or_else(|| root.get("chatgpt_account_id"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

async fn load_oauth_details(
    db: &db::Db,
    provider_id: i64,
) -> AppResult<crate::providers::ProviderOAuthDetails> {
    blocking::run("provider_models_oauth_load", {
        let db = db.clone();
        move || crate::providers::get_oauth_details(&db, provider_id)
    })
    .await
}

fn resolve_stored_oauth_credential(
    details: &crate::providers::ProviderOAuthDetails,
) -> AppResult<ResolvedOAuthCredential> {
    let adapter = crate::gateway::oauth::registry::resolve_oauth_adapter_for_details(details)?;
    let token_set = crate::gateway::oauth::provider_trait::OAuthTokenSet {
        access_token: details.oauth_access_token.clone(),
        refresh_token: details.oauth_refresh_token.clone(),
        expires_at: details.oauth_expires_at,
        id_token: details.oauth_id_token.clone(),
    };
    let (access_token, id_token) =
        adapter.resolve_effective_token(&token_set, details.oauth_id_token.as_deref());
    let access_token = access_token.trim().to_string();
    if access_token.is_empty() {
        return Err("SEC_INVALID_INPUT: OAuth access token is empty".into());
    }
    Ok(ResolvedOAuthCredential {
        adapter,
        access_token,
        id_token,
    })
}

async fn resolve_oauth_credential(
    db: &db::Db,
    provider: &LoadedProvider,
) -> AppResult<ResolvedOAuthCredential> {
    let details = load_oauth_details(db, provider.id).await?;
    if details.cli_key != provider.cli_key {
        return Err(format!(
            "SEC_INVALID_STATE: OAuth cli_key mismatch for provider_id={} (expected={}, actual={})",
            provider.id, provider.cli_key, details.cli_key
        )
        .into());
    }

    let current = resolve_stored_oauth_credential(&details)?;
    if !crate::gateway::oauth::refresh::should_refresh_now(
        details.oauth_expires_at,
        details.oauth_refresh_lead_s,
    ) {
        return Ok(current);
    }

    let Some(refresh_token) = details
        .oauth_refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(current);
    };
    let Some(token_uri) = details
        .oauth_token_uri
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(current);
    };
    let Some(client_id) = details
        .oauth_client_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(current);
    };

    let client = crate::gateway::oauth::build_default_oauth_http_client()?;
    let refreshed = match crate::gateway::oauth::refresh::refresh_provider_token_with_retry(
        &client,
        token_uri,
        client_id,
        details.oauth_client_secret.as_deref(),
        refresh_token,
    )
    .await
    {
        Ok(value) => value,
        Err(error) => {
            let still_valid = details
                .oauth_expires_at
                .is_some_and(|expires_at| expires_at > crate::shared::time::now_unix_seconds());
            if still_valid {
                tracing::warn!(
                    provider_id = provider.id,
                    cli_key = %provider.cli_key,
                    "model detection OAuth refresh failed; using existing token: {error}"
                );
                return Ok(current);
            }
            return Err(format!("OAUTH_REFRESH_FAILED: {error}").into());
        }
    };

    let (access_token, id_token) = current
        .adapter
        .resolve_effective_token(&refreshed, details.oauth_id_token.as_deref());
    let access_token = access_token.trim().to_string();
    if access_token.is_empty() {
        return Err("OAUTH_REFRESH_FAILED: refreshed access token is empty".into());
    }

    let persisted = blocking::run("provider_models_oauth_refresh_save", {
        let db = db.clone();
        let provider_id = provider.id;
        let provider_type = current.adapter.provider_type().to_string();
        let access_token = access_token.clone();
        let new_refresh_token = refreshed
            .refresh_token
            .as_deref()
            .unwrap_or(refresh_token)
            .to_string();
        let id_token = id_token.clone();
        let token_uri = token_uri.to_string();
        let client_id = client_id.to_string();
        let client_secret = details.oauth_client_secret.clone();
        let expires_at = refreshed.expires_at.or(details.oauth_expires_at);
        let email = details.oauth_email.clone();
        let expected_last_refreshed_at = details.oauth_last_refreshed_at;
        move || {
            crate::providers::update_oauth_tokens_if_last_refreshed_matches(
                &db,
                provider_id,
                "oauth",
                &provider_type,
                &access_token,
                Some(&new_refresh_token),
                id_token.as_deref(),
                &token_uri,
                &client_id,
                client_secret.as_deref(),
                expires_at,
                email.as_deref(),
                expected_last_refreshed_at,
            )
        }
    })
    .await?;

    if !persisted {
        tracing::info!(
            provider_id = provider.id,
            cli_key = %provider.cli_key,
            "model detection OAuth refresh raced with another refresh; using latest stored token"
        );
        return resolve_stored_oauth_credential(&load_oauth_details(db, provider.id).await?);
    }

    Ok(ResolvedOAuthCredential {
        adapter: current.adapter,
        access_token,
        id_token,
    })
}

async fn request_context(
    db: &db::Db,
    provider: &LoadedProvider,
    base_url_override: Option<&str>,
) -> AppResult<ProviderRequestContext> {
    validate_base_url_override(provider, base_url_override)?;
    if provider.source_provider_id.is_some() || provider.bridge_type.as_deref() == Some("cx2cc") {
        return Err(
            "SEC_INVALID_INPUT: CX2CC provider must be tested through its source provider".into(),
        );
    }

    let mut headers = HeaderMap::new();
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));

    let (base_url, oauth_access_token) = if provider.auth_mode == "oauth" {
        let credential = resolve_oauth_credential(db, provider).await?;
        credential
            .adapter
            .inject_upstream_headers(&mut headers, &credential.access_token)
            .map_err(|e| format!("OAUTH_HEADER_ERROR: {e}"))?;
        if provider.cli_key == "codex" {
            headers.insert(
                header::USER_AGENT,
                HeaderValue::from_static(crate::gateway::oauth::DEFAULT_OAUTH_USER_AGENT),
            );
            if let Some(account_id) = parse_codex_account_id(credential.id_token.as_deref()) {
                if let Ok(value) = HeaderValue::from_str(&account_id) {
                    headers.insert("chatgpt-account-id", value);
                }
            }
        }
        (
            base_url_override
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| provider.base_urls.first().map(String::as_str))
                .unwrap_or_else(|| credential.adapter.default_base_url())
                .trim_end_matches('/')
                .to_string(),
            Some(credential.access_token),
        )
    } else {
        let key = provider.api_key_plaintext.trim();
        if key.is_empty() {
            return Err("SEC_INVALID_INPUT: provider API Key is empty".into());
        }
        crate::gateway::util::inject_provider_auth(&provider.cli_key, key, &mut headers);
        crate::gateway::util::ensure_cli_required_headers(&provider.cli_key, &mut headers);
        (
            base_url_override
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| provider.base_urls.first().map(String::as_str))
                .ok_or_else(|| {
                    crate::shared::error::AppError::from(
                        "SEC_INVALID_INPUT: provider Base URL is empty".to_string(),
                    )
                })?
                .trim_end_matches('/')
                .to_string(),
            None,
        )
    };

    Ok(ProviderRequestContext {
        base_url,
        headers,
        oauth_access_token,
    })
}

fn is_codex_chatgpt_base_url(base_url: &str) -> bool {
    reqwest::Url::parse(base_url).ok().is_some_and(|url| {
        url.path()
            .trim_end_matches('/')
            .ends_with("/backend-api/codex")
    })
}

fn models_path(cli_key: &str, base_url: &str) -> &'static str {
    match cli_key {
        "gemini" => "/v1beta/models",
        "codex" if is_codex_chatgpt_base_url(base_url) => "/models",
        _ => "/v1/models",
    }
}

fn extract_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn normalize_model_id(value: &str) -> Option<String> {
    let value = value.trim().strip_prefix("models/").unwrap_or(value.trim());
    if value.is_empty() || value.chars().nth(MAX_MODEL_ID_CHARS).is_some() {
        return None;
    }
    Some(value.to_string())
}

fn parse_model_item(value: &serde_json::Value) -> Option<ProviderModelInfo> {
    if let Some(id) = value.as_str().and_then(normalize_model_id) {
        return Some(ProviderModelInfo {
            id,
            display_name: None,
            owned_by: None,
            model_type: None,
            supported_methods: Vec::new(),
        });
    }

    let object = value.as_object()?;
    let id = ["id", "model", "slug", "name", "modelId", "model_id"]
        .into_iter()
        .find_map(|key| extract_string(object.get(key)))
        .and_then(|value| normalize_model_id(&value))?;
    let supported_methods = object
        .get("supportedGenerationMethods")
        .or_else(|| object.get("supported_methods"))
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| extract_string(Some(item)))
                .collect()
        })
        .unwrap_or_default();

    Some(ProviderModelInfo {
        id,
        display_name: extract_string(
            object
                .get("displayName")
                .or_else(|| object.get("display_name")),
        ),
        owned_by: extract_string(object.get("owned_by").or_else(|| object.get("ownedBy"))),
        model_type: extract_string(object.get("object").or_else(|| object.get("type"))),
        supported_methods,
    })
}

fn parse_models_response(root: &serde_json::Value) -> Vec<ProviderModelInfo> {
    let items = root
        .as_array()
        .or_else(|| root.get("data").and_then(serde_json::Value::as_array))
        .or_else(|| root.get("models").and_then(serde_json::Value::as_array));
    let Some(items) = items else {
        return Vec::new();
    };

    let mut seen = HashSet::new();
    items
        .iter()
        .filter_map(parse_model_item)
        .filter(|model| seen.insert(model.id.clone()))
        .take(MAX_MODELS)
        .collect()
}

fn body_preview(body: &str) -> String {
    let body = body.trim();
    let mut preview: String = body.chars().take(ERROR_PREVIEW_CHARS).collect();
    if body.chars().count() > ERROR_PREVIEW_CHARS {
        preview.push_str(" [truncated]");
    }
    preview
}

fn response_error(body: &str, status: u16) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|root| {
            root.get("error")
                .and_then(|error| {
                    error
                        .get("message")
                        .and_then(serde_json::Value::as_str)
                        .or_else(|| error.as_str())
                })
                .or_else(|| root.get("message").and_then(serde_json::Value::as_str))
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("HTTP {status}"))
}

fn elapsed_ms(started: Instant) -> i64 {
    started.elapsed().as_millis().min(i64::MAX as u128) as i64
}

pub async fn list_provider_models(
    db: db::Db,
    provider_id: i64,
    base_url: Option<String>,
) -> AppResult<ProviderModelsResult> {
    let provider = load_provider(db.clone(), provider_id).await?;
    let context = request_context(&db, &provider, base_url.as_deref()).await?;
    if provider.cli_key == "gemini" && provider.auth_mode == "oauth" {
        return Ok(ProviderModelsResult {
            ok: false,
            provider_id: provider.id,
            provider_name: provider.name,
            cli_key: provider.cli_key,
            auth_mode: provider.auth_mode,
            base_url: context.base_url.clone(),
            endpoint: context.base_url,
            status: None,
            latency_ms: 0,
            models: Vec::new(),
            error: Some(
                "Gemini OAuth / Code Assist 未提供模型目录接口，请手动添加模型后逐个检测"
                    .to_string(),
            ),
            response_preview: None,
        });
    }
    let path = models_path(&provider.cli_key, &context.base_url);
    let client_version_query = (provider.cli_key == "codex").then(|| {
        let version = crate::gateway::oauth::DEFAULT_OAUTH_USER_AGENT
            .strip_prefix("codex_cli_rs/")
            .unwrap_or("0.137.0");
        format!("client_version={version}")
    });
    let endpoint = crate::gateway::util::build_target_url(
        &context.base_url,
        path,
        client_version_query.as_deref(),
    )?;
    let endpoint_text = endpoint.to_string();
    let client = reqwest::Client::builder()
        .user_agent(format!(
            "aio-coding-hub-models/{}",
            env!("CARGO_PKG_VERSION")
        ))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(LIST_TIMEOUT)
        .build()
        .map_err(|e| format!("HTTP_CLIENT_INIT: {e}"))?;
    let started = Instant::now();
    let response = client.get(endpoint).headers(context.headers).send().await;

    match response {
        Ok(response) => {
            let status = response.status().as_u16();
            let text = match read_text_with_limit(response, RESPONSE_BODY_LIMIT, "provider models")
                .await
            {
                Ok(text) => text,
                Err(error) => {
                    return Ok(ProviderModelsResult {
                        ok: false,
                        provider_id: provider.id,
                        provider_name: provider.name,
                        cli_key: provider.cli_key,
                        auth_mode: provider.auth_mode,
                        base_url: context.base_url,
                        endpoint: endpoint_text,
                        status: Some(status),
                        latency_ms: elapsed_ms(started),
                        models: Vec::new(),
                        error: Some(format!("读取模型列表响应失败: {error}")),
                        response_preview: None,
                    });
                }
            };
            let latency_ms = elapsed_ms(started);
            if !(200..300).contains(&status) {
                return Ok(ProviderModelsResult {
                    ok: false,
                    provider_id: provider.id,
                    provider_name: provider.name,
                    cli_key: provider.cli_key,
                    auth_mode: provider.auth_mode,
                    base_url: context.base_url,
                    endpoint: endpoint_text,
                    status: Some(status),
                    latency_ms,
                    models: Vec::new(),
                    error: Some(response_error(&text, status)),
                    response_preview: Some(body_preview(&text)),
                });
            }

            let parsed = serde_json::from_str::<serde_json::Value>(&text).ok();
            let models = parsed
                .as_ref()
                .map(parse_models_response)
                .unwrap_or_default();
            let parse_error = if parsed.is_none() {
                Some("模型接口返回的内容不是有效 JSON".to_string())
            } else if models.is_empty() {
                Some("模型接口未返回可识别的模型 ID".to_string())
            } else {
                None
            };
            Ok(ProviderModelsResult {
                ok: parse_error.is_none(),
                provider_id: provider.id,
                provider_name: provider.name,
                cli_key: provider.cli_key,
                auth_mode: provider.auth_mode,
                base_url: context.base_url,
                endpoint: endpoint_text,
                status: Some(status),
                latency_ms,
                models,
                error: parse_error,
                response_preview: None,
            })
        }
        Err(error) => Ok(ProviderModelsResult {
            ok: false,
            provider_id: provider.id,
            provider_name: provider.name,
            cli_key: provider.cli_key,
            auth_mode: provider.auth_mode,
            base_url: context.base_url,
            endpoint: endpoint_text,
            status: None,
            latency_ms: elapsed_ms(started),
            models: Vec::new(),
            error: Some(if error.is_timeout() {
                "获取模型列表超时".to_string()
            } else {
                format!("获取模型列表失败: {error}")
            }),
            response_preview: None,
        }),
    }
}

fn build_gemini_oauth_probe_request(
    context: &ProviderRequestContext,
    model: &str,
    project_id: &str,
) -> AppResult<ProbeRequest> {
    let mut headers = context.headers.clone();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(ProbeRequest {
        protocol: "generate_content",
        endpoint: crate::gateway::util::build_target_url(
            crate::gateway::oauth::adapters::gemini::GEMINI_CODE_ASSIST_BASE_URL,
            &format!(
                "/{}:generateContent",
                crate::gateway::oauth::adapters::gemini::GEMINI_CODE_ASSIST_API_VERSION
            ),
            None,
        )?
        .to_string(),
        headers,
        body: serde_json::json!({
            "model": model,
            "project": project_id,
            "request": {
                "contents": [{"parts": [{"text": "ping"}]}],
                "generationConfig": {"maxOutputTokens": 1}
            }
        }),
    })
}

async fn build_probe_requests(
    client: &reqwest::Client,
    provider: &LoadedProvider,
    context: &ProviderRequestContext,
    model: &str,
) -> AppResult<Vec<ProbeRequest>> {
    let mut headers = context.headers.clone();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    match provider.cli_key.as_str() {
        "claude" => Ok(vec![ProbeRequest {
            protocol: "messages",
            endpoint: crate::gateway::util::build_target_url(
                &context.base_url,
                "/v1/messages",
                None,
            )?
            .to_string(),
            headers,
            body: serde_json::json!({
                "model": model,
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "ping"}]
            }),
        }]),
        "gemini" if provider.auth_mode == "oauth" => {
            let access_token = context.oauth_access_token.as_deref().ok_or_else(|| {
                crate::shared::error::AppError::from(
                    "SEC_INVALID_STATE: Gemini OAuth access token is unavailable".to_string(),
                )
            })?;
            let project_id =
                crate::gateway::oauth::adapters::gemini::resolve_project_id_for_access_token(
                    client,
                    access_token,
                )
                .await
                .map_err(|error| format!("GEMINI_OAUTH_PROJECT_ERROR: {error}"))?;
            Ok(vec![build_gemini_oauth_probe_request(
                context,
                model.strip_prefix("models/").unwrap_or(model),
                &project_id,
            )?])
        }
        "gemini" => {
            let model = model.strip_prefix("models/").unwrap_or(model);
            let encoded = crate::gateway::util::encode_url_component(model);
            Ok(vec![ProbeRequest {
                protocol: "generate_content",
                endpoint: crate::gateway::util::build_target_url(
                    &context.base_url,
                    &format!("/v1beta/models/{encoded}:generateContent"),
                    None,
                )?
                .to_string(),
                headers,
                body: serde_json::json!({
                    "contents": [{"parts": [{"text": "ping"}]}],
                    "generationConfig": {"maxOutputTokens": 1}
                }),
            }])
        }
        "codex" => {
            let chatgpt_backend = is_codex_chatgpt_base_url(&context.base_url);
            let responses_path = if chatgpt_backend {
                "/responses"
            } else {
                "/v1/responses"
            };
            let responses_body = if chatgpt_backend {
                serde_json::json!({
                    "model": model,
                    "instructions": "",
                    "input": "ping",
                    "stream": true,
                    "store": false
                })
            } else {
                serde_json::json!({
                    "model": model,
                    "input": "ping",
                    "max_output_tokens": 1,
                    "stream": false
                })
            };
            let mut requests = vec![ProbeRequest {
                protocol: "responses",
                endpoint: crate::gateway::util::build_target_url(
                    &context.base_url,
                    responses_path,
                    None,
                )?
                .to_string(),
                headers: headers.clone(),
                body: responses_body,
            }];
            if !chatgpt_backend {
                requests.push(ProbeRequest {
                    protocol: "chat_completions",
                    endpoint: crate::gateway::util::build_target_url(
                        &context.base_url,
                        "/v1/chat/completions",
                        None,
                    )?
                    .to_string(),
                    headers,
                    body: serde_json::json!({
                        "model": model,
                        "max_tokens": 1,
                        "messages": [{"role": "user", "content": "ping"}]
                    }),
                });
            }
            Ok(requests)
        }
        other => Err(format!("UNSUPPORTED_CLI_KEY: {other}").into()),
    }
}

fn should_try_chat_completions(status: u16, body: &str) -> bool {
    if matches!(status, 405 | 501) {
        return true;
    }
    let lower = body.to_ascii_lowercase();
    if status == 404 {
        let model_specific = lower.contains("model")
            && [
                "not found",
                "does not exist",
                "unsupported",
                "not available",
                "invalid model",
            ]
            .iter()
            .any(|needle| lower.contains(needle));
        return !model_specific;
    }
    [
        "unknown endpoint",
        "unsupported endpoint",
        "endpoint not found",
        "route not found",
        "responses api is not supported",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn probe_outcome(status: u16, body: &str) -> &'static str {
    if (200..300).contains(&status) {
        "available"
    } else if status == 429 {
        "rate_limited"
    } else if matches!(status, 401 | 403) {
        "auth_error"
    } else {
        let lower = body.to_ascii_lowercase();
        if lower.contains("model")
            && [
                "not found",
                "does not exist",
                "unsupported",
                "not available",
                "invalid model",
            ]
            .iter()
            .any(|needle| lower.contains(needle))
        {
            "model_unavailable"
        } else {
            "request_failed"
        }
    }
}

pub async fn probe_provider_model(
    db: db::Db,
    provider_id: i64,
    model: String,
    base_url: Option<String>,
) -> AppResult<ProviderModelProbeResult> {
    let model = validate_model_id(&model)?;
    let provider = load_provider(db.clone(), provider_id).await?;
    let context = request_context(&db, &provider, base_url.as_deref()).await?;
    let client = reqwest::Client::builder()
        .user_agent(format!(
            "aio-coding-hub-model-probe/{}",
            env!("CARGO_PKG_VERSION")
        ))
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|e| format!("HTTP_CLIENT_INIT: {e}"))?;
    let requests = build_probe_requests(&client, &provider, &context, &model).await?;

    let mut last_result = None;
    for (index, request) in requests.into_iter().enumerate() {
        let started = Instant::now();
        let response = client
            .post(&request.endpoint)
            .headers(request.headers)
            .json(&request.body)
            .send()
            .await;
        let result = match response {
            Ok(response) => {
                let status = response.status().as_u16();
                let text = match read_text_with_limit(response, RESPONSE_BODY_LIMIT, "model probe")
                    .await
                {
                    Ok(text) => text,
                    Err(error) => {
                        let ok = (200..300).contains(&status);
                        return Ok(ProviderModelProbeResult {
                            ok,
                            provider_id: provider.id,
                            provider_name: provider.name.clone(),
                            model: model.clone(),
                            protocol: request.protocol.to_string(),
                            endpoint: request.endpoint,
                            status: Some(status),
                            latency_ms: elapsed_ms(started),
                            outcome: if ok { "available" } else { "request_failed" }.to_string(),
                            error: (!ok).then(|| format!("读取模型检测响应失败: {error}")),
                            response_preview: None,
                        });
                    }
                };
                let ok = (200..300).contains(&status);
                ProviderModelProbeResult {
                    ok,
                    provider_id: provider.id,
                    provider_name: provider.name.clone(),
                    model: model.clone(),
                    protocol: request.protocol.to_string(),
                    endpoint: request.endpoint,
                    status: Some(status),
                    latency_ms: elapsed_ms(started),
                    outcome: probe_outcome(status, &text).to_string(),
                    error: (!ok).then(|| response_error(&text, status)),
                    response_preview: (!ok).then(|| body_preview(&text)),
                }
            }
            Err(error) => ProviderModelProbeResult {
                ok: false,
                provider_id: provider.id,
                provider_name: provider.name.clone(),
                model: model.clone(),
                protocol: request.protocol.to_string(),
                endpoint: request.endpoint,
                status: None,
                latency_ms: elapsed_ms(started),
                outcome: if error.is_timeout() {
                    "timeout".to_string()
                } else {
                    "network_error".to_string()
                },
                error: Some(if error.is_timeout() {
                    "模型检测超时".to_string()
                } else {
                    format!("模型检测失败: {error}")
                }),
                response_preview: None,
            },
        };

        let try_fallback = provider.cli_key == "codex"
            && index == 0
            && !result.ok
            && result.status.is_some_and(|status| {
                should_try_chat_completions(
                    status,
                    result.response_preview.as_deref().unwrap_or_default(),
                )
            });
        if !try_fallback {
            return Ok(result);
        }
        last_result = Some(result);
    }

    last_result.ok_or_else(|| "INTERNAL_ERROR: no model probe request was built".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_anthropic_gemini_and_string_model_shapes() {
        let openai = serde_json::json!({
            "data": [
                {"id": "deepseek-r1", "owned_by": "deepseek"},
                {"id": "qwen3", "owned_by": "qwen"}
            ]
        });
        assert_eq!(
            parse_models_response(&openai)
                .into_iter()
                .map(|model| model.id)
                .collect::<Vec<_>>(),
            vec!["deepseek-r1", "qwen3"]
        );

        let models = serde_json::json!({
            "models": [
                {"name": "models/gemini-2.5-pro", "displayName": "Gemini 2.5 Pro", "supportedGenerationMethods": ["generateContent"]},
                {"id": "claude-sonnet-4-6", "type": "model"},
                "mistral-large"
            ]
        });
        let parsed = parse_models_response(&models);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].id, "gemini-2.5-pro");
        assert_eq!(parsed[0].supported_methods, vec!["generateContent"]);
        assert_eq!(parsed[2].id, "mistral-large");

        let codex_oauth = serde_json::json!({
            "models": [{
                "slug": "gpt-5.5",
                "display_name": "GPT-5.5",
                "use_responses_lite": true
            }]
        });
        let parsed = parse_models_response(&codex_oauth);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].id, "gpt-5.5");
        assert_eq!(parsed[0].display_name.as_deref(), Some("GPT-5.5"));
    }

    #[test]
    fn preserves_order_and_deduplicates_model_ids() {
        let root = serde_json::json!({
            "data": [{"id": "b"}, {"id": "a"}, {"id": "b"}]
        });
        assert_eq!(
            parse_models_response(&root)
                .into_iter()
                .map(|model| model.id)
                .collect::<Vec<_>>(),
            vec!["b", "a"]
        );
    }

    #[test]
    fn builds_expected_models_paths_without_duplicate_api_versions() {
        assert_eq!(
            models_path("codex", "https://api.example.com/v1"),
            "/v1/models"
        );
        assert_eq!(
            models_path("claude", "https://api.example.com"),
            "/v1/models"
        );
        assert_eq!(
            models_path("gemini", "https://api.example.com/v1beta"),
            "/v1beta/models"
        );
        assert_eq!(
            models_path("codex", "https://chatgpt.com/backend-api/codex"),
            "/models"
        );
    }

    #[test]
    fn fallback_only_applies_to_missing_responses_endpoint() {
        assert!(should_try_chat_completions(404, "not found"));
        assert!(should_try_chat_completions(404, "endpoint not found"));
        assert!(should_try_chat_completions(
            400,
            "Responses API is not supported"
        ));
        assert!(!should_try_chat_completions(404, "model not found"));
        assert!(!should_try_chat_completions(400, "model is not supported"));
        assert!(!should_try_chat_completions(401, "unauthorized"));
    }

    #[tokio::test]
    async fn codex_chatgpt_probe_uses_only_responses_with_compatible_body() {
        let provider = LoadedProvider {
            id: 1,
            cli_key: "codex".to_string(),
            name: "OpenAI OAuth".to_string(),
            base_urls: vec!["https://chatgpt.com/backend-api/codex".to_string()],
            api_key_plaintext: String::new(),
            auth_mode: "oauth".to_string(),
            source_provider_id: None,
            bridge_type: None,
        };
        let context = ProviderRequestContext {
            base_url: "https://chatgpt.com/backend-api/codex".to_string(),
            headers: HeaderMap::new(),
            oauth_access_token: Some("token".to_string()),
        };
        let client = reqwest::Client::new();

        let requests = build_probe_requests(&client, &provider, &context, "gpt-5.5")
            .await
            .expect("build ChatGPT model probe");

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].protocol, "responses");
        assert_eq!(
            requests[0].endpoint,
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(requests[0].body["stream"], true);
        assert_eq!(requests[0].body["store"], false);
        assert!(requests[0].body.get("max_output_tokens").is_none());
    }

    #[tokio::test]
    async fn codex_api_key_probe_keeps_chat_completions_fallback() {
        let provider = LoadedProvider {
            id: 1,
            cli_key: "codex".to_string(),
            name: "Compatible Provider".to_string(),
            base_urls: vec!["https://api.example.com/v1".to_string()],
            api_key_plaintext: "secret".to_string(),
            auth_mode: "api_key".to_string(),
            source_provider_id: None,
            bridge_type: None,
        };
        let context = ProviderRequestContext {
            base_url: "https://api.example.com/v1".to_string(),
            headers: HeaderMap::new(),
            oauth_access_token: None,
        };
        let client = reqwest::Client::new();

        let requests = build_probe_requests(&client, &provider, &context, "deepseek-r1")
            .await
            .expect("build compatible provider probe");

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].protocol, "responses");
        assert_eq!(requests[1].protocol, "chat_completions");
        assert_eq!(requests[0].endpoint, "https://api.example.com/v1/responses");
        assert_eq!(
            requests[1].endpoint,
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn gemini_oauth_probe_wraps_the_model_and_project_for_code_assist() {
        let context = ProviderRequestContext {
            base_url: "https://cloudcode-pa.googleapis.com".to_string(),
            headers: HeaderMap::new(),
            oauth_access_token: Some("ya29.token".to_string()),
        };

        let request =
            build_gemini_oauth_probe_request(&context, "gemini-2.5-flash", "projects/test-project")
                .expect("build Gemini OAuth model probe");

        assert_eq!(request.protocol, "generate_content");
        assert_eq!(
            request.endpoint,
            "https://cloudcode-pa.googleapis.com/v1internal:generateContent"
        );
        assert_eq!(request.body["model"], "gemini-2.5-flash");
        assert_eq!(request.body["project"], "projects/test-project");
        assert_eq!(
            request.body["request"]["contents"][0]["parts"][0]["text"],
            "ping"
        );
    }

    #[test]
    fn classifies_probe_outcomes() {
        assert_eq!(probe_outcome(200, "{}"), "available");
        assert_eq!(probe_outcome(429, "rate limit"), "rate_limited");
        assert_eq!(probe_outcome(401, "unauthorized"), "auth_error");
        assert_eq!(
            probe_outcome(400, "model does not exist"),
            "model_unavailable"
        );
    }
}
