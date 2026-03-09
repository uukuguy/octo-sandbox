pub mod auto_compact;
pub mod budget;
pub mod builder;
pub mod flush;
pub mod fork;
pub mod manager;
pub mod observation_masker;
pub mod pruner;
pub mod system_prompt; // Zone A: SystemPromptBuilder
pub mod token_counter;

pub use auto_compact::{AutoCompactConfig, AutoCompactSummary};
pub use budget::{ContextBudgetManager, DegradationLevel};
pub use builder::{estimate_messages_tokens, BootstrapFile, ContextBuilder, SystemPromptBuilder};
pub use flush::MemoryFlusher;
pub use fork::ContextFork;
pub use manager::{ContextBudgetSnapshot, ContextManager, EstimateCounter, TokenCounter};
pub use observation_masker::{ObservationMaskConfig, ObservationMasker};
pub use pruner::ContextPruner;
pub use system_prompt::SystemPromptBuilder as NewSystemPromptBuilder;
pub use token_counter::CjkAwareCounter;

#[cfg(feature = "tiktoken")]
pub mod tiktoken_counter;
#[cfg(feature = "tiktoken")]
pub use tiktoken_counter::TiktokenCounter;
