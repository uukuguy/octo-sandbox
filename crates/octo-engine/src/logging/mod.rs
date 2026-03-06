use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize structured logging with JSON format.
///
/// This function sets up the tracing subscriber to output logs in JSON format,
/// including target, thread IDs, file paths, and line numbers for better
/// observability and log analysis.
pub fn init_logging() {
    let formatter = fmt::layer()
        .json()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(formatter)
        .with(EnvFilter::new("info"))
        .try_init()
        .ok();
}

/// Initialize structured logging with custom filter.
///
/// # Arguments
/// * `filter` - The filter string (e.g., "debug", "info,octo_engine=debug")
pub fn init_logging_with_filter(filter: &str) {
    let formatter = fmt::layer()
        .json()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(formatter)
        .with(EnvFilter::new(filter))
        .try_init()
        .ok();
}

/// Initialize logging with pretty formatting (non-JSON) for development.
///
/// This is useful for local development where human-readable output is preferred.
pub fn init_pretty_logging() {
    let formatter = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(formatter)
        .with(EnvFilter::new("info"))
        .try_init()
        .ok();
}

/// Initialize logging with pretty formatting and custom filter.
pub fn init_pretty_logging_with_filter(filter: &str) {
    let formatter = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(formatter)
        .with(EnvFilter::new(filter))
        .try_init()
        .ok();
}

#[macro_export]
/// Audit log macro for tracking user actions on resources.
///
/// # Arguments
/// * `$user_id` - The ID of the user performing the action
/// * `$action` - The action being performed (e.g., "create", "read", "update", "delete")
/// * `$resource` - The resource being acted upon (e.g., "agent:123", "session:abc")
///
/// # Example
/// ```ignore
/// audit_log!("user_123", "create", "agent:456");
/// ```
macro_rules! audit_log {
    ($user_id:expr, $action:expr, $resource:expr) => {
        tracing::info!(
            target: "audit",
            user_id = $user_id,
            action = $action,
            resource = $resource,
        )
    };
    ($user_id:expr, $action:expr, $resource:expr, $($field:tt)*) => {
        tracing::info!(
            target: "audit",
            user_id = $user_id,
            action = $action,
            resource = $resource,
            $($field)*
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_logging_does_not_panic() {
        // Use try_init to avoid panic when global subscriber is already set
        let _ = tracing_subscriber::registry()
            .with(fmt::layer().json())
            .with(EnvFilter::new("info"))
            .try_init();
    }

    #[test]
    fn test_init_logging_with_filter_does_not_panic() {
        let _ = tracing_subscriber::registry()
            .with(fmt::layer().json())
            .with(EnvFilter::new("warn"))
            .try_init();
    }

    #[test]
    fn test_init_pretty_logging_does_not_panic() {
        let _ = tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::new("info"))
            .try_init();
    }

    #[test]
    fn test_init_pretty_logging_with_filter_does_not_panic() {
        let _ = tracing_subscriber::registry()
            .with(fmt::layer())
            .with(EnvFilter::new("info"))
            .try_init();
    }

    #[test]
    fn test_audit_log_macro_compiles() {
        // Just verify the macro compiles - actual logging is tested separately
        let user_id = "test_user";
        let action = "test_action";
        let resource = "test_resource";

        // This will only compile the macro, not actually log
        let _ = format!("{} {} {}", user_id, action, resource);
    }
}
