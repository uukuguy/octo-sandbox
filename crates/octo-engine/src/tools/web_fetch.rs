use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{RiskLevel, ToolContext, ToolOutput, ToolSource};

use super::traits::Tool;

/// Tool for fetching web content from a URL
pub struct WebFetchTool {
    client: reqwest::Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        super::prompts::WEB_FETCH_DESCRIPTION
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch content from"
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum number of characters to return (default: 50000)"
                },
                "extract_content": {
                    "type": "boolean",
                    "description": "Extract readable text from HTML, removing scripts/styles/nav (default: true). Set to false for raw HTML."
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput> {
        let url = params["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'url' parameter"))?;

        let max_length = params["max_length"].as_u64().map(|v| v as usize);

        // SSRF protection: validate URL scheme and destination
        let parsed_url =
            reqwest::Url::parse(url).map_err(|e| anyhow::anyhow!("Invalid URL: {}", e))?;

        // Block non-HTTP schemes
        match parsed_url.scheme() {
            "http" | "https" => {}
            scheme => {
                return Ok(ToolOutput::error(format!(
                    "Blocked URL scheme '{}'. Only http and https are allowed.",
                    scheme
                )));
            }
        }

        // Block private/loopback/link-local IPs and cloud metadata endpoints
        if let Some(host) = parsed_url.host_str() {
            let is_loopback =
                host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "0.0.0.0";

            let is_private_10 = host.starts_with("10.");
            let is_private_192 = host.starts_with("192.168.");
            let is_link_local = host.starts_with("169.254.");

            // Check 172.16.0.0/12 range
            let is_private_172 = if let Some(rest) = host.strip_prefix("172.") {
                rest.split('.')
                    .next()
                    .and_then(|s| s.parse::<u8>().ok())
                    .map(|n| (16..=31).contains(&n))
                    .unwrap_or(false)
            } else {
                false
            };

            // Cloud metadata endpoints
            let is_metadata = host == "metadata.google.internal" || host == "169.254.169.254";

            if is_loopback
                || is_private_10
                || is_private_192
                || is_link_local
                || is_private_172
                || is_metadata
            {
                return Ok(ToolOutput::error(format!(
                    "Blocked request to private/internal address: {}",
                    host
                )));
            }
        }

        debug!(url, max_length, "fetching web content");

        let response = self
            .client
            .get(url)
            .header("User-Agent", "Octo-Sandbox/1.0")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("failed to fetch URL: {e}"))?;

        if !response.status().is_success() {
            return Ok(ToolOutput::error(format!(
                "HTTP error: {} - {}",
                response.status().as_u16(),
                response
                    .status()
                    .canonical_reason()
                    .unwrap_or("Unknown error")
            )));
        }

        let raw_content = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("failed to read response body: {e}"))?;

        let extract = params["extract_content"].as_bool().unwrap_or(true);

        let mut content = if extract && is_html_content(&raw_content) {
            extract_readable_text(&raw_content)
        } else {
            raw_content
        };

        // Truncate if necessary
        let max = max_length.unwrap_or(50000);
        if content.len() > max {
            content.truncate(max);
            content.push_str("\n... (content truncated)");
        }

        Ok(ToolOutput::success(content))
    }

    fn source(&self) -> ToolSource {
        ToolSource::BuiltIn
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

/// Check if content looks like HTML
fn is_html_content(content: &str) -> bool {
    let trimmed = content.trim_start();
    trimmed.starts_with("<!") || trimmed.starts_with("<html") || trimmed.starts_with("<HTML")
        || trimmed.contains("<head") || trimmed.contains("<body")
}

/// Extract readable text from HTML by removing non-content elements
fn extract_readable_text(html: &str) -> String {
    let mut result = html.to_string();

    // Remove script, style, nav, footer, header, aside tags and their content (case-insensitive)
    for tag in &["script", "style", "nav", "footer", "header", "aside", "noscript", "svg"] {
        // Use a simple iterative approach to remove matched tag pairs
        loop {
            let open_tag = format!("<{}", tag);
            let close_tag = format!("</{}>", tag);
            let lower = result.to_lowercase();
            if let Some(start) = lower.find(&open_tag) {
                if let Some(end_pos) = lower[start..].find(&close_tag) {
                    let end = start + end_pos + close_tag.len();
                    result.replace_range(start..end, "");
                    continue;
                } else {
                    // No closing tag — remove from open tag to end of its > bracket
                    if let Some(bracket) = result[start..].find('>') {
                        result.replace_range(start..start + bracket + 1, "");
                        continue;
                    }
                }
            }
            break;
        }
    }

    // Remove all remaining HTML tags but keep their text content
    let mut out = String::with_capacity(result.len());
    let mut in_tag = false;
    let mut last_was_block = false;
    let chars: Vec<char> = result.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '<' {
            in_tag = true;
            // Check if it's a block-level element for line break insertion
            let rest: String = chars[i..].iter().take(10).collect();
            let rest_lower = rest.to_lowercase();
            if rest_lower.starts_with("<p") || rest_lower.starts_with("<div")
                || rest_lower.starts_with("<h1") || rest_lower.starts_with("<h2")
                || rest_lower.starts_with("<h3") || rest_lower.starts_with("<h4")
                || rest_lower.starts_with("<h5") || rest_lower.starts_with("<h6")
                || rest_lower.starts_with("<li") || rest_lower.starts_with("<tr")
                || rest_lower.starts_with("<br") || rest_lower.starts_with("</p")
                || rest_lower.starts_with("</div") || rest_lower.starts_with("</tr")
            {
                if !last_was_block {
                    out.push('\n');
                    last_was_block = true;
                }
            }
            i += 1;
        } else if chars[i] == '>' && in_tag {
            in_tag = false;
            i += 1;
        } else if !in_tag {
            out.push(chars[i]);
            last_was_block = false;
            i += 1;
        } else {
            i += 1;
        }
    }

    // Decode common HTML entities
    let out = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");

    // Compress whitespace: collapse multiple blank lines and trailing spaces
    let mut lines: Vec<&str> = out.lines().map(|l| l.trim()).collect();
    lines.dedup_by(|a, b| a.is_empty() && b.is_empty());

    let result = lines.join("\n");
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_html_content() {
        assert!(is_html_content("<!DOCTYPE html><html>...</html>"));
        assert!(is_html_content("<html><body>Hello</body></html>"));
        assert!(is_html_content("  <!DOCTYPE html>"));
        assert!(is_html_content("<HTML lang=\"en\">"));
        assert!(is_html_content("something<head>blah</head>"));
        assert!(!is_html_content("Hello, this is plain text."));
        assert!(!is_html_content("{\"key\": \"value\"}"));
    }

    #[test]
    fn test_extract_readable_text_removes_scripts() {
        let html = "<html><body><p>Hello</p><script>var x = 1;</script><p>World</p></body></html>";
        let result = extract_readable_text(html);
        assert!(result.contains("Hello"));
        assert!(result.contains("World"));
        assert!(!result.contains("var x"));
    }

    #[test]
    fn test_extract_readable_text_removes_styles() {
        let html = "<html><head><style>.foo { color: red; }</style></head><body><p>Content</p></body></html>";
        let result = extract_readable_text(html);
        assert!(result.contains("Content"));
        assert!(!result.contains("color: red"));
    }

    #[test]
    fn test_extract_readable_text_removes_nav_footer() {
        let html = "<html><body><nav>Menu items</nav><main><p>Main content</p></main><footer>Copyright</footer></body></html>";
        let result = extract_readable_text(html);
        assert!(result.contains("Main content"));
        assert!(!result.contains("Menu items"));
        assert!(!result.contains("Copyright"));
    }

    #[test]
    fn test_extract_readable_text_decodes_entities() {
        let html = "<p>A &amp; B &lt; C &gt; D &quot;E&quot; &#39;F&#39;</p>";
        let result = extract_readable_text(html);
        assert!(result.contains("A & B < C > D \"E\" 'F'"));
    }

    #[test]
    fn test_extract_readable_text_block_elements_newlines() {
        let html = "<div>First</div><div>Second</div><p>Third</p>";
        let result = extract_readable_text(html);
        assert!(result.contains("First"));
        assert!(result.contains("Second"));
        assert!(result.contains("Third"));
        // Should have line breaks between block elements
        let lines: Vec<&str> = result.lines().filter(|l| !l.is_empty()).collect();
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_extract_readable_text_whitespace_compression() {
        let html = "<p>Hello</p>\n\n\n\n\n<p>World</p>";
        let result = extract_readable_text(html);
        // Multiple blank lines should be compressed
        assert!(!result.contains("\n\n\n"));
    }

    #[test]
    fn test_web_fetch_tool_metadata() {
        let tool = WebFetchTool::new();
        assert_eq!(tool.name(), "web_fetch");
        assert_eq!(tool.source(), ToolSource::BuiltIn);
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert!(tool.description().contains("extract"));
    }

    #[test]
    fn test_web_fetch_parameters_schema() {
        let tool = WebFetchTool::new();
        let params = tool.parameters();
        assert_eq!(params["type"], "object");
        let props = &params["properties"];
        assert!(props["url"].is_object());
        assert!(props["max_length"].is_object());
        assert!(props["extract_content"].is_object());
    }
}
