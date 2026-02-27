use std::collections::{HashMap, VecDeque};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Loop Guard 触发原因
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopGuardViolation {
    /// 同一工具调用（name+params）重复 ≥ 5 次
    RepetitiveCall { tool_name: String, count: u32 },
    /// 乒乓模式：A-B-A 或 A-B-A-B 检测到
    PingPong { pattern: String },
    /// 全局断路器：总调用次数 ≥ 30
    CircuitBreaker { total_calls: u64 },
}

impl std::fmt::Display for LoopGuardViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RepetitiveCall { tool_name, count } =>
                write!(f, "repetitive tool call '{}' ({} times)", tool_name, count),
            Self::PingPong { pattern } =>
                write!(f, "ping-pong loop detected: {}", pattern),
            Self::CircuitBreaker { total_calls } =>
                write!(f, "circuit breaker triggered after {} total calls", total_calls),
        }
    }
}

pub struct LoopGuard {
    /// tool_name + params_hash → (name, count)
    call_counts: HashMap<u64, (String, u32)>,
    /// 最近 6 次工具调用名称（滑动窗口）
    recent_calls: VecDeque<String>,
    /// 全局调用计数器（原子，无锁）
    total_calls: Arc<AtomicU64>,
    /// 重复阈值（默认 5）
    repetition_threshold: u32,
    /// 全局断路器阈值（默认 30）
    circuit_breaker_threshold: u64,
}

impl LoopGuard {
    pub fn new() -> Self {
        Self {
            call_counts: HashMap::new(),
            recent_calls: VecDeque::with_capacity(6),
            total_calls: Arc::new(AtomicU64::new(0)),
            repetition_threshold: 5,
            circuit_breaker_threshold: 30,
        }
    }

    /// 记录一次工具调用，返回是否触发违规
    pub fn record_call(&mut self, tool_name: &str, params_json: &str) -> Option<LoopGuardViolation> {
        let total = self.total_calls.fetch_add(1, Ordering::Relaxed) + 1;

        // 1. 全局断路器检查
        if total >= self.circuit_breaker_threshold {
            return Some(LoopGuardViolation::CircuitBreaker { total_calls: total });
        }

        // 2. 重复调用检测
        let key = Self::hash_call(tool_name, params_json);
        let entry = self.call_counts.entry(key).or_insert((tool_name.to_string(), 0));
        entry.1 += 1;
        if entry.1 >= self.repetition_threshold {
            return Some(LoopGuardViolation::RepetitiveCall {
                tool_name: tool_name.to_string(),
                count: entry.1,
            });
        }

        // 3. 乒乓检测（滑动窗口 6 次）
        self.recent_calls.push_back(tool_name.to_string());
        if self.recent_calls.len() > 6 {
            self.recent_calls.pop_front();
        }
        if let Some(violation) = self.detect_ping_pong() {
            return Some(violation);
        }

        None
    }

    fn hash_call(tool_name: &str, params_json: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        tool_name.hash(&mut hasher);
        params_json.hash(&mut hasher);
        hasher.finish()
    }

    fn detect_ping_pong(&self) -> Option<LoopGuardViolation> {
        let calls: Vec<&str> = self.recent_calls.iter().map(|s| s.as_str()).collect();
        let len = calls.len();
        if len < 4 {
            return None;
        }
        // 检测 A-B-A-B 模式（长度 4）
        if len >= 4
            && calls[len - 4] == calls[len - 2]
            && calls[len - 3] == calls[len - 1]
            && calls[len - 4] != calls[len - 3]
        {
            let pattern = format!(
                "{}-{}-{}-{}",
                calls[len - 4],
                calls[len - 3],
                calls[len - 2],
                calls[len - 1]
            );
            return Some(LoopGuardViolation::PingPong { pattern });
        }
        // 检测 A-B-A 模式连续出现 2 次（长度 6）
        if len >= 6
            && calls[len - 6] == calls[len - 4]
            && calls[len - 4] == calls[len - 2]
            && calls[len - 5] == calls[len - 3]
        {
            let pattern = format!("{}-{}-{} (x2)", calls[len - 4], calls[len - 3], calls[len - 2]);
            return Some(LoopGuardViolation::PingPong { pattern });
        }
        None
    }

    pub fn total_calls(&self) -> u64 {
        self.total_calls.load(Ordering::Relaxed)
    }
}

impl Default for LoopGuard {
    fn default() -> Self {
        Self::new()
    }
}
