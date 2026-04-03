//! LSP tool — code intelligence via Language Server Protocol.
//!
//! Aligns with CC-OSS LSPTool: supports go_to_definition, find_references,
//! hover, and document_symbols operations.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use octo_types::{ToolContext, ToolOutput, ToolSource};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::traits::Tool;

/// Maximum file size for LSP operations (10 MB, matching CC-OSS).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// LSP location result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspLocation {
    pub file: String,
    pub line: u32,
    pub character: u32,
}

/// LSP hover info result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspHoverInfo {
    pub contents: String,
    pub range: Option<LspRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspRange {
    pub start_line: u32,
    pub start_character: u32,
    pub end_line: u32,
    pub end_character: u32,
}

/// Document symbol (outline entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LspDocumentSymbol {
    pub name: String,
    pub kind: String,
    pub range: LspRange,
    pub children: Vec<LspDocumentSymbol>,
}

/// Trait for LSP client backends. Allows testing with mocks.
#[async_trait]
pub trait LspClient: Send + Sync {
    async fn go_to_definition(
        &self,
        file: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspLocation>>;

    async fn find_references(
        &self,
        file: &str,
        line: u32,
        character: u32,
    ) -> Result<Vec<LspLocation>>;

    async fn hover(&self, file: &str, line: u32, character: u32) -> Result<Option<LspHoverInfo>>;

    async fn document_symbols(&self, file: &str) -> Result<Vec<LspDocumentSymbol>>;
}

/// Stub LSP client that returns empty results.
/// Real implementation would connect to rust-analyzer, tsserver, etc.
pub struct StubLspClient;

#[async_trait]
impl LspClient for StubLspClient {
    async fn go_to_definition(
        &self,
        _file: &str,
        _line: u32,
        _character: u32,
    ) -> Result<Vec<LspLocation>> {
        Ok(vec![])
    }

    async fn find_references(
        &self,
        _file: &str,
        _line: u32,
        _character: u32,
    ) -> Result<Vec<LspLocation>> {
        Ok(vec![])
    }

    async fn hover(
        &self,
        _file: &str,
        _line: u32,
        _character: u32,
    ) -> Result<Option<LspHoverInfo>> {
        Ok(None)
    }

    async fn document_symbols(&self, _file: &str) -> Result<Vec<LspDocumentSymbol>> {
        Ok(vec![])
    }
}

pub struct LspTool {
    client: Arc<dyn LspClient>,
}

impl LspTool {
    pub fn new(client: Arc<dyn LspClient>) -> Self {
        Self { client }
    }

    pub fn stub() -> Self {
        Self {
            client: Arc::new(StubLspClient),
        }
    }
}

#[async_trait]
impl Tool for LspTool {
    fn name(&self) -> &str {
        "lsp"
    }

