pub mod budget;
pub mod builder;
pub mod flush;
pub mod pruner;
pub mod system_prompt; // Zone A: SystemPromptBuilder

pub use budget::{ContextBudgetManager, DegradationLevel};
pub use builder::{estimate_messages_tokens, BootstrapFile, ContextBuilder, SystemPromptBuilder};
pub use flush::MemoryFlusher;
pub use pruner::ContextPruner;
pub use system_prompt::SystemPromptBuilder as NewSystemPromptBuilder;
