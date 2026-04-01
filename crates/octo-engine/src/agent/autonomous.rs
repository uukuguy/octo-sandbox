use octo_types::SessionId;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Default values
fn default_idle_sleep() -> u64 {
    30
}
fn default_active_sleep() -> u64 {
    5
}
fn default_max_rounds() -> u32 {
    100
}
fn default_max_duration() -> u64 {
    3600
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomousConfig {
    pub enabled: bool,
    #[serde(default = "default_idle_sleep")]
    pub idle_sleep_secs: u64,
    #[serde(default = "default_active_sleep")]
    pub active_sleep_secs: u64,
    #[serde(default = "default_max_rounds")]
    pub max_autonomous_rounds: u32,
    #[serde(default = "default_max_duration")]
    pub max_duration_secs: u64,
    pub max_tokens_per_round: Option<u32>,
    pub max_cost_usd: Option<f64>,
    #[serde(default)]
    pub trigger: AutonomousTrigger,
    #[serde(default = "default_true")]
    pub user_presence_aware: bool,
}

impl Default for AutonomousConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            idle_sleep_secs: default_idle_sleep(),
            active_sleep_secs: default_active_sleep(),
            max_autonomous_rounds: default_max_rounds(),
            max_duration_secs: default_max_duration(),
            max_tokens_per_round: None,
            max_cost_usd: None,
            trigger: AutonomousTrigger::default(),
            user_presence_aware: default_true(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AutonomousTrigger {
    #[default]
    Manual,
    Cron {
        expression: String,
    },
    Webhook {
        path: String,
    },
    MessageQueue {
        topic: String,
    },
}

/// Runtime state for autonomous mode (not persisted).
#[derive(Debug, Clone)]
pub struct AutonomousState {
    pub session_id: SessionId,
    pub config: AutonomousConfig,
    pub status: AutonomousStatus,
    pub rounds_completed: u32,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub started_at: Instant,
    pub last_tick_at: Option<Instant>,
    pub user_online: bool,
}

impl AutonomousState {
    pub fn new(session_id: SessionId, config: AutonomousConfig) -> Self {
        Self {
            session_id,
            config,
            status: AutonomousStatus::Running,
            rounds_completed: 0,
            total_tokens: 0,
            total_cost_usd: 0.0,
            started_at: Instant::now(),
            last_tick_at: None,
            user_online: true,
        }
    }

    /// Check if any budget limit has been exceeded.
    pub fn check_budget(&self) -> Option<String> {
        if self.rounds_completed >= self.config.max_autonomous_rounds {
            return Some("max_rounds".into());
        }
        if self.started_at.elapsed().as_secs() >= self.config.max_duration_secs {
            return Some("max_duration".into());
        }
        if let Some(max_cost) = self.config.max_cost_usd {
            if self.total_cost_usd >= max_cost {
                return Some("max_cost".into());
            }
        }
        None
    }

    /// Get the sleep duration based on whether a sleep tool was called.
    pub fn sleep_duration(&self, sleep_tool_secs: Option<u64>) -> u64 {
        sleep_tool_secs.unwrap_or(self.config.idle_sleep_secs)
    }

    /// Get the appropriate sleep duration considering user presence.
    pub fn effective_sleep_duration(&self) -> u64 {
        if self.config.user_presence_aware && self.user_online {
            self.config.active_sleep_secs
        } else {
            self.config.idle_sleep_secs
        }
    }

    /// Record a tick completion.
    pub fn record_tick(&mut self) {
        self.rounds_completed += 1;
        self.last_tick_at = Some(Instant::now());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousStatus {
    Running,
    Sleeping(u64),
    Paused,
    BudgetExhausted,
    RoundsExhausted,
    Completed,
    Failed(String),
}

// Implement Serialize manually because of the String in Failed
impl Serialize for AutonomousStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Running => s.serialize_str("running"),
            Self::Sleeping(secs) => s.serialize_str(&format!("sleeping_{secs}s")),
            Self::Paused => s.serialize_str("paused"),
            Self::BudgetExhausted => s.serialize_str("budget_exhausted"),
            Self::RoundsExhausted => s.serialize_str("rounds_exhausted"),
            Self::Completed => s.serialize_str("completed"),
            Self::Failed(msg) => s.serialize_str(&format!("failed: {msg}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autonomous_config_defaults() {
        let config = AutonomousConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.idle_sleep_secs, 30);
        assert_eq!(config.active_sleep_secs, 5);
        assert_eq!(config.max_autonomous_rounds, 100);
        assert_eq!(config.max_duration_secs, 3600);
        assert!(config.max_tokens_per_round.is_none());
        assert!(config.max_cost_usd.is_none());
        assert!(config.user_presence_aware);
        assert!(matches!(config.trigger, AutonomousTrigger::Manual));
    }

    #[test]
    fn test_autonomous_config_serde_roundtrip() {
        let config = AutonomousConfig {
            enabled: true,
            idle_sleep_secs: 60,
            max_cost_usd: Some(5.0),
            trigger: AutonomousTrigger::Cron {
                expression: "*/5 * * * *".into(),
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: AutonomousConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.idle_sleep_secs, 60);
        assert_eq!(parsed.max_cost_usd, Some(5.0));
        assert!(matches!(parsed.trigger, AutonomousTrigger::Cron { .. }));
    }

    #[test]
    fn test_autonomous_config_serde_defaults() {
        let json = r#"{"enabled": true}"#;
        let config: AutonomousConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.idle_sleep_secs, 30);
        assert_eq!(config.active_sleep_secs, 5);
        assert_eq!(config.max_autonomous_rounds, 100);
        assert_eq!(config.max_duration_secs, 3600);
        assert!(config.user_presence_aware);
    }

    #[test]
    fn test_autonomous_state_new() {
        let config = AutonomousConfig::default();
        let state = AutonomousState::new(SessionId::default(), config);
        assert_eq!(state.rounds_completed, 0);
        assert_eq!(state.total_tokens, 0);
        assert_eq!(state.total_cost_usd, 0.0);
        assert!(state.user_online);
        assert!(state.last_tick_at.is_none());
        assert_eq!(state.status, AutonomousStatus::Running);
    }

    #[test]
    fn test_autonomous_state_check_budget_rounds() {
        let config = AutonomousConfig {
            max_autonomous_rounds: 5,
            ..Default::default()
        };
        let mut state = AutonomousState::new(SessionId::default(), config);
        assert!(state.check_budget().is_none());

        state.rounds_completed = 5;
        assert_eq!(state.check_budget(), Some("max_rounds".into()));
    }

    #[test]
    fn test_autonomous_state_check_budget_cost() {
        let config = AutonomousConfig {
            max_cost_usd: Some(1.0),
            ..Default::default()
        };
        let mut state = AutonomousState::new(SessionId::default(), config);
        assert!(state.check_budget().is_none());

        state.total_cost_usd = 1.5;
        assert_eq!(state.check_budget(), Some("max_cost".into()));
    }

    #[test]
    fn test_autonomous_state_check_budget_duration() {
        let config = AutonomousConfig {
            max_duration_secs: 0, // immediate expiry
            ..Default::default()
        };
        let mut state = AutonomousState::new(SessionId::default(), config);
        // Force started_at to be in the past to avoid race condition on fast machines
        state.started_at = Instant::now() - std::time::Duration::from_secs(1);
        assert_eq!(state.check_budget(), Some("max_duration".into()));
    }

    #[test]
    fn test_autonomous_state_sleep_duration() {
        let config = AutonomousConfig {
            idle_sleep_secs: 30,
            ..Default::default()
        };
        let state = AutonomousState::new(SessionId::default(), config);

        // With explicit sleep tool seconds
        assert_eq!(state.sleep_duration(Some(10)), 10);
        // Without — falls back to idle_sleep_secs
        assert_eq!(state.sleep_duration(None), 30);
    }

    #[test]
    fn test_autonomous_status_serialize() {
        let cases = vec![
            (AutonomousStatus::Running, "\"running\""),
            (AutonomousStatus::Sleeping(30), "\"sleeping_30s\""),
            (AutonomousStatus::Paused, "\"paused\""),
            (AutonomousStatus::BudgetExhausted, "\"budget_exhausted\""),
            (AutonomousStatus::RoundsExhausted, "\"rounds_exhausted\""),
            (AutonomousStatus::Completed, "\"completed\""),
            (
                AutonomousStatus::Failed("timeout".into()),
                "\"failed: timeout\"",
            ),
        ];
        for (status, expected) in cases {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, expected, "failed for {:?}", status);
        }
    }

    #[test]
    fn test_autonomous_state_record_tick() {
        let config = AutonomousConfig::default();
        let mut state = AutonomousState::new(SessionId::default(), config);
        assert_eq!(state.rounds_completed, 0);
        assert!(state.last_tick_at.is_none());

        state.record_tick();
        assert_eq!(state.rounds_completed, 1);
        assert!(state.last_tick_at.is_some());

        state.record_tick();
        assert_eq!(state.rounds_completed, 2);
    }

    #[test]
    fn test_autonomous_state_effective_sleep_duration() {
        let config = AutonomousConfig {
            idle_sleep_secs: 30,
            active_sleep_secs: 5,
            user_presence_aware: true,
            ..Default::default()
        };
        let mut state = AutonomousState::new(SessionId::default(), config);

        // user_online defaults to true → active_sleep
        assert_eq!(state.effective_sleep_duration(), 5);

        state.user_online = false;
        assert_eq!(state.effective_sleep_duration(), 30);
    }

    #[test]
    fn test_autonomous_state_effective_sleep_not_presence_aware() {
        let config = AutonomousConfig {
            idle_sleep_secs: 30,
            active_sleep_secs: 5,
            user_presence_aware: false,
            ..Default::default()
        };
        let state = AutonomousState::new(SessionId::default(), config);
        // Even though user_online is true, presence_aware is false → idle sleep
        assert_eq!(state.effective_sleep_duration(), 30);
    }

    #[test]
    fn test_autonomous_trigger_variants() {
        let triggers = vec![
            AutonomousTrigger::Manual,
            AutonomousTrigger::Cron {
                expression: "0 * * * *".into(),
            },
            AutonomousTrigger::Webhook {
                path: "/hook".into(),
            },
            AutonomousTrigger::MessageQueue {
                topic: "tasks".into(),
            },
        ];
        for trigger in triggers {
            let json = serde_json::to_string(&trigger).unwrap();
            let _parsed: AutonomousTrigger = serde_json::from_str(&json).unwrap();
        }
    }
}
