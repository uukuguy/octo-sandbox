pub mod channel;
pub mod context;
pub mod handle;
pub mod injection;
pub mod manager;
pub mod persistence;
pub mod protocol;

pub use channel::{create_channel_pair, CollaborationChannel, CollaborationMessage};
pub use context::*;
pub use handle::CollaborationHandle;
pub use injection::build_collaboration_injection;
pub use manager::{CollaborationAgent, CollaborationManager};
pub use persistence::{CollaborationSnapshot, CollaborationStore, InMemoryCollaborationStore};
pub use protocol::CollaborationProtocol;
