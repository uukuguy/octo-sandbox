//! Centralized tool description manuals.
//!
//! Each tool's description is a detailed usage manual (not a one-liner)
//! following the tool-prompt coupling pattern from Claude Code OSS.
//! Structure: purpose -> when to use -> when NOT to use -> best practices -> examples.

pub const BASH_DESCRIPTION: &str = r#"Execute a bash command in the working directory. Returns stdout, stderr, and exit code.

## When to use
- System commands that have no dedicated tool equivalent (git, make, cargo, npm, docker, etc.)
- Running tests, builds, or scripts
- Process management (ps, kill, etc.)

## When NOT to use
- To read files — use `file_read` instead of cat, head, or tail
- To edit files — use `file_edit` instead of sed or awk
- To create files — use `file_write` instead of echo/cat with redirection
- To search for files — use `glob` instead of find or ls
- To search file contents — use `grep` instead of grep or rg

## Parameters
- `command` (required): The bash command to execute
- `timeout_ms` (optional): Maximum execution time in milliseconds (default: 30000)
- `working_dir` (optional): Directory to run the command in

## Best practices
- Quote file paths that contain spaces
- Prefer short-running commands; for long operations, inform the user
- When running multiple independent commands, batch them with `&&` or run multiple tool calls in parallel
- For git operations: never force-push to main, never skip hooks (--no-verify), prefer new commits over amending
- Be cautious with destructive commands (rm -rf, git reset --hard) — confirm with user first

## Dangerous patterns (always confirm first)
- `rm -rf /` or `rm -rf ~` — targets root or home directory
- `git push --force` — can overwrite remote history
- `curl ... | sh` — executes arbitrary remote code
- `sudo ...` — elevated privilege operations"#;

pub const FILE_READ_DESCRIPTION: &str = r#"Read a file from the filesystem. Returns file content with line numbers.

## Supported formats
- Text files: source code, config, markdown, JSON, YAML, etc.
- Binary files: PDF, images (PNG/JPG), Excel (xlsx/xls), Word (docx)
- Jupyter notebooks (.ipynb): returns all cells with outputs

## Parameters
- `file_path` (required): Absolute path to the file
- `offset` (optional): Line number to start reading from (1-based)
- `limit` (optional): Maximum number of lines to read

## Best practices
- Always read a file before editing it
- For large files, use `offset` and `limit` to read specific sections
- For binary files not directly supported, use bash with python3 and appropriate libraries
- Results include line numbers for easy reference in `file_edit`"#;

pub const FILE_EDIT_DESCRIPTION: &str = r#"Edit a file by replacing an exact string match with new content.

## Important rules
- You MUST read the file first before editing. This tool will fail if you haven't read the file.
- The `old_string` must appear exactly once in the file (unless `replace_all` is true). If it's not unique, provide more surrounding context to make it unique.
- Preserve exact indentation (tabs/spaces) from the original file.
- NEVER include line numbers in old_string or new_string — they are display-only from file_read.

## Parameters
- `file_path` (required): Absolute path to the file to modify
- `old_string` (required): The exact text to replace
- `new_string` (required): The replacement text (must differ from old_string)
- `replace_all` (optional): Replace all occurrences (default: false)

## Best practices
- Prefer this tool over `file_write` for modifying existing files — it only sends the diff
- Include enough surrounding context in `old_string` to ensure uniqueness
- Use `replace_all` for renaming variables or updating repeated patterns"#;

pub const FILE_WRITE_DESCRIPTION: &str = r#"Write content to a file. Creates the file if it doesn't exist, or overwrites if it does. Creates parent directories as needed.

## Important rules
- If the file already exists, you MUST read it first with `file_read`
- ALWAYS prefer `file_edit` for modifying existing files — it only sends the diff
- Do NOT create files unless absolutely necessary for your task
- Do NOT create documentation files (*.md, README) unless explicitly requested

## Parameters
- `file_path` (required): Absolute path to the file to write
- `content` (required): The complete file content"#;

