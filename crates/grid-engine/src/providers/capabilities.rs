//! Provider × Model capability matrix.
//!
//! Tracks what each (provider, model, base_url) combination supports, so the
//! agent loop can decide **up front** whether to use features like
//! `tool_choice` rather than discovering incompatibilities at runtime.
//!
//! Design per 2026-04-14 decision (see
//! `docs/design/EAASP/PROVIDER_CAPABILITY_MATRIX.md`):
//!
//!   1. **Static baseline** — well-known (provider, model) combos are
//!      hard-coded as Supported / Unsupported.
//!   2. **Unknown → probe** — an unknown combo is left as `Unknown` until
//!      a startup probe (a cheap real API request) determines the answer.
//!   3. **Cache probe results** — probe outcomes are cached by
//!      (provider, model, base_url) for the process lifetime so we don't
//!      repeat them on every session.
//!   4. **Loud failure** — if a capability is marked `Supported` but a
//!      live request 400s, we report an error (do not silently fallback);
//!      if `Unsupported`, we never arm the feature in the first place.
//!
//! The goal is that D87's `tool_choice=Required` mechanism only ever runs
//! against providers we *know* can honor it.

use std::collections::HashMap;
use std::sync::RwLock;

use grid_types::{
    ChatMessage, CompletionRequest, MessageRole, ContentBlock, ToolChoice, ToolSpec,
};

use super::traits::Provider;

/// Whether a specific capability (e.g. `tool_choice`) is known to work for
/// a given (provider, model, base_url) combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// Confirmed supported (static table or successful probe).
    Supported,
    /// Confirmed not supported (static table or failed probe).
    Unsupported,
    /// Haven't checked yet. Probe on first use, then update this entry.
    Unknown,
}

/// Key for the capability cache. `base_url` matters because OpenRouter
/// routes the same model to different backends over time — but we cache
/// by URL which at least distinguishes "OpenAI direct" vs "OpenRouter".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CapabilityKey {
    pub provider: String,
    pub model: String,
    pub base_url: String,
}

impl CapabilityKey {
    pub fn new(provider: &str, model: &str, base_url: &str) -> Self {
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            base_url: base_url.to_string(),
        }
    }
}

/// The set of capabilities we track per (provider, model, base_url).
/// Grow this struct over time as new provider-divergent features appear.
#[derive(Debug, Clone, Copy)]
pub struct CapabilitySet {
    /// Supports OpenAI-style `tool_choice: "required" | "none" | ...` field
    /// (or Anthropic-style `tool_choice: {type: "any" | ...}`).
    pub tool_choice: Capability,
}

impl Default for CapabilitySet {
    fn default() -> Self {
        Self {
            tool_choice: Capability::Unknown,
        }
    }
}

/// Static baseline for well-known (provider, model) combinations. The
/// `base_url` is matched as a substring so all Anthropic direct endpoints
/// share one entry, OpenRouter is its own entry, etc.
///
/// Entries here avoid the need to probe on startup. If you add a new
/// provider or upgrade a model family, extend this table rather than
/// relying on probe to catch up.
fn static_baseline(key: &CapabilityKey) -> CapabilitySet {
    // OpenAI direct — the canonical tool_choice contract.
    if key.provider == "openai"
        && (key.base_url.contains("api.openai.com") || key.base_url.is_empty())
        && (key.model.starts_with("gpt-4") || key.model.starts_with("gpt-3.5-turbo"))
    {
        return CapabilitySet {
            tool_choice: Capability::Supported,
        };
    }

    // Anthropic direct — native tool_choice support since claude-3.
    if key.provider == "anthropic"
        && (key.base_url.contains("api.anthropic.com") || key.base_url.is_empty())
        && (key.model.starts_with("claude-3")
            || key.model.starts_with("claude-sonnet")
            || key.model.starts_with("claude-opus")
            || key.model.starts_with("claude-haiku"))
    {
        return CapabilitySet {
            tool_choice: Capability::Supported,
        };
    }

    // OpenRouter — capability depends on the dynamically routed backend.
    // 2026-04-14 empirical results inform a small per-model overlay so we
    // can skip the probe round-trip for known-good and known-bad combos.
    // Anything not matched here falls through to `Unknown` and gets
    // probed at session start.
    if key.base_url.contains("openrouter.ai") {
        let m = key.model.to_lowercase();

        // Known to reject `tool_choice` (small Qwen variants on AtlasCloud).
        // Skipping the probe avoids one wasted LLM call per startup.
        let unsupported = m.contains("qwen") && (m.contains("122b") || m.contains("27b"));
        if unsupported {
            return CapabilitySet {
                tool_choice: Capability::Unsupported,
            };
        }

        // Known to honor `tool_choice` (verified end-to-end 2026-04-14).
        let supported = m.starts_with("openai/")
            || m.starts_with("anthropic/")
            || m.contains("glm-4")
            || (m.contains("qwen") && m.contains("397b"));
        if supported {
            return CapabilitySet {
                tool_choice: Capability::Supported,
            };
        }

        return CapabilitySet {
            tool_choice: Capability::Unknown,
        };
    }

    // Everything else (vLLM, self-hosted, unknown gateways) — probe.
    CapabilitySet::default()
}

