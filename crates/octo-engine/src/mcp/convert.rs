use super::traits::{
    McpPromptArgument, McpPromptInfo, McpPromptMessage, McpPromptResult, McpResourceContent,
    McpResourceInfo,
};

/// Map rmcp Resource list to internal McpResourceInfo list.
pub fn map_resources(resources: Vec<rmcp::model::Resource>) -> Vec<McpResourceInfo> {
    resources
        .into_iter()
        .map(|r| McpResourceInfo {
            uri: r.uri.clone(),
            name: r.name.clone(),
            description: r.description.clone(),
            mime_type: r.mime_type.clone(),
        })
        .collect()
}

/// Map the first entry of an rmcp ResourceContents list to McpResourceContent.
///
/// `fallback_uri` is used when the server returns an empty contents list.
pub fn map_resource_content(
    contents: Vec<rmcp::model::ResourceContents>,
    fallback_uri: &str,
) -> McpResourceContent {
    match contents.into_iter().next() {
        Some(rmcp::model::ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        }) => McpResourceContent {
            uri,
            mime_type,
            text: Some(text),
            blob: None,
        },
        Some(rmcp::model::ResourceContents::BlobResourceContents {
            uri,
            mime_type,
            blob,
            ..
        }) => McpResourceContent {
            uri,
            mime_type,
            text: None,
            blob: Some(blob),
        },
        None => McpResourceContent {
            uri: fallback_uri.to_string(),
            mime_type: None,
            text: None,
            blob: None,
        },
    }
}

/// Map rmcp Prompt list to internal McpPromptInfo list.
pub fn map_prompts(prompts: Vec<rmcp::model::Prompt>) -> Vec<McpPromptInfo> {
    prompts
        .into_iter()
        .map(|p| McpPromptInfo {
            name: p.name.clone(),
            description: p.description.clone(),
            arguments: p
                .arguments
                .unwrap_or_default()
                .into_iter()
                .map(|a| McpPromptArgument {
                    name: a.name,
                    description: a.description,
                    required: a.required.unwrap_or(false),
                })
                .collect(),
        })
        .collect()
}

/// Map rmcp PromptMessage list to internal McpPromptMessage list.
pub fn map_prompt_messages(messages: Vec<rmcp::model::PromptMessage>) -> Vec<McpPromptMessage> {
    messages
        .into_iter()
        .map(|m| {
            let role = match m.role {
                rmcp::model::PromptMessageRole::User => "user".to_string(),
                rmcp::model::PromptMessageRole::Assistant => "assistant".to_string(),
            };
            let content = match m.content {
                rmcp::model::PromptMessageContent::Text { text } => text,
                other => serde_json::to_string(&other).unwrap_or_default(),
            };
            McpPromptMessage { role, content }
        })
        .collect()
}

/// Build the McpPromptResult from an rmcp GetPromptResult.
pub fn map_prompt_result(result: rmcp::model::GetPromptResult) -> McpPromptResult {
    McpPromptResult {
        description: result.description,
        messages: map_prompt_messages(result.messages),
    }
}
