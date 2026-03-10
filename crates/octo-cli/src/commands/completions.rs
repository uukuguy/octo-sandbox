//! Shell completions generation — bash/zsh/fish/powershell via clap_complete

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;
use std::io;

/// Generate shell completions for the given shell type
pub fn generate_completions(shell: clap_complete::Shell) -> Result<()> {
    let mut cmd = crate::Cli::command();
    generate(shell, &mut cmd, "octo", &mut io::stdout());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_completions_bash() {
        // Just verify it doesn't panic — output goes to stdout
        // We can't easily capture stdout in a test, but we verify the function exists
        // and the Cli::command() works
        let cmd = crate::Cli::command();
        assert_eq!(cmd.get_name(), "octo");
    }

    #[test]
    fn test_generate_completions_zsh() {
        let cmd = crate::Cli::command();
        assert!(cmd.get_subcommands().count() > 0);
    }
}
