use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use tracing::debug;

use octo_types::{RiskLevel, ToolContext, ToolOutput, ToolSource};

use super::traits::Tool;

const MAX_FILE_SIZE: u64 = 1_024 * 1_024; // 1 MB
/// Max size for binary files (XLSX/PDF can be larger)
const MAX_BINARY_FILE_SIZE: u64 = 10 * 1_024 * 1_024; // 10 MB

pub struct FileReadTool;

impl Default for FileReadTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FileReadTool {
    pub fn new() -> Self {
        Self
    }
}

/// Recognized binary file formats
#[derive(Debug, PartialEq)]
#[allow(dead_code)]
enum FileFormat {
    /// Plain text (UTF-8)
    Text,
    /// CSV/TSV spreadsheet
    Csv,
    /// Excel spreadsheet (xlsx, xls, ods)
    Spreadsheet,
    /// PDF document
    Pdf,
    /// ZIP archive
    Zip,
    /// JSON / JSONL
    Json,
    /// Unknown binary format
    UnknownBinary,
}

fn detect_format(path: &std::path::Path) -> FileFormat {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("csv" | "tsv") => FileFormat::Csv,
        Some("xlsx" | "xls" | "ods") => FileFormat::Spreadsheet,
        Some("pdf") => FileFormat::Pdf,
        Some("zip") => FileFormat::Zip,
        Some("json" | "jsonl" | "jsonld") => FileFormat::Json,
        Some("txt" | "md" | "rs" | "py" | "js" | "ts" | "toml" | "yaml" | "yml" | "cfg"
            | "ini" | "sh" | "bash" | "zsh" | "html" | "htm" | "xml" | "css" | "sql"
            | "rb" | "go" | "java" | "c" | "cpp" | "h" | "hpp" | "cs" | "swift" | "kt"
            | "r" | "m" | "mm" | "pl" | "pm" | "lua" | "vim" | "el" | "clj" | "ex"
            | "exs" | "erl" | "hs" | "ml" | "mli" | "tex" | "log" | "env" | "lock"
            | "dockerfile" | "makefile" | "gitignore") => FileFormat::Text,
        Some("docx" | "pptx") => FileFormat::Zip, // Office formats are ZIP archives
        _ => {
            // Try to detect by content: attempt UTF-8 read
            FileFormat::Text
        }
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Supports text files (with line numbers), CSV/TSV, \
         Excel spreadsheets (xlsx/xls/ods), PDF documents, ZIP archives, and JSON/JSONL. \
         For unsupported binary formats, returns file info and suggests alternative approaches."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path to read (absolute or relative to working directory)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Line number to start reading from (1-based, default: 1). Only applies to text files."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read (default: 2000). Only applies to text files."
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let path_str = params["path"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'path' parameter"))?;

        let offset = params["offset"].as_u64().unwrap_or(1).max(1) as usize;
        let limit = params["limit"].as_u64().unwrap_or(2000) as usize;

        // Defensive: detect blob references passed as file paths.
        // This can happen if the LLM sees a [blob:sha256:...] reference from a prior
        // turn and mistakenly tries to read it as a file.
        if crate::storage::BlobStore::parse_blob_ref(path_str).is_some()
            || path_str.starts_with("blob:")
        {
            return Ok(ToolOutput::error(
                "This path is a blob reference, not a file path. \
                 The original content was already shown in the tool result above. \
                 Do not attempt to file_read blob references."
                    .to_string(),
            ));
        }

        let path = if std::path::Path::new(path_str).is_absolute() {
            std::path::PathBuf::from(path_str)
        } else {
            ctx.working_dir.join(path_str)
        };

        // Security: validate path against policy
        if let Some(ref validator) = ctx.path_validator {
            if let Err(e) = validator.check_path(&path) {
                return Ok(ToolOutput::error(format!("Path validation failed: {e}")));
            }
        }

        // Symlink defense: reject symbolic links
        if let Some(output) = super::path_safety::reject_symlink(&path) {
            return Ok(output);
        }

        debug!(?path, offset, limit, "reading file");

        // Check file exists
        if !path.exists() {
            return Ok(ToolOutput::error(format!(
                "File not found: {}",
                path.display()
            )));
        }

        let metadata = tokio::fs::metadata(&path).await?;
        let file_size = metadata.len();
        let format = detect_format(&path);

        match format {
            FileFormat::Text | FileFormat::Json => {
                // Original text reading logic
                if file_size > MAX_FILE_SIZE {
                    return Ok(ToolOutput::error(format!(
                        "File too large: {} bytes (max: {MAX_FILE_SIZE} bytes)",
                        file_size
                    )));
                }
                read_text_file(&path, offset, limit).await
            }
            FileFormat::Csv => {
                if file_size > MAX_FILE_SIZE {
                    return Ok(ToolOutput::error(format!(
                        "File too large: {} bytes (max: {MAX_FILE_SIZE} bytes)",
                        file_size
                    )));
                }
                read_csv_file(&path).await
            }
            #[cfg(feature = "file-parsing")]
            FileFormat::Spreadsheet => {
                if file_size > MAX_BINARY_FILE_SIZE {
                    return Ok(ToolOutput::error(format!(
                        "Spreadsheet too large: {} bytes (max: {MAX_BINARY_FILE_SIZE} bytes)",
                        file_size
                    )));
                }
                read_spreadsheet(&path)
            }
            #[cfg(not(feature = "file-parsing"))]
            FileFormat::Spreadsheet => Ok(ToolOutput::success(format!(
                "File: {} ({} bytes, spreadsheet format)\n\n\
                 Spreadsheet parsing is not enabled. Use bash with python3:\n\
                 python3 -c \"import openpyxl; wb = openpyxl.load_workbook('{}'); \
                 [print(row) for row in wb.active.iter_rows(values_only=True)]\"",
                path.display(),
                file_size,
                path_str
            ))),
            #[cfg(feature = "file-parsing")]
            FileFormat::Pdf => {
                if file_size > MAX_BINARY_FILE_SIZE {
                    return Ok(ToolOutput::error(format!(
                        "PDF too large: {} bytes (max: {MAX_BINARY_FILE_SIZE} bytes)",
                        file_size
                    )));
                }
                read_pdf(&path)
            }
            #[cfg(not(feature = "file-parsing"))]
            FileFormat::Pdf => Ok(ToolOutput::success(format!(
                "File: {} ({} bytes, PDF format)\n\n\
                 PDF parsing is not enabled. Use bash:\n\
                 pdftotext '{}' - | head -200",
                path.display(),
                file_size,
                path_str
            ))),
            #[cfg(feature = "file-parsing")]
            FileFormat::Zip => {
                if file_size > MAX_BINARY_FILE_SIZE {
                    return Ok(ToolOutput::error(format!(
                        "Archive too large: {} bytes (max: {MAX_BINARY_FILE_SIZE} bytes)",
                        file_size
                    )));
                }
                read_zip(&path)
            }
            #[cfg(not(feature = "file-parsing"))]
            FileFormat::Zip => Ok(ToolOutput::success(format!(
                "File: {} ({} bytes, ZIP archive)\n\n\
                 ZIP parsing is not enabled. Use bash:\n\
                 unzip -l '{}'",
                path.display(),
                file_size,
                path_str
            ))),
            FileFormat::UnknownBinary => Ok(ToolOutput::success(format!(
                "File: {} ({} bytes, binary format)\n\n\
                 This file appears to be in a binary format that cannot be read as text.\n\
                 Suggestions:\n\
                 - Use `bash` with `file '{}'` to identify the file type\n\
                 - Use `bash` with `xxd '{}' | head -20` to inspect raw bytes\n\
                 - Use `bash` with `python3` and appropriate libraries to parse it",
                path.display(),
                file_size,
                path_str,
                path_str
            ))),
        }
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

/// Read a text file with line numbers
async fn read_text_file(
    path: &std::path::Path,
    offset: usize,
    limit: usize,
) -> Result<ToolOutput> {
    let content = tokio::fs::read_to_string(path).await;
    match content {
        Ok(text) => {
            let lines: Vec<&str> = text.lines().collect();
            let total_lines = lines.len();

            let start_idx = (offset - 1).min(total_lines);
            let end_idx = (start_idx + limit).min(total_lines);

            let mut output = String::new();
            for (i, line) in lines[start_idx..end_idx].iter().enumerate() {
                let line_num = start_idx + i + 1;
                output.push_str(&format!("{:>6}\t{}\n", line_num, line));
            }

            if end_idx < total_lines {
                output.push_str(&format!(
                    "\n[... {} more lines, {total_lines} total]",
                    total_lines - end_idx
                ));
            }

            Ok(ToolOutput::success(output))
        }
        Err(e) => {
            // If UTF-8 read fails, it's likely a binary file
            Ok(ToolOutput::error(format!(
                "Failed to read file as text: {e}\n\n\
                 This file may be in a binary format. Try:\n\
                 - bash: file '{}'\n\
                 - bash: xxd '{}' | head -20",
                path.display(),
                path.display()
            )))
        }
    }
}

/// Read a CSV/TSV file and format as a table
async fn read_csv_file(path: &std::path::Path) -> Result<ToolOutput> {
    let content = tokio::fs::read_to_string(path).await;
    match content {
        Ok(text) => {
            let lines: Vec<&str> = text.lines().collect();
            let total = lines.len();
            let display_lines = lines.iter().take(200);

            let mut output = format!("CSV file: {} ({} rows)\n\n", path.display(), total);
            for (i, line) in display_lines.enumerate() {
                output.push_str(&format!("{:>4}\t{}\n", i + 1, line));
            }
            if total > 200 {
                output.push_str(&format!("\n[... {} more rows]", total - 200));
            }
            Ok(ToolOutput::success(output))
        }
        Err(e) => Ok(ToolOutput::error(format!("Failed to read CSV: {e}"))),
    }
}

/// Read an Excel spreadsheet using calamine
#[cfg(feature = "file-parsing")]
fn read_spreadsheet(path: &std::path::Path) -> Result<ToolOutput> {
    use calamine::{open_workbook_auto, Data, Reader};

    let mut workbook = open_workbook_auto(path)
        .map_err(|e| anyhow::anyhow!("Failed to open spreadsheet: {e}"))?;

    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();
    let mut output = format!(
        "Spreadsheet: {} ({} sheet(s): {})\n\n",
        path.display(),
        sheet_names.len(),
        sheet_names.join(", ")
    );

    for (sheet_idx, name) in sheet_names.iter().enumerate() {
        if let Ok(range) = workbook.worksheet_range(name) {
            let rows: Vec<Vec<String>> = range
                .rows()
                .take(500) // Limit rows per sheet
                .map(|row| {
                    row.iter()
                        .map(|cell| match cell {
                            Data::Empty => String::new(),
                            Data::String(s) => s.clone(),
                            Data::Float(f) => {
                                if *f == (*f as i64) as f64 {
                                    format!("{}", *f as i64)
                                } else {
                                    format!("{f}")
                                }
                            }
                            Data::Int(i) => format!("{i}"),
                            Data::Bool(b) => format!("{b}"),
                            Data::DateTime(dt) => format!("{dt}"),
                            Data::DateTimeIso(s) => s.clone(),
                            Data::DurationIso(s) => s.clone(),
                            Data::Error(e) => format!("#ERR:{e:?}"),
                        })
                        .collect()
                })
                .collect();

            let total_rows = range.rows().count();

            if sheet_names.len() > 1 {
                output.push_str(&format!("### Sheet {}: {}\n", sheet_idx + 1, name));
            }

            // Format as CSV-like text
            for (i, row) in rows.iter().enumerate() {
                output.push_str(&format!("{:>4}\t{}\n", i + 1, row.join("\t")));
            }
            if total_rows > 500 {
                output.push_str(&format!("\n[... {} more rows]", total_rows - 500));
            }
            output.push('\n');
        }
    }

    Ok(ToolOutput::success(output.trim_end().to_string()))
}

/// Read a PDF document
#[cfg(feature = "file-parsing")]
fn read_pdf(path: &std::path::Path) -> Result<ToolOutput> {
    let bytes = std::fs::read(path)?;
    match pdf_extract::extract_text_from_mem(&bytes) {
        Ok(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok(ToolOutput::success(format!(
                    "PDF file: {} ({} bytes)\n\n\
                     No text content could be extracted (may be a scanned/image PDF).\n\
                     Try: pdftotext '{}' - | head -200",
                    path.display(),
                    bytes.len(),
                    path.display()
                )))
            } else {
                // Limit output to ~50K chars
                let display = if trimmed.len() > 50_000 {
                    format!("{}\n\n[... content truncated, {} chars total]", &trimmed[..50_000], trimmed.len())
                } else {
                    trimmed.to_string()
                };
                Ok(ToolOutput::success(format!(
                    "PDF file: {} ({} bytes)\n\n{}",
                    path.display(),
                    bytes.len(),
                    display
                )))
            }
        }
        Err(e) => Ok(ToolOutput::success(format!(
            "PDF file: {} ({} bytes)\n\n\
             Failed to extract text: {e}\n\
             Try: pdftotext '{}' - | head -200",
            path.display(),
            bytes.len(),
            path.display()
        ))),
    }
}

/// Read a ZIP archive - list contents and extract small text files
#[cfg(feature = "file-parsing")]
fn read_zip(path: &std::path::Path) -> Result<ToolOutput> {
    let file = std::fs::File::open(path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| anyhow::anyhow!("Failed to open ZIP: {e}"))?;

    let mut output = format!(
        "ZIP archive: {} ({} entries)\n\n",
        path.display(),
        archive.len()
    );

    output.push_str("Contents:\n");
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let size = entry.size();
            let compressed = entry.compressed_size();
            let name = entry.name().to_string();
            if entry.is_dir() {
                output.push_str(&format!("  📁 {}\n", name));
            } else {
                output.push_str(&format!(
                    "  📄 {} ({} bytes, compressed: {})\n",
                    name, size, compressed
                ));
            }
        }
    }

    // Try to extract and show small text files
    let mut previewed = 0;
    for i in 0..archive.len() {
        if previewed >= 3 {
            break; // Limit previews
        }
        if let Ok(mut entry) = archive.by_index(i) {
            if entry.is_dir() || entry.size() > 10_000 {
                continue;
            }
            let name = entry.name().to_string();
            // Only preview text-like files
            if name.ends_with(".txt")
                || name.ends_with(".csv")
                || name.ends_with(".json")
                || name.ends_with(".md")
                || name.ends_with(".xml")
            {
                let mut content = String::new();
                if std::io::Read::read_to_string(&mut entry, &mut content).is_ok() {
                    output.push_str(&format!(
                        "\n--- Preview: {} ---\n{}\n",
                        name,
                        if content.len() > 2000 {
                            // Find a valid UTF-8 char boundary at or before 2000
                            let end = content
                                .char_indices()
                                .take_while(|&(i, _)| i <= 2000)
                                .last()
                                .map(|(i, _)| i)
                                .unwrap_or(0);
                            format!("{}...\n[truncated]", &content[..end])
                        } else {
                            content
                        }
                    ));
                    previewed += 1;
                }
            }
        }
    }

    Ok(ToolOutput::success(output.trim_end().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_format_text() {
        assert_eq!(
            detect_format(std::path::Path::new("test.txt")),
            FileFormat::Text
        );
        assert_eq!(
            detect_format(std::path::Path::new("test.rs")),
            FileFormat::Text
        );
        assert_eq!(
            detect_format(std::path::Path::new("test.py")),
            FileFormat::Text
        );
    }

    #[test]
    fn test_detect_format_spreadsheet() {
        assert_eq!(
            detect_format(std::path::Path::new("data.xlsx")),
            FileFormat::Spreadsheet
        );
        assert_eq!(
            detect_format(std::path::Path::new("data.xls")),
            FileFormat::Spreadsheet
        );
        assert_eq!(
            detect_format(std::path::Path::new("data.ods")),
            FileFormat::Spreadsheet
        );
    }

    #[test]
    fn test_detect_format_pdf() {
        assert_eq!(
            detect_format(std::path::Path::new("doc.pdf")),
            FileFormat::Pdf
        );
    }

    #[test]
    fn test_detect_format_zip() {
        assert_eq!(
            detect_format(std::path::Path::new("archive.zip")),
            FileFormat::Zip
        );
        assert_eq!(
            detect_format(std::path::Path::new("report.docx")),
            FileFormat::Zip
        );
    }

    #[test]
    fn test_detect_format_csv() {
        assert_eq!(
            detect_format(std::path::Path::new("data.csv")),
            FileFormat::Csv
        );
        assert_eq!(
            detect_format(std::path::Path::new("data.tsv")),
            FileFormat::Csv
        );
    }

    #[test]
    fn test_detect_format_json() {
        assert_eq!(
            detect_format(std::path::Path::new("data.json")),
            FileFormat::Json
        );
        assert_eq!(
            detect_format(std::path::Path::new("data.jsonl")),
            FileFormat::Json
        );
    }

    #[test]
    fn test_file_read_tool_metadata() {
        let tool = FileReadTool::new();
        assert_eq!(tool.name(), "file_read");
        assert_eq!(tool.source(), ToolSource::BuiltIn);
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert!(tool.description().contains("Excel"));
        assert!(tool.description().contains("PDF"));
    }

    #[tokio::test]
    async fn test_file_read_text() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

        let tool = FileReadTool::new();
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::new(),
            working_dir: dir.path().to_path_buf(),
            path_validator: None,
        };
        let result = tool
            .execute(
                json!({"path": file_path.to_str().unwrap()}),
                &ctx,
            )
            .await
            .unwrap();
        let text = result.content;
        assert!(text.contains("line1"));
        assert!(text.contains("line2"));
    }

    #[tokio::test]
    async fn test_file_read_csv() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("data.csv");
        std::fs::write(&file_path, "name,age\nAlice,30\nBob,25\n").unwrap();

        let tool = FileReadTool::new();
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::new(),
            working_dir: dir.path().to_path_buf(),
            path_validator: None,
        };
        let result = tool
            .execute(json!({"path": file_path.to_str().unwrap()}), &ctx)
            .await
            .unwrap();
        let text = result.content;
        assert!(text.contains("CSV file"));
        assert!(text.contains("name,age"));
        assert!(text.contains("Alice,30"));
    }

    #[tokio::test]
    async fn test_file_read_not_found() {
        let tool = FileReadTool::new();
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::new(),
            working_dir: std::path::PathBuf::from("/tmp"),
            path_validator: None,
        };
        let result = tool
            .execute(json!({"path": "/tmp/nonexistent_file_xyz.txt"}), &ctx)
            .await
            .unwrap();
        assert!(result.content.contains("File not found"));
    }

    #[cfg(feature = "file-parsing")]
    #[tokio::test]
    async fn test_file_read_zip() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("test.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zip_writer = zip::ZipWriter::new(file);
        zip_writer
            .start_file("hello.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip_writer.write_all(b"Hello, World!").unwrap();
        zip_writer.finish().unwrap();

        let tool = FileReadTool::new();
        let ctx = ToolContext {
            sandbox_id: octo_types::SandboxId::new(),
            working_dir: dir.path().to_path_buf(),
            path_validator: None,
        };
        let result = tool
            .execute(json!({"path": zip_path.to_str().unwrap()}), &ctx)
            .await
            .unwrap();
        let text = result.content;
        assert!(text.contains("ZIP archive"));
        assert!(text.contains("hello.txt"));
        assert!(text.contains("Hello, World!"));
    }
}
