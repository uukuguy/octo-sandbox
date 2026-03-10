//! Markdown terminal rendering using termimad

/// Render markdown text to terminal
pub fn render_markdown(text: &str) {
    // Use termimad for markdown rendering
    let skin = termimad::MadSkin::default();
    skin.print_text(text);
}

/// Render markdown to string (for piping)
pub fn render_markdown_to_string(text: &str) -> String {
    let skin = termimad::MadSkin::default();
    skin.term_text(text).to_string()
}