    fn description(&self) -> &str {
        "Interact with Language Server Protocol for code intelligence.\n\
         \n\
         ## Operations\n\
         - go_to_definition: Jump to symbol definition\n\
         - find_references: Find all references to a symbol\n\
         - hover: Get type/documentation info at position\n\
         - document_symbols: List all symbols in a file (outline)\n\
         \n\
         ## Parameters\n\
         - operation (required): One of the operations above\n\
         - file_path (required): Absolute or relative path to the file\n\
         - line (required for definition/references/hover): Line number (1-based)\n\
         - character (required for definition/references/hover): Character offset (1-based)\n\
         \n\
         ## File size limit\n\
         Files larger than 10MB are rejected."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["go_to_definition", "find_references", "hover", "document_symbols"],
                    "description": "LSP operation to perform"
                },
                "file_path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (1-based)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character offset (1-based)"
                }
            },
            "required": ["operation", "file_path"]
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let operation = params["operation"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: operation"))?;
        let file_path_str = params["file_path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: file_path"))?;

        // Resolve path
        let file_path = if PathBuf::from(file_path_str).is_absolute() {
            PathBuf::from(file_path_str)
        } else {
            ctx.working_dir.join(file_path_str)
        };

        // Check file size
        if let Ok(metadata) = tokio::fs::metadata(&file_path).await {
            if metadata.len() > MAX_FILE_SIZE {
                return Ok(ToolOutput::error(format!(
                    "File exceeds 10MB limit: {} bytes",
                    metadata.len()
                )));
            }
        }

        let file_str = file_path.to_string_lossy().to_string();

        match operation {
            "go_to_definition" => {
                let (line, character) = parse_position(&params)?;
                let locations = self
                    .client
                    .go_to_definition(&file_str, line - 1, character - 1) // convert to 0-based
                    .await?;
                let result = json!({
                    "operation": "go_to_definition",
                    "file_path": file_str,
                    "results": locations,
                    "result_count": locations.len(),
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            "find_references" => {
                let (line, character) = parse_position(&params)?;
                let locations = self
                    .client
                    .find_references(&file_str, line - 1, character - 1)
                    .await?;
                let result = json!({
                    "operation": "find_references",
                    "file_path": file_str,
                    "results": locations,
                    "result_count": locations.len(),
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            "hover" => {
                let (line, character) = parse_position(&params)?;
                let info = self
                    .client
                    .hover(&file_str, line - 1, character - 1)
                    .await?;
                let result = json!({
                    "operation": "hover",
                    "file_path": file_str,
                    "result": info,
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            "document_symbols" => {
                let symbols = self.client.document_symbols(&file_str).await?;
                let result = json!({
                    "operation": "document_symbols",
                    "file_path": file_str,
                    "symbols": symbols,
                    "symbol_count": symbols.len(),
                });
                Ok(ToolOutput::success(serde_json::to_string_pretty(&result)?))
            }
            _ => Ok(ToolOutput::error(format!(
                "Unknown operation: {}. Use go_to_definition, find_references, hover, or document_symbols.",
                operation
            ))),
        }
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrency_safe(&self) -> bool {
        true
    }

    fn category(&self) -> &str {
        "code_intelligence"
    }
}

fn parse_position(params: &serde_json::Value) -> Result<(u32, u32)> {
    let line = params["line"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: line (1-based)"))?
        as u32;
    let character = params["character"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: character (1-based)"))?
        as u32;
    if line == 0 || character == 0 {
        anyhow::bail!("line and character must be 1-based (minimum 1)");
    }
    Ok((line, character))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ctx() -> ToolContext {
        ToolContext {
            sandbox_id: octo_types::SandboxId::default(),
            user_id: octo_types::UserId::from_string(octo_types::id::DEFAULT_USER_ID),
            working_dir: PathBuf::from("/tmp"),
            path_validator: None,
        }
    }

    /// Mock LSP client with canned responses.
    struct MockLspClient;

    #[async_trait]
    impl LspClient for MockLspClient {
        async fn go_to_definition(
            &self,
            _file: &str,
            _line: u32,
            _character: u32,
        ) -> Result<Vec<LspLocation>> {
            Ok(vec![LspLocation {
                file: "/src/lib.rs".to_string(),
                line: 10,
                character: 5,
            }])
        }

        async fn find_references(
            &self,
            _file: &str,
            _line: u32,
            _character: u32,
        ) -> Result<Vec<LspLocation>> {
            Ok(vec![
                LspLocation {
                    file: "/src/main.rs".to_string(),
                    line: 20,
                    character: 10,
                },
                LspLocation {
                    file: "/src/lib.rs".to_string(),
                    line: 30,
                    character: 15,
                },
            ])
        }

        async fn hover(
            &self,
            _file: &str,
            _line: u32,
            _character: u32,
        ) -> Result<Option<LspHoverInfo>> {
            Ok(Some(LspHoverInfo {
                contents: "fn main()".to_string(),
                range: None,
            }))
        }

        async fn document_symbols(&self, _file: &str) -> Result<Vec<LspDocumentSymbol>> {
            Ok(vec![LspDocumentSymbol {
                name: "main".to_string(),
                kind: "Function".to_string(),
                range: LspRange {
                    start_line: 1,
                    start_character: 0,
                    end_line: 10,
                    end_character: 1,
                },
                children: vec![],
            }])
        }
    }

    fn mock_tool() -> LspTool {
        LspTool::new(Arc::new(MockLspClient))
    }

    #[tokio::test]
    async fn test_lsp_go_to_definition() {
        let tool = mock_tool();
        let result = tool
            .execute(
                json!({"operation": "go_to_definition", "file_path": "/src/main.rs", "line": 5, "character": 10}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("/src/lib.rs"));
        assert!(result.content.contains("\"result_count\": 1"));
    }

    #[tokio::test]
    async fn test_lsp_find_references() {
        let tool = mock_tool();
        let result = tool
            .execute(
                json!({"operation": "find_references", "file_path": "/src/lib.rs", "line": 1, "character": 1}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("\"result_count\": 2"));
    }

    #[tokio::test]
    async fn test_lsp_hover() {
        let tool = mock_tool();
        let result = tool
            .execute(
                json!({"operation": "hover", "file_path": "/src/main.rs", "line": 1, "character": 5}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("fn main()"));
    }

    #[tokio::test]
    async fn test_lsp_document_symbols() {
        let tool = mock_tool();
        let result = tool
            .execute(
                json!({"operation": "document_symbols", "file_path": "/src/main.rs"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.content.contains("main"));
        assert!(result.content.contains("Function"));
        assert!(result.content.contains("\"symbol_count\": 1"));
    }

    #[tokio::test]
    async fn test_lsp_missing_position() {
        let tool = mock_tool();
        let result = tool
            .execute(
                json!({"operation": "go_to_definition", "file_path": "/src/main.rs"}),
                &test_ctx(),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_lsp_unknown_operation() {
        let tool = mock_tool();
        let result = tool
            .execute(
                json!({"operation": "unknown_op", "file_path": "/src/main.rs"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Unknown operation"));
    }

    #[tokio::test]
    async fn test_lsp_zero_based_position_rejected() {
        let tool = mock_tool();
        let result = tool
            .execute(
                json!({"operation": "hover", "file_path": "/src/main.rs", "line": 0, "character": 1}),
                &test_ctx(),
            )
            .await;
        assert!(result.is_err());
    }
}
