//! Usage: Dynamic model service status monitor backed by status.input.im.

use crate::shared::error::AppError;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

const STATUS_ENDPOINT: &str = "https://status.input.im/api/status";
const STATUS_TIMEOUT_SECS: u64 = 20;
const MAX_MONITORED_SERVICES: usize = 64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatusCellKind {
    Green,
    Yellow,
    Red,
    Gray,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
pub struct ServiceStatusProbe {
    pub ts: Option<f64>,
    pub ok: Option<bool>,
    pub latency_ms: Option<i64>,
    pub error: Option<String>,
    pub kind: ServiceStatusCellKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
pub struct ServiceStatusService {
    pub model: String,
    pub uptime_pct: Option<f64>,
    pub last: Option<ServiceStatusProbe>,
    pub history: Vec<ServiceStatusProbe>,
    pub latest_kind: ServiceStatusCellKind,
    pub status_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
pub struct ServiceStatusResponse {
    pub all_ok: bool,
    pub generated_at: Option<f64>,
    pub services: Vec<ServiceStatusService>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
pub struct ServiceStatusSnapshot {
    pub endpoint_url: String,
    pub refreshed_at: i64,
    pub raw_json_text: String,
    pub response: ServiceStatusResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq)]
pub struct ServiceStatusResult {
    pub snapshot: Option<ServiceStatusSnapshot>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawResponse {
    #[serde(default)]
    all_ok: bool,
    #[serde(default, deserialize_with = "de_f64_opt")]
    generated_at: Option<f64>,
    #[serde(default)]
    services: Vec<RawService>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawService {
    model: String,
    #[serde(default, deserialize_with = "de_f64_opt")]
    uptime_pct: Option<f64>,
    #[serde(default)]
    last: Option<RawProbe>,
    #[serde(default)]
    history: Vec<RawProbe>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawProbe {
    #[serde(default, deserialize_with = "de_f64_opt")]
    ts: Option<f64>,
    #[serde(default)]
    ok: Option<bool>,
    #[serde(default, deserialize_with = "de_i64_opt")]
    latency_ms: Option<i64>,
    #[serde(default)]
    error: Option<String>,
}

fn de_f64_opt<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| match value {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }))
}

fn de_i64_opt<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| match value {
        serde_json::Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|v| v as i64)),
        serde_json::Value::String(text) => text.trim().parse::<f64>().ok().map(|v| v as i64),
        _ => None,
    }))
}

fn classify_probe(probe: Option<&ServiceStatusProbe>) -> ServiceStatusCellKind {
    let Some(probe) = probe else {
        return ServiceStatusCellKind::Gray;
    };
    match probe.ok {
        Some(false) => ServiceStatusCellKind::Red,
        Some(true) => match probe.latency_ms {
            Some(latency) if latency >= 3000 => ServiceStatusCellKind::Yellow,
            Some(_) => ServiceStatusCellKind::Green,
            None => ServiceStatusCellKind::Gray,
        },
        None => ServiceStatusCellKind::Gray,
    }
}

fn status_text(kind: ServiceStatusCellKind) -> &'static str {
    match kind {
        ServiceStatusCellKind::Green => "正常",
        ServiceStatusCellKind::Yellow => "高延迟",
        ServiceStatusCellKind::Red => "失败",
        ServiceStatusCellKind::Gray => "缺少数据",
    }
}

fn convert_probe(value: RawProbe) -> ServiceStatusProbe {
    let mut probe = ServiceStatusProbe {
        ts: value.ts,
        ok: value.ok,
        latency_ms: value.latency_ms,
        error: value.error.filter(|v| !v.trim().is_empty()),
        kind: ServiceStatusCellKind::Gray,
    };
    probe.kind = classify_probe(Some(&probe));
    probe
}

fn convert_response(raw: RawResponse) -> ServiceStatusResponse {
    let mut seen_models = HashSet::new();
    let services = raw
        .services
        .into_iter()
        .filter_map(|mut service| {
            service.model = service.model.trim().to_string();
            if service.model.is_empty() || !seen_models.insert(service.model.clone()) {
                return None;
            }
            let last = service.last.map(convert_probe);
            let history = service
                .history
                .into_iter()
                .map(convert_probe)
                .collect::<Vec<_>>();
            let latest_kind = classify_probe(last.as_ref());
            Some(ServiceStatusService {
                model: service.model,
                uptime_pct: service.uptime_pct,
                last,
                history,
                latest_kind,
                status_text: status_text(latest_kind).to_string(),
            })
        })
        .take(MAX_MONITORED_SERVICES)
        .collect::<Vec<_>>();

    ServiceStatusResponse {
        all_ok: raw.all_ok,
        generated_at: raw.generated_at,
        services,
    }
}

