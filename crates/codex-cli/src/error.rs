use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("No git repository found at {path} or any parent.\n\ncodex-tree requires a git repository because:\n  - Tree versioning tracks the source commit\n  - The 'update' command uses git diff to detect changes\n\nTo fix: Run 'git init' in your project root, then retry.\n\n[AI hint: If running in CI, ensure the checkout action includes git history.]")]
    NoGitRepo { path: PathBuf },

    #[error(".codex-tree/ already exists at {path}\n\nA knowledge tree has already been generated for this project.\n\n  To update incrementally:  codex-tree update\n  To rebuild from scratch:  codex-tree regen\n  To force overwrite:       rm -rf .codex-tree && codex-tree init")]
    TreeAlreadyExists { path: PathBuf },

    #[error("No source files found at {path}\n\ncodex-tree found no files matching any registered language.\nSupported languages: {languages}\n\nTo fix: Ensure the path points to a directory with source files.")]
    NoSourceFiles { path: PathBuf, languages: String },

    #[error("{0}")]
    Parser(#[from] codex_parser::error::ParserError),

    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, CliError>;
