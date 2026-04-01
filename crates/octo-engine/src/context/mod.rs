pub mod auto_compact;
pub mod budget;
pub mod builder;
pub mod collapse;
pub mod compact_prompt;
pub mod compaction_pipeline;
pub mod flush;
pub mod fork;
pub mod manager;
pub mod observation_masker;
pub mod pruner;
pub mod system_prompt; // Zone A: SystemPromptBuilder
pub mod token_counter;
pub mod tool_use_summary;

pub use auto_compact::{AutoCompactConfig, AutoCompactSummary};
pub use budget::{ContextBudgetManager, DegradationLevel};
pub use collapse::ContextCollapser;
pub use builder::{estimate_messages_tokens, BootstrapFile, ContextBuilder, SystemPromptBuilder};
pub use compaction_pipeline::{
    CompactionContext, CompactionPipeline, CompactionPipelineConfig, CompactionResult,
    SNIP_MARKER,
};
pub use flush::MemoryFlusher;
pub use fork::ContextFork;
pub use manager::{ContextBudgetSnapshot, ContextManager, EstimateCounter, TokenCounter};
pub use observation_masker::{ObservationMaskConfig, ObservationMasker};
pub use pruner::{
    CompactionAction, CompactionConfig, CompactionStrategy, ContextPruner, SKILL_PROTECTED_MARKER,
};
pub use system_prompt::{PromptParts, SystemPromptBuilder as NewSystemPromptBuilder};
pub use token_counter::CjkAwareCounter;

#[cfg(feature = "tiktoken")]
pub mod tiktoken_counter;
#[cfg(feature = "tiktoken")]
pub use tiktoken_counter::TiktokenCounter;

/// Create the best available token counter.
///
/// Returns `TiktokenCounter` when the `tiktoken` feature is enabled,
/// otherwise falls back to the lightweight `EstimateCounter`.
pub fn default_token_counter() -> Box<dyn TokenCounter> {
    #[cfg(feature = "tiktoken")]
    {
        Box::new(TiktokenCounter::new())
    }
    #[cfg(not(feature = "tiktoken"))]
    {
        Box::new(EstimateCounter)
    }
}
