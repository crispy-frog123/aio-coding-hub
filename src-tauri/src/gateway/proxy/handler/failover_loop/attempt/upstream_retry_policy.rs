//! Usage: Shared upstream transient retry policy decisions.

use super::*;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RetryPolicyMatch {
    HttpStatus(u16),
    Transport(crate::settings::UpstreamTransportRetryKind),
}

pub(super) fn policy_matches(
    policy: &crate::settings::UpstreamRetryPolicy,
    matched: RetryPolicyMatch,
    retry_index: u32,
) -> bool {
    if !policy.enabled || policy.max_retries == 0 || retry_index > policy.max_retries {
        return false;
    }

    match matched {
        RetryPolicyMatch::HttpStatus(status) => policy.status_codes.contains(&status),
        RetryPolicyMatch::Transport(kind) => policy.transport_errors.contains(&kind),
    }
}

pub(super) fn should_retry_same_provider(
    policy: &crate::settings::UpstreamRetryPolicy,
    matched: RetryPolicyMatch,
    retry_index: u32,
    max_attempts_per_provider: u32,
) -> bool {
    policy_matches(policy, matched, retry_index) && retry_index < max_attempts_per_provider
}

pub(super) fn transient_failure_decision(
    is_count_tokens: bool,
    matched: RetryPolicyMatch,
    policy: &crate::settings::UpstreamRetryPolicy,
    retry_index: u32,
    max_attempts_per_provider: u32,
) -> (FailoverDecision, bool) {
    if is_count_tokens {
        return (FailoverDecision::Abort, false);
    }

    if should_retry_same_provider(policy, matched, retry_index, max_attempts_per_provider) {
        return (FailoverDecision::RetrySameProvider, true);
    }

    (FailoverDecision::SwitchProvider, false)
}

pub(super) fn retry_policy_backoff_delay(
    policy: &crate::settings::UpstreamRetryPolicy,
) -> Option<Duration> {
    (policy.backoff_ms > 0).then(|| Duration::from_millis(policy.backoff_ms as u64))
}

pub(super) fn should_record_circuit_failure(
    policy: &crate::settings::UpstreamRetryPolicy,
    configured_retry: bool,
) -> bool {
    !configured_retry || policy.counts_toward_circuit_breaker
}

#[cfg(test)]
mod tests {
    use super::{
        should_record_circuit_failure, transient_failure_decision, FailoverDecision,
        RetryPolicyMatch,
    };
    use crate::settings::{UpstreamRetryPolicy, UpstreamTransportRetryKind};

    #[test]
    fn transient_failure_decision_retries_default_http_and_transport_once() {
        for matched in [
            RetryPolicyMatch::HttpStatus(503),
            RetryPolicyMatch::Transport(UpstreamTransportRetryKind::Read),
            RetryPolicyMatch::Transport(UpstreamTransportRetryKind::Timeout),
        ] {
            let (decision, configured_retry) =
                transient_failure_decision(false, matched, &UpstreamRetryPolicy::default(), 1, 2);
            assert!(matches!(decision, FailoverDecision::RetrySameProvider));
            assert!(configured_retry);
        }
    }

    #[test]
    fn transient_failure_decision_switches_when_policy_is_disabled_or_unmatched() {
        let disabled = UpstreamRetryPolicy {
            enabled: false,
            ..Default::default()
        };
        let (disabled_decision, disabled_retry) =
            transient_failure_decision(false, RetryPolicyMatch::HttpStatus(503), &disabled, 1, 2);
        assert!(matches!(
            disabled_decision,
            FailoverDecision::SwitchProvider
        ));
        assert!(!disabled_retry);

        let unmatched = UpstreamRetryPolicy {
            transport_errors: vec![UpstreamTransportRetryKind::Connect],
            ..Default::default()
        };
        let (unmatched_decision, unmatched_retry) = transient_failure_decision(
            false,
            RetryPolicyMatch::Transport(UpstreamTransportRetryKind::Timeout),
            &unmatched,
            1,
            2,
        );
        assert!(matches!(
            unmatched_decision,
            FailoverDecision::SwitchProvider
        ));
        assert!(!unmatched_retry);
    }

    #[test]
    fn transient_failure_decision_respects_attempt_limits_and_count_tokens_abort() {
        let (at_limit_decision, at_limit_retry) = transient_failure_decision(
            false,
            RetryPolicyMatch::HttpStatus(503),
            &UpstreamRetryPolicy::default(),
            2,
            2,
        );
        assert!(matches!(
            at_limit_decision,
            FailoverDecision::SwitchProvider
        ));
        assert!(!at_limit_retry);

        let (count_tokens_decision, count_tokens_retry) = transient_failure_decision(
            true,
            RetryPolicyMatch::HttpStatus(503),
            &UpstreamRetryPolicy::default(),
            1,
            2,
        );
        assert!(matches!(count_tokens_decision, FailoverDecision::Abort));
        assert!(!count_tokens_retry);
    }

    #[test]
    fn should_record_circuit_failure_skips_only_configured_retries_when_requested() {
        let default_policy = UpstreamRetryPolicy::default();
        assert!(!should_record_circuit_failure(&default_policy, true));
        assert!(should_record_circuit_failure(&default_policy, false));

        let counted_policy = UpstreamRetryPolicy {
            counts_toward_circuit_breaker: true,
            ..Default::default()
        };
        assert!(should_record_circuit_failure(&counted_policy, true));
    }
}
