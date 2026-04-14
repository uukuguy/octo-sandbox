//! Error classification for provider failures (hermes pattern).
//!
//! Maps `ProviderError` → `FailoverReason` → `RecoveryActions`. Unlike the
//! coarse-grained `LlmErrorKind` (8 variants, HTTP-centric), `FailoverReason`
//! distinguishes recovery-relevant cases (e.g. `AuthPermanent` vs transient
//! `Auth`, `PayloadTooLarge` vs `ContextOverflow`, `ThinkingSignature` vs
//! generic `FormatError`) so `withRetry` can pick the right action.
//!
//! Reference: `docs/design/EAASP/AGENT_LOOP_PATTERNS_TO_ADOPT.md` §2
//! (hermes `error_classifier.py:25-80`).

use super::retry::{LlmErrorKind, ProviderError};

/// Why a provider call failed, at a granularity useful for recovery routing.
///
/// This is a superset of `LlmErrorKind` — it splits several `LlmErrorKind`
/// variants into finer-grained reasons that map to distinct `RecoveryActions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailoverReason {
    /// 401/403 transient auth (e.g. token expired, can retry after rotating credential).
    Auth,
    /// 401/403 permanent (invalid key, revoked, wrong account) — never retry.
    AuthPermanent,
    /// 402 / "credit_balance_too_low" — billing issue, do not retry.
    Billing,
    /// 429 rate-limit — retryable with backoff + credential rotation.
    RateLimit,
    /// 529 / "overloaded" — provider-side load, failover preferred.
    Overloaded,
    /// 500 / 502 / 503 — transient server error.
    ServerError,
    /// 408 / 504 / connection reset — transient network-level timeout.
    Timeout,
    /// Context window exceeded — compact then retry.
    ContextOverflow,
    /// 413 request body too large (distinct from context window).
    PayloadTooLarge,
    /// Requested model not available — failover to alternative model.
    ModelNotFound,
    /// Response could not be parsed (malformed JSON, schema mismatch).
    FormatError,
    /// Anthropic "thinking signature" / redaction-related failure — usually retry.
    ThinkingSignature,
    /// Tier-limited long-context model unavailable at requested length.
    LongContextTier,
    /// Unclassified error — conservative default (retryable, fallback allowed).
    Unknown,
}

/// Recovery actions a retry/failover pipeline should take for a given reason.
///
/// Each flag is independent. For example, `RateLimit` sets all four true
/// (retry same provider, compress not helpful, rotate credential, allow
/// fallback if retries exhausted), while `Billing` sets all false.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryActions {
    /// Caller may retry the same request on the same provider.
    pub retryable: bool,
    /// Caller should compress / trim context before retrying.
    pub should_compress: bool,
    /// Caller should rotate to a different credential / API key.
    pub should_rotate_credential: bool,
    /// Caller may fall back to a different provider / model.
    pub should_fallback: bool,
}

impl RecoveryActions {
    const NONE: Self = Self {
        retryable: false,
        should_compress: false,
        should_rotate_credential: false,
        should_fallback: false,
    };
}

impl FailoverReason {
    /// Classify a `ProviderError` into a `FailoverReason`.
    ///
    /// Uses the pre-computed `retry_info.kind` from the HTTP layer, then
    /// inspects `status`, `body`, and `retry_info.error_code` to refine into
    /// variants the coarser `LlmErrorKind` doesn't distinguish.
    pub fn classify(err: &ProviderError) -> Self {
        // Fast path: explicit HTTP status codes that map 1:1 to a FailoverReason
        // without needing body inspection.
        match err.status {
            413 => return Self::PayloadTooLarge,
            404 if is_model_not_found(&err.body, err.retry_info.error_code.as_deref()) => {
                return Self::ModelNotFound
            }
            _ => {}
        }

        // Body-driven refinements that the broad LlmErrorKind doesn't capture.
        let body_lc = err.body.to_lowercase();
        if is_thinking_signature(&body_lc, err.retry_info.error_code.as_deref()) {
            return Self::ThinkingSignature;
        }
        if is_long_context_tier(&body_lc, err.retry_info.error_code.as_deref()) {
            return Self::LongContextTier;
        }
        if is_model_not_found(&err.body, err.retry_info.error_code.as_deref()) {
            return Self::ModelNotFound;
        }
        if is_payload_too_large(&body_lc, err.retry_info.error_code.as_deref()) {
            return Self::PayloadTooLarge;
        }
        if is_format_error(&body_lc, err.retry_info.error_code.as_deref()) {
            return Self::FormatError;
        }

        // Fall back to the 8-variant HTTP classification, splitting Auth further.
        match err.retry_info.kind {
            LlmErrorKind::RateLimit => Self::RateLimit,
            LlmErrorKind::Overloaded => Self::Overloaded,
            LlmErrorKind::Timeout => Self::Timeout,
            LlmErrorKind::ServiceError => Self::ServerError,
            LlmErrorKind::BillingError => Self::Billing,
            LlmErrorKind::AuthError => {
                if is_auth_permanent(&body_lc, err.retry_info.error_code.as_deref()) {
                    Self::AuthPermanent
                } else {
                    Self::Auth
                }
            }
            LlmErrorKind::ContextOverflow => Self::ContextOverflow,
            LlmErrorKind::Unknown => Self::Unknown,
        }
    }