fn pretty_json_text(data: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(data)
        .ok()
        .and_then(|value| serde_json::to_string_pretty(&value).ok())
        .unwrap_or_else(|| String::from_utf8_lossy(data).to_string())
}

fn decode_error_message(data: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(data)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(|message| message.as_str())
                .map(str::to_string)
        })
}

pub async fn fetch_status() -> crate::shared::error::AppResult<ServiceStatusResult> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(STATUS_TIMEOUT_SECS))
        .build()
        .map_err(|e| {
            AppError::new(
                "NETWORK_ERROR",
                format!("failed to create HTTP client: {e}"),
            )
        })?;

    let response = client
        .get(STATUS_ENDPOINT)
        .send()
        .await
        .map_err(|e| AppError::new("NETWORK_ERROR", format!("status request failed: {e}")))?;
    let status = response.status();
    let bytes = response.bytes().await.map_err(|e| {
        AppError::new(
            "NETWORK_ERROR",
            format!("failed to read status response: {e}"),
        )
    })?;

    if status != StatusCode::OK {
        let message = decode_error_message(&bytes)
            .unwrap_or_else(|| format!("状态接口 HTTP {}", status.as_u16()));
        return Ok(ServiceStatusResult {
            snapshot: None,
            error: Some(message),
        });
    }

    let raw = serde_json::from_slice::<RawResponse>(&bytes)
        .map_err(|e| AppError::new("DECODE_ERROR", format!("status response format error: {e}")))?;
    let response = convert_response(raw);
    Ok(ServiceStatusResult {
        snapshot: Some(ServiceStatusSnapshot {
            endpoint_url: STATUS_ENDPOINT.to_string(),
            refreshed_at: crate::shared::time::now_unix_seconds(),
            raw_json_text: pretty_json_text(&bytes),
            response,
        }),
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_probe_kinds() {
        assert_eq!(classify_probe(None), ServiceStatusCellKind::Gray);
        assert_eq!(
            classify_probe(Some(&ServiceStatusProbe {
                ts: None,
                ok: Some(false),
                latency_ms: None,
                error: None,
                kind: ServiceStatusCellKind::Gray,
            })),
            ServiceStatusCellKind::Red
        );
        assert_eq!(
            classify_probe(Some(&ServiceStatusProbe {
                ts: None,
                ok: Some(true),
                latency_ms: Some(3000),
                error: None,
                kind: ServiceStatusCellKind::Gray,
            })),
            ServiceStatusCellKind::Yellow
        );
        assert_eq!(
            classify_probe(Some(&ServiceStatusProbe {
                ts: None,
                ok: Some(true),
                latency_ms: Some(2999),
                error: None,
                kind: ServiceStatusCellKind::Gray,
            })),
            ServiceStatusCellKind::Green
        );
    }

    #[test]
    fn decodes_dynamic_services_with_string_numbers() {
        let raw: RawResponse = serde_json::from_str(
            r#"{
              "all_ok": true,
              "generated_at": "1778762578",
              "services": [
                {
                  "model": "gpt-5.5",
                  "uptime_pct": "81.67",
                  "last": {"ts": "1778762557", "ok": true, "latency_ms": "1111"},
                  "history": [
                    {"ok": true, "latency_ms": 1103},
                    {"ok": false, "error": "boom"}
                  ]
                },
                {"model": "other", "history": []}
              ]
            }"#,
        )
        .expect("decode");
        let response = convert_response(raw);
        assert_eq!(response.services.len(), 2);
        assert_eq!(response.services[0].model, "gpt-5.5");
        assert_eq!(response.services[0].uptime_pct, Some(81.67));
        assert_eq!(
            response.services[0].latest_kind,
            ServiceStatusCellKind::Green
        );
        assert_eq!(
            response.services[0].history[1].kind,
            ServiceStatusCellKind::Red
        );
        assert_eq!(response.services[1].model, "other");
    }

    #[test]
    fn dynamic_services_preserve_order_and_drop_blank_or_duplicate_models() {
        let raw: RawResponse = serde_json::from_str(
            r#"{
              "services": [
                {"model":" gpt-5.6-sol ","history":[]},
                {"model":"","history":[]},
                {"model":"gpt-5.6-terra","history":[]},
                {"model":"gpt-5.6-sol","history":[]}
              ]
            }"#,
        )
        .expect("decode");

        let response = convert_response(raw);
        let models = response
            .services
            .iter()
            .map(|service| service.model.as_str())
            .collect::<Vec<_>>();
        assert_eq!(models, vec!["gpt-5.6-sol", "gpt-5.6-terra"]);
    }
}