/// Process-global capability store.
///
/// Holds the static baseline augmented with probe results. Access is
/// read-mostly so we use `RwLock<HashMap<..>>`.
pub struct CapabilityStore {
    cache: RwLock<HashMap<CapabilityKey, CapabilitySet>>,
}

impl CapabilityStore {
    pub fn new() -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Look up the current capability set for a (provider, model, base_url)
    /// combination. Reads from cache; falls back to static baseline if not
    /// cached yet. Does **not** mutate.
    pub fn get(&self, key: &CapabilityKey) -> CapabilitySet {
        if let Ok(cache) = self.cache.read() {
            if let Some(cached) = cache.get(key) {
                return *cached;
            }
        }
        static_baseline(key)
    }

    /// Record a probe outcome. Later `get()` calls will return the cached
    /// result instead of re-running the static baseline logic.
    pub fn record(&self, key: CapabilityKey, caps: CapabilitySet) {
        if let Ok(mut cache) = self.cache.write() {
            cache.insert(key, caps);
        }
    }
}

impl Default for CapabilityStore {
    fn default() -> Self {
        Self::new()
    }
}

/// When to run capability probes.
///
/// Per 2026-04-14 decision: default is **Eager** — the runtime must know,
/// at startup, which capabilities work. Lazy/PerSession are opt-in knobs
/// for environments where eager probing is impractical (offline boot,
/// multi-tenant per-session routing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeStrategy {
    /// Probe at grid-runtime startup; fail boot if provider unreachable.
    Eager,
    /// Probe on first use inside a session; one-time per key.
    Lazy,
    /// Probe at every `initialize()`.
    PerSession,
}

impl Default for ProbeStrategy {
    fn default() -> Self {
        Self::Eager
    }
}

/// Outcome of probing a single capability for a single (provider, model, url).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// HTTP 200 — the feature is honored.
    Supported,
    /// HTTP 400 or analogous rejection — feature not honored.
    Unsupported,
    /// Provider unreachable / transport error — can't decide. Treated as
    /// `Unsupported` by the harness (conservative: don't arm the feature).
    /// For `ProbeStrategy::Eager`, grid-runtime startup should fail instead.
    Inconclusive,
}

impl From<ProbeOutcome> for Capability {
    fn from(o: ProbeOutcome) -> Self {
        match o {
            ProbeOutcome::Supported => Capability::Supported,
            ProbeOutcome::Unsupported => Capability::Unsupported,
            ProbeOutcome::Inconclusive => Capability::Unsupported,
        }
    }
}

/// Issue a minimal `stream` request with `tool_choice=Required` against a
/// trivial stub tool. The response body is irrelevant — we only look at
/// whether the HTTP call succeeded (200 → supported) or was rejected
/// (400 → unsupported).
///
/// Max tokens are capped at 8 to keep the probe cheap.
pub async fn probe_tool_choice(provider: &dyn Provider, model: &str) -> ProbeOutcome {
    let req = CompletionRequest {
        model: model.to_string(),
        system: None,
        messages: vec![ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: "ping".into(),
            }],
        }],
        max_tokens: 8,
        temperature: Some(0.0),
        tools: vec![ToolSpec {
            name: "ping".into(),
            description: "Capability probe tool — never actually invoked.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
            }),
        }],
        stream: true,
        tool_choice: Some(ToolChoice::Required),
    };

    match provider.stream(req).await {
        Ok(_stream) => {
            // We don't need to consume the stream — receiving 200 and
            // getting a stream handle is enough. The stream drops here.
            ProbeOutcome::Supported
        }
        Err(e) => {
            let msg = e.to_string();
            // Treat HTTP 400 specifically as Unsupported. Other errors
            // (network, 401 auth, 5xx) mean we couldn't determine the
            // capability — report Inconclusive so the caller decides.
            if msg.contains("400") || msg.to_lowercase().contains("invalid parameter") {
                tracing::warn!(
                    error = %msg,
                    model = %model,
                    provider = %provider.id(),
                    "tool_choice probe: 400 — treating as Unsupported"
                );
                ProbeOutcome::Unsupported
            } else {
                tracing::error!(
                    error = %msg,
                    model = %model,
                    provider = %provider.id(),
                    "tool_choice probe: inconclusive (non-400 error)"
                );
                ProbeOutcome::Inconclusive
            }
        }
    }
}