    /// Recovery actions associated with this reason.
    pub fn recovery_actions(&self) -> RecoveryActions {
        match self {
            // Transient auth — retry after rotating credential, and allow
            // fallback: if the only credential slot is bad, a backup provider
            // with its own key is the escape hatch.
            Self::Auth => RecoveryActions {
                retryable: true,
                should_compress: false,
                should_rotate_credential: true,
                should_fallback: true,
            },
            // Permanent auth (invalid key, revoked) — give up.
            Self::AuthPermanent => RecoveryActions::NONE,
            // Billing — never retry.
            Self::Billing => RecoveryActions::NONE,
            // Rate-limit — retry, rotate credential, allow fallback if exhausted.
            Self::RateLimit => RecoveryActions {
                retryable: true,
                should_compress: false,
                should_rotate_credential: true,
                should_fallback: true,
            },
            // Provider overloaded — prefer failover, still retryable.
            Self::Overloaded => RecoveryActions {
                retryable: true,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: true,
            },
            // 5xx server error — retry + fallback.
            Self::ServerError => RecoveryActions {
                retryable: true,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: true,
            },
            // Network timeout — retry + fallback.
            Self::Timeout => RecoveryActions {
                retryable: true,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: true,
            },
            // Context window — must compress before retry; a raw fallback
            // to another provider just hits the same wall on the same payload.
            // PreCompact (S3.T1) handles the cross-provider context-window
            // routing separately, layered on top of this flag.
            Self::ContextOverflow => RecoveryActions {
                retryable: true,
                should_compress: true,
                should_rotate_credential: false,
                should_fallback: false,
            },
            // Payload-too-large — must compress, no credential/fallback.
            Self::PayloadTooLarge => RecoveryActions {
                retryable: true,
                should_compress: true,
                should_rotate_credential: false,
                should_fallback: false,
            },
            // Model not found — retry pointless on same provider, fallback only.
            Self::ModelNotFound => RecoveryActions {
                retryable: false,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: true,
            },
            // Format error — caller bug, don't retry; may fallback to different
            // model that tolerates the input shape.
            Self::FormatError => RecoveryActions {
                retryable: false,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: true,
            },
            // Thinking-signature — retry same model (transient).
            Self::ThinkingSignature => RecoveryActions {
                retryable: true,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: false,
            },
            // Long-context tier unavailable — fallback to shorter-context model.
            Self::LongContextTier => RecoveryActions {
                retryable: false,
                should_compress: true,
                should_rotate_credential: false,
                should_fallback: true,
            },
            // Unknown — conservative: retry once, allow fallback.
            Self::Unknown => RecoveryActions {
                retryable: true,
                should_compress: false,
                should_rotate_credential: false,
                should_fallback: true,
            },
        }
    }
}

fn is_auth_permanent(body_lc: &str, error_code: Option<&str>) -> bool {
    if let Some(code) = error_code {
        let c = code.to_lowercase();
        if c.contains("invalid_api_key")
            || c.contains("invalid_request_error")
            || c.contains("permission_denied")
            || c.contains("account_deactivated")
            || c.contains("revoked")
        {
            return true;
        }
    }
    body_lc.contains("invalid_api_key")
        || body_lc.contains("account_deactivated")
        || body_lc.contains("account has been deactivated")
        || body_lc.contains("api key has been revoked")
        || body_lc.contains("permission_denied")
}

