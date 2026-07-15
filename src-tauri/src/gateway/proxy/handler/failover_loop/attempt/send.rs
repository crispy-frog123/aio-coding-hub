//! Usage: Upstream request sending helpers (first-byte timeout aware).

use super::context::CommonCtx;
use axum::body::Bytes;
use axum::http::{HeaderMap, Method};

pub(super) enum SendResult {
    Ok(reqwest::Response),
    Err(reqwest::Error),
    Timeout,
    LayeredTimeout(super::layered_policy::TimeoutPhase),
}

pub(super) async fn send_upstream<R: tauri::Runtime>(
    ctx: CommonCtx<'_, R>,
    method: Method,
    url: reqwest::Url,
    headers: HeaderMap,
    body: Bytes,
    policy_timing: super::layered_policy::AttemptPolicyTiming,
) -> SendResult {
    let client = ctx.state.client();
    let send = client
        .request(method, url)
        .headers(headers)
        .body(body)
        .send();

    let first_byte_deadline = ctx
        .upstream_first_byte_timeout
        .and_then(|timeout| policy_timing.dispatched_at.checked_add(timeout));
    let deadline = earliest_send_deadline(
        first_byte_deadline,
        policy_timing.first_progress_deadline,
        policy_timing.total_deadline,
    );

    if let Some((deadline, kind)) = deadline {
        match tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), send).await {
            Ok(Ok(resp)) => SendResult::Ok(resp),
            Ok(Err(err)) => SendResult::Err(err),
            Err(_) => match kind {
                SendDeadlineKind::FirstByte => SendResult::Timeout,
                SendDeadlineKind::Layered(phase) => SendResult::LayeredTimeout(phase),
            },
        }
    } else {
        match send.await {
            Ok(resp) => SendResult::Ok(resp),
            Err(err) => SendResult::Err(err),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendDeadlineKind {
    FirstByte,
    Layered(super::layered_policy::TimeoutPhase),
}

fn earliest_send_deadline(
    first_byte: Option<std::time::Instant>,
    first_progress: Option<std::time::Instant>,
    total: Option<std::time::Instant>,
) -> Option<(std::time::Instant, SendDeadlineKind)> {
    let mut candidates = Vec::with_capacity(3);
    if let Some(deadline) = first_byte {
        candidates.push((deadline, SendDeadlineKind::FirstByte, 2u8));
    }
    if let Some(deadline) = first_progress {
        candidates.push((
            deadline,
            SendDeadlineKind::Layered(super::layered_policy::TimeoutPhase::FirstProgress),
            1u8,
        ));
    }
    if let Some(deadline) = total {
        candidates.push((
            deadline,
            SendDeadlineKind::Layered(super::layered_policy::TimeoutPhase::Total),
            0u8,
        ));
    }
    candidates
        .into_iter()
        .min_by_key(|(deadline, _, priority)| (*deadline, *priority))
        .map(|(deadline, kind, _)| (deadline, kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn total_deadline_wins_ties() {
        let now = Instant::now();
        let selected = earliest_send_deadline(Some(now), Some(now), Some(now)).unwrap();
        assert_eq!(
            selected.1,
            SendDeadlineKind::Layered(super::super::layered_policy::TimeoutPhase::Total)
        );
    }

    #[test]
    fn earliest_absolute_deadline_wins() {
        let now = Instant::now();
        let selected = earliest_send_deadline(
            Some(now + Duration::from_secs(10)),
            Some(now + Duration::from_secs(2)),
            Some(now + Duration::from_secs(20)),
        )
        .unwrap();
        assert_eq!(
            selected.1,
            SendDeadlineKind::Layered(super::super::layered_policy::TimeoutPhase::FirstProgress)
        );
    }
}
