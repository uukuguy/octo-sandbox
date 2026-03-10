//! Human-readable text output

use super::TextOutput;

pub fn print_text<T: TextOutput>(value: &T) {
    println!("{}", value.to_text());
}

/// Print a simple key-value table
pub fn print_kv(pairs: &[(&str, &str)]) {
    let max_key = pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
    for (key, value) in pairs {
        println!("  {:<width$}  {}", key, value, width = max_key);
    }
}

/// Print a list with bullet points
pub fn print_list(items: &[String]) {
    for item in items {
        println!("  - {}", item);
    }
}

/// Print a section header
pub fn print_header(title: &str) {
    println!("{}", title);
    println!("{}", "-".repeat(title.len()));
}