fn is_model_not_found(body: &str, error_code: Option<&str>) -> bool {
    let body_lc = body.to_lowercase();
    if let Some(code) = error_code {
        let c = code.to_lowercase();
        if c.contains("model_not_found") || c.contains("model_not_supported") {
            return true;
        }
    }
    body_lc.contains("model_not_found")
        || body_lc.contains("model not found")
        || body_lc.contains("does not exist")
            && (body_lc.contains("model") || body_lc.contains("engine"))
}

fn is_payload_too_large(body_lc: &str, error_code: Option<&str>) -> bool {
    if let Some(code) = error_code {
        if code.to_lowercase().contains("payload_too_large") {
            return true;
        }
    }
    body_lc.contains("request entity too large")
        || body_lc.contains("payload too large")
        || body_lc.contains("request too large")
}

fn is_format_error(body_lc: &str, error_code: Option<&str>) -> bool {
    if let Some(code) = error_code {
        let c = code.to_lowercase();
        if c == "invalid_request_error"
            || c.contains("malformed")
            || c.contains("invalid_schema")
            || c.contains("invalid_json")
        {
            return true;
        }
    }
    body_lc.contains("malformed")
        || body_lc.contains("invalid json")
        || body_lc.contains("invalid request body")
        || body_lc.contains("schema validation failed")
}

fn is_thinking_signature(body_lc: &str, error_code: Option<&str>) -> bool {
    if let Some(code) = error_code {
        if code.to_lowercase().contains("thinking") {
            return true;
        }
    }
    body_lc.contains("thinking_signature")
        || body_lc.contains("thinking block signature")
        || body_lc.contains("redacted_thinking")
}

