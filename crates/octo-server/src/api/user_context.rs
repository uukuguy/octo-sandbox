use octo_engine::auth::UserContext;

const DEFAULT_USER_ID: &str = "default";

/// Get user_id from UserContext or use default
pub fn get_user_id_from_context(ctx: Option<&UserContext>) -> String {
    ctx.and_then(|c| c.user_id.clone())
        .unwrap_or_else(|| DEFAULT_USER_ID.to_string())
}
