pub mod anthropic;
pub mod openai;
pub mod traits;

pub use anthropic::create_provider as create_anthropic_provider;
pub use openai::create_openai_provider;
pub use traits::{CompletionStream, Provider};

/// Create a provider by name.
///
/// Supported providers: "anthropic", "openai".
/// Falls back to Anthropic if the name is unrecognized.
pub fn create_provider(
    provider_name: &str,
    api_key: String,
    base_url: Option<String>,
) -> Box<dyn Provider> {
    match provider_name {
        "openai" => create_openai_provider(api_key, base_url),
        _ => create_anthropic_provider(api_key, base_url),
    }
}