fn is_long_context_tier(body_lc: &str, error_code: Option<&str>) -> bool {
    if let Some(code) = error_code {
        let c = code.to_lowercase();
        if c.contains("long_context_tier") || c.contains("tier_not_available") {
            return true;
        }
    }
    body_lc.contains("long context tier")
        || body_lc.contains("tier is not available")
        || body_lc.contains("long-context tier")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(status: u16, body: &str) -> ProviderError {
        ProviderError::from_http_response("TestProvider", status, None, body.to_string())
    }

    // ---- All variants reachable ----

    #[test]
    fn variant_rate_limit_from_429() {
        let e = err(429, r#"{"error":{"type":"rate_limit_error"}}"#);
        assert_eq!(FailoverReason::classify(&e), FailoverReason::RateLimit);
    }

    #[test]
    fn variant_overloaded_from_529() {
        let e = err(529, r#"{"error":{"type":"overloaded"}}"#);
        assert_eq!(FailoverReason::classify(&e), FailoverReason::Overloaded);
    }

    #[test]
    fn variant_server_error_from_500() {
        let e = err(500, "internal server error");
        assert_eq!(FailoverReason::classify(&e), FailoverReason::ServerError);
    }

    #[test]
    fn variant_server_error_from_502() {
        let e = err(502, "bad gateway");
        assert_eq!(FailoverReason::classify(&e), FailoverReason::ServerError);
    }

    #[test]
    fn variant_server_error_from_503() {
        let e = err(503, "service unavailable");
        assert_eq!(FailoverReason::classify(&e), FailoverReason::ServerError);
    }

    #[test]
    fn variant_timeout_from_504() {
        let e = err(504, "gateway timeout");
        assert_eq!(FailoverReason::classify(&e), FailoverReason::Timeout);
    }

    #[test]
    fn variant_timeout_from_408() {
        let e = err(408, "request timeout");
        assert_eq!(FailoverReason::classify(&e), FailoverReason::Timeout);
    }

    #[test]
    fn variant_billing_from_402() {
        let e = err(402, r#"{"error":{"code":"credit_balance_too_low"}}"#);
        assert_eq!(FailoverReason::classify(&e), FailoverReason::Billing);
    }

    #[test]
    fn variant_auth_transient_from_401() {
        let e = err(401, r#"{"error":{"type":"authentication_error"}}"#);
        assert_eq!(FailoverReason::classify(&e), FailoverReason::Auth);
    }

    #[test]
    fn variant_auth_permanent_invalid_key() {
        let e = err(403, r#"{"error":{"type":"invalid_api_key"}}"#);
        assert_eq!(FailoverReason::classify(&e), FailoverReason::AuthPermanent);
    }

    #[test]
    fn variant_auth_permanent_account_deactivated() {
        let e = err(
            403,
            "account has been deactivated. Please contact support.",
        );
        assert_eq!(FailoverReason::classify(&e), FailoverReason::AuthPermanent);
    }

    #[test]
    fn variant_context_overflow_from_body() {
        let e = err(400, r#"{"error":{"type":"context_length_exceeded"}}"#);
        assert_eq!(FailoverReason::classify(&e), FailoverReason::ContextOverflow);
    }

    #[test]
    fn variant_payload_too_large_from_413() {
        let e = err(413, "request entity too large");
        assert_eq!(FailoverReason::classify(&e), FailoverReason::PayloadTooLarge);
    }

    #[test]
    fn variant_model_not_found_from_404_body() {
        let e = err(404, r#"{"error":{"code":"model_not_found","message":"The model `gpt-x` does not exist"}}"#);
        assert_eq!(FailoverReason::classify(&e), FailoverReason::ModelNotFound);
    }

    #[test]
    fn variant_format_error_from_invalid_json() {
        let e = err(400, r#"{"error":{"type":"malformed_request","message":"invalid json"}}"#);
        assert_eq!(FailoverReason::classify(&e), FailoverReason::FormatError);
    }

    #[test]
    fn variant_thinking_signature() {
        let e = err(
            400,
            r#"{"error":{"type":"thinking_signature_mismatch","message":"thinking_signature invalid"}}"#,
        );
        assert_eq!(FailoverReason::classify(&e), FailoverReason::ThinkingSignature);
    }

    #[test]
    fn variant_long_context_tier() {
        let e = err(
            400,
            r#"{"error":{"type":"long_context_tier_not_available"}}"#,
        );
        assert_eq!(FailoverReason::classify(&e), FailoverReason::LongContextTier);
    }

    #[test]
    fn variant_unknown_fallback() {
        let e = err(418, "I'm a teapot");
        assert_eq!(FailoverReason::classify(&e), FailoverReason::Unknown);
    }

    // ---- RecoveryActions matrix ----

    #[test]
    fn recovery_rate_limit_retryable_rotate_fallback() {
        let a = FailoverReason::RateLimit.recovery_actions();
        assert!(a.retryable);
        assert!(a.should_rotate_credential);
        assert!(a.should_fallback);
        assert!(!a.should_compress);
    }

    #[test]
    fn recovery_billing_none() {
        let a = FailoverReason::Billing.recovery_actions();
        assert_eq!(a, RecoveryActions::NONE);
    }

    #[test]
    fn recovery_auth_permanent_none() {
        let a = FailoverReason::AuthPermanent.recovery_actions();
        assert_eq!(a, RecoveryActions::NONE);
    }

    #[test]
    fn recovery_auth_transient_rotate_and_fallback() {
        let a = FailoverReason::Auth.recovery_actions();
        assert!(a.retryable);
        assert!(a.should_rotate_credential);
        assert!(a.should_fallback);
        assert!(!a.should_compress);
    }

    #[test]
    fn recovery_context_overflow_compress_no_fallback() {
        let a = FailoverReason::ContextOverflow.recovery_actions();
        assert!(a.retryable);
        assert!(a.should_compress);
        assert!(!a.should_rotate_credential);
        assert!(!a.should_fallback);
    }

    #[test]
    fn recovery_payload_too_large_compress_no_fallback() {
        let a = FailoverReason::PayloadTooLarge.recovery_actions();
        assert!(a.retryable);
        assert!(a.should_compress);
        assert!(!a.should_fallback);
    }

    #[test]
    fn recovery_model_not_found_fallback_only() {
        let a = FailoverReason::ModelNotFound.recovery_actions();
        assert!(!a.retryable);
        assert!(a.should_fallback);
    }

    #[test]
    fn recovery_format_error_no_retry_yes_fallback() {
        let a = FailoverReason::FormatError.recovery_actions();
        assert!(!a.retryable);
        assert!(a.should_fallback);
    }

    #[test]
    fn recovery_overloaded_retry_and_fallback() {
        let a = FailoverReason::Overloaded.recovery_actions();
        assert!(a.retryable);
        assert!(a.should_fallback);
        assert!(!a.should_rotate_credential);
    }

    #[test]
    fn recovery_server_error_retry_and_fallback() {
        let a = FailoverReason::ServerError.recovery_actions();
        assert!(a.retryable);
        assert!(a.should_fallback);
    }

    #[test]
    fn recovery_timeout_retry_and_fallback() {
        let a = FailoverReason::Timeout.recovery_actions();
        assert!(a.retryable);
        assert!(a.should_fallback);
    }

    #[test]
    fn recovery_thinking_signature_retry_same_provider() {
        let a = FailoverReason::ThinkingSignature.recovery_actions();
        assert!(a.retryable);
        assert!(!a.should_fallback);
    }

    #[test]
    fn recovery_long_context_tier_no_retry_fallback() {
        let a = FailoverReason::LongContextTier.recovery_actions();
        assert!(!a.retryable);
        assert!(a.should_compress);
        assert!(a.should_fallback);
    }

    #[test]
    fn recovery_unknown_retry_conservative() {
        let a = FailoverReason::Unknown.recovery_actions();
        assert!(a.retryable);
        assert!(a.should_fallback);
        assert!(!a.should_rotate_credential);
    }

    // ---- Interaction with upstream LlmErrorKind classification ----

    #[test]
    fn body_cue_overrides_http_status_for_thinking() {
        // Upstream would classify 400 body 'thinking_signature' as Unknown,
        // but the classifier lifts it to ThinkingSignature via body cue.
        let e = err(400, "thinking_signature invalid");
        assert_eq!(
            FailoverReason::classify(&e),
            FailoverReason::ThinkingSignature
        );
    }

    #[test]
    fn payload_too_large_before_generic_format_error() {
        // "request too large" must hit PayloadTooLarge, not FormatError.
        let e = err(413, "request too large: body exceeds 32MB limit");
        assert_eq!(
            FailoverReason::classify(&e),
            FailoverReason::PayloadTooLarge
        );
    }

    #[test]
    fn thinking_signature_takes_precedence_over_format_error() {
        // When a body mentions both "malformed" (would hit FormatError) AND
        // "thinking block signature" (ThinkingSignature), the thinking-signature
        // classifier must fire first.
        let e = err(
            400,
            r#"{"error":{"type":"malformed_request","message":"thinking block signature malformed"}}"#,
        );
        assert_eq!(
            FailoverReason::classify(&e),
            FailoverReason::ThinkingSignature
        );
    }

    /// Locks in the full 4-tuple `(retryable, compress, rotate, fallback)` for
    /// every `FailoverReason` variant. If a future edit silently flips a bit,
    /// this test catches it and forces the author to update the table (and
    /// by extension, the callsite expectations in `withRetry`).
    #[test]
    fn recovery_matrix_is_complete_and_stable() {
        use FailoverReason::*;
        let cases = [
            (Auth,              (true,  false, true,  true)),
            (AuthPermanent,     (false, false, false, false)),
            (Billing,           (false, false, false, false)),
            (RateLimit,         (true,  false, true,  true)),
            (Overloaded,        (true,  false, false, true)),
            (ServerError,       (true,  false, false, true)),
            (Timeout,           (true,  false, false, true)),
            (ContextOverflow,   (true,  true,  false, false)),
            (PayloadTooLarge,   (true,  true,  false, false)),
            (ModelNotFound,     (false, false, false, true)),
            (FormatError,       (false, false, false, true)),
            (ThinkingSignature, (true,  false, false, false)),
            (LongContextTier,   (false, true,  false, true)),
            (Unknown,           (true,  false, false, true)),
        ];
        for (reason, (r, c, rot, f)) in cases {
            let a = reason.recovery_actions();
            assert_eq!(
                (
                    a.retryable,
                    a.should_compress,
                    a.should_rotate_credential,
                    a.should_fallback
                ),
                (r, c, rot, f),
                "{:?}",
                reason
            );
        }
    }
}