pub const GREP_DESCRIPTION: &str = r#"Search file contents using regular expressions. Built on ripgrep for high performance.

## Parameters
- `pattern` (required): Regular expression pattern to search for
- `path` (optional): File or directory to search in (default: working directory)
- `glob` (optional): Glob pattern to filter files (e.g., "*.rs", "*.{ts,tsx}")
- `output_mode` (optional): "content" (matching lines), "files_with_matches" (file paths only, default), "count" (match counts)
- `-A`, `-B`, `-C` (optional): Lines of context after/before/around matches (requires output_mode: "content")
- `head_limit` (optional): Limit output to first N results (default: 250)

## Best practices
- Use `output_mode: "files_with_matches"` first to find relevant files, then `"content"` for details
- Use `glob` to narrow search to specific file types
- For literal special characters (braces, dots), escape them: `interface\\{\\}` to find `interface{}`
- Use `-i` for case-insensitive search"#;

pub const GLOB_DESCRIPTION: &str = r#"Find files matching a glob pattern. Returns file paths sorted by modification time (newest first).

## Parameters
- `pattern` (required): Glob pattern (e.g., "**/*.rs", "src/**/*.ts", "*.json")
- `path` (optional): Base directory to search in (default: working directory)

## Glob syntax
- `*` matches any characters except path separator
- `**` matches any characters including path separator (recursive)
- `?` matches exactly one character
- `{a,b}` matches either pattern
- `[abc]` matches any character in the set

## Best practices
- Use this instead of `bash(find ...)` for file discovery
- Combine with `grep` for content search after finding files"#;

pub const WEB_SEARCH_DESCRIPTION: &str = r#"Search the web for information. Returns search results with titles, URLs, and content snippets.

## Parameters
- `query` (required): Search query string

## Best practices
- Formulate specific, precise queries rather than vague ones
- If results are insufficient, reformulate with different keywords
- Use `web_fetch` to read full page content when search snippets are not enough
- Cross-reference information from multiple sources when accuracy is critical
- For library/framework documentation, prefer reading official docs over blog posts"#;

pub const WEB_FETCH_DESCRIPTION: &str = r#"Fetch content from a URL. Extracts readable text from HTML pages, stripping scripts, styles, and navigation.

## Parameters
- `url` (required): The URL to fetch
- `raw` (optional): If true, return raw HTML instead of extracted text (default: false)

## Best practices
- Use after `web_search` to read full page content from promising results
- Content is automatically truncated if too long — check for truncation markers
- For API endpoints returning JSON, use `raw: true` to get the full response
- Respect rate limits — don't fetch the same URL repeatedly"#;

pub const SUBAGENT_DESCRIPTION: &str = r#"Spawn a sub-agent to handle a delegated task. The sub-agent runs asynchronously with its own context and returns results when complete.

## When to use
- Need to parallelize multiple independent sub-tasks
- Research tasks that require extensive searching/reading (protects main context from bloat)
- Tasks needing different tool permissions or models
- Complex work that benefits from focused, isolated execution

## When NOT to use
- Simple single-step operations (just use the tool directly)
- Tasks that need the current conversation context (sub-agents start fresh)
- Searching within 2-3 specific files (use `grep`/`glob` directly)

## Writing effective prompts
Sub-agents start with zero context. Write prompts like briefing a capable colleague who just walked in:
- Explain WHAT you want and WHY
- Describe what you already know and what you've already ruled out
- Give enough background for the sub-agent to make judgment calls
- If you need a brief response, say so explicitly

## Anti-patterns
- Don't delegate synthesis/judgment ("fix the bug based on your findings") — specify what to change
- Don't spawn sub-agents for trivial tasks (reading one file, running one command)
- Don't peek at sub-agent intermediate output — wait for the final result

## Parameters
- `prompt` (required): Detailed task description for the sub-agent
- `agent_type` (optional): Specialized agent type to use
- `model` (optional): Model override for this sub-agent"#;