impl CapabilityStore {
    /// Probe and cache the `tool_choice` capability for a
    /// (provider, model, base_url) tuple. Skips the probe if a definitive
    /// answer is already cached (`Supported` or `Unsupported`).
    ///
    /// Returns the final recorded capability.
    pub async fn ensure_tool_choice(
        &self,
        key: CapabilityKey,
        provider: &dyn Provider,
    ) -> Capability {
        let existing = self.get(&key).tool_choice;
        if existing != Capability::Unknown {
            return existing;
        }
        let outcome = probe_tool_choice(provider, &key.model).await;
        let cap = Capability::from(outcome);
        self.record(
            key,
            CapabilitySet {
                tool_choice: cap,
            },
        );
        cap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_gpt4_is_supported_by_default() {
        let key = CapabilityKey::new("openai", "gpt-4o", "https://api.openai.com/v1");
        let caps = static_baseline(&key);
        assert_eq!(caps.tool_choice, Capability::Supported);
    }

    #[test]
    fn anthropic_claude_sonnet_is_supported_by_default() {
        let key = CapabilityKey::new(
            "anthropic",
            "claude-sonnet-4-20250514",
            "https://api.anthropic.com",
        );
        let caps = static_baseline(&key);
        assert_eq!(caps.tool_choice, Capability::Supported);
    }

    #[test]
    fn openrouter_qwen_122b_is_unsupported_by_baseline() {
        // 2026-04-14 empirical: AtlasCloud rejects tool_choice for this model.
        let key = CapabilityKey::new(
            "openai",
            "qwen/qwen3.5-122b-a10b",
            "https://openrouter.ai/api/v1",
        );
        assert_eq!(static_baseline(&key).tool_choice, Capability::Unsupported);
    }

    #[test]
    fn openrouter_qwen_27b_is_unsupported_by_baseline() {
        let key = CapabilityKey::new(
            "openai",
            "qwen/qwen3.5-27b",
            "https://openrouter.ai/api/v1",
        );
        assert_eq!(static_baseline(&key).tool_choice, Capability::Unsupported);
    }

    #[test]
    fn openrouter_qwen_397b_is_supported_by_baseline() {
        let key = CapabilityKey::new(
            "openai",
            "qwen/qwen3.5-397b-a17b",
            "https://openrouter.ai/api/v1",
        );
        assert_eq!(static_baseline(&key).tool_choice, Capability::Supported);
    }

    #[test]
    fn openrouter_glm4_is_supported_by_baseline() {
        let key = CapabilityKey::new(
            "openai",
            "z-ai/glm-4.7-flash",
            "https://openrouter.ai/api/v1",
        );
        assert_eq!(static_baseline(&key).tool_choice, Capability::Supported);
    }

    #[test]
    fn openrouter_openai_passthrough_is_supported_by_baseline() {
        let key = CapabilityKey::new(
            "openai",
            "openai/gpt-4o",
            "https://openrouter.ai/api/v1",
        );
        assert_eq!(static_baseline(&key).tool_choice, Capability::Supported);
    }

    #[test]
    fn openrouter_unknown_model_remains_unknown() {
        let key = CapabilityKey::new(
            "openai",
            "some-vendor/exotic-model-9000",
            "https://openrouter.ai/api/v1",
        );
        assert_eq!(static_baseline(&key).tool_choice, Capability::Unknown);
    }

    #[test]
    fn unknown_provider_defaults_to_unknown() {
        let key = CapabilityKey::new("vllm", "llama-70b", "http://localhost:8000");
        let caps = static_baseline(&key);
        assert_eq!(caps.tool_choice, Capability::Unknown);
    }

    #[test]
    fn store_returns_recorded_result_over_baseline() {
        let store = CapabilityStore::new();
        let key = CapabilityKey::new(
            "openai",
            "qwen/qwen3.5-122b-a10b",
            "https://openrouter.ai/api/v1",
        );
        // Baseline says Unknown; probe determined Unsupported.
        store.record(
            key.clone(),
            CapabilitySet {
                tool_choice: Capability::Unsupported,
            },
        );
        assert_eq!(store.get(&key).tool_choice, Capability::Unsupported);
    }
}
