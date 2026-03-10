//! Table formatting for CLI output

use std::fmt::Write;

/// Simple table renderer for terminal output
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    col_widths: Vec<usize>,
}

impl Table {
    pub fn new(headers: Vec<&str>) -> Self {
        let col_widths = headers.iter().map(|h| h.len()).collect();
        Self {
            headers: headers.into_iter().map(|h| h.to_string()).collect(),
            rows: Vec::new(),
            col_widths,
        }
    }

    pub fn add_row(&mut self, cells: Vec<String>) {
        for (i, cell) in cells.iter().enumerate() {
            if i < self.col_widths.len() {
                self.col_widths[i] = self.col_widths[i].max(cell.len());
            }
        }
        self.rows.push(cells);
    }

    pub fn render(&self) -> String {
        let mut output = String::new();

        // Header
        for (i, header) in self.headers.iter().enumerate() {
            if i > 0 {
                write!(output, "  ").ok();
            }
            write!(output, "{:<width$}", header, width = self.col_widths[i]).ok();
        }
        writeln!(output).ok();

        // Separator
        for (i, width) in self.col_widths.iter().enumerate() {
            if i > 0 {
                write!(output, "  ").ok();
            }
            write!(output, "{}", "-".repeat(*width)).ok();
        }
        writeln!(output).ok();

        // Rows
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    write!(output, "  ").ok();
                }
                let width = self.col_widths.get(i).copied().unwrap_or(0);
                write!(output, "{:<width$}", cell, width = width).ok();
            }
            writeln!(output).ok();
        }

        output
    }

    pub fn print(&self) {
        print!("{}", self.render());
    }
}
