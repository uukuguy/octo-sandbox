use std::path::Path;

use anyhow::{Context, Result};
use git2::{Repository, Signature};

/// Git-backed version tracking for skill assets.
pub struct GitBackend {
    repo: Repository,
}

impl GitBackend {
    /// Open an existing git repo or initialize a new one at `path`.
    pub fn open_or_init(path: &Path) -> Result<Self> {
        let repo = match Repository::open(path) {
            Ok(repo) => repo,
            Err(_) => {
                let repo = Repository::init(path).context("init git repository")?;

                // Create an initial empty commit so HEAD exists
                let sig = Signature::now("skill-registry", "registry@eaasp.local")
                    .context("create git signature")?;
                let tree_id = repo.index()?.write_tree()?;
                {
                    let tree = repo.find_tree(tree_id)?;
                    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                        .context("create initial commit")?;
                }

                repo
            }
        };

        Ok(Self { repo })
    }

    /// Stage all changes and commit, returning the short commit hash.
    pub fn commit_change(&self, message: &str) -> Result<String> {
        let sig = Signature::now("skill-registry", "registry@eaasp.local")
            .context("create git signature")?;

        let mut index = self.repo.index().context("get repo index")?;
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .context("add all to index")?;
        index.write().context("write index")?;

        let tree_id = index.write_tree().context("write tree")?;
        let tree = self.repo.find_tree(tree_id).context("find tree")?;

        let head = self.repo.head().context("get HEAD")?;
        let parent = head.peel_to_commit().context("peel HEAD to commit")?;

        let oid = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
            .context("create commit")?;

        // Return short hash (7 chars)
        let short = &oid.to_string()[..7];
        Ok(short.to_string())
    }
}
