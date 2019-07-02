mod app;
mod error;
mod sync;
mod util;

/// Check that the local repo does not contain any staged or unstaged changes
///
/// This basically checks that "git diff HEAD" does not return any deltas
fn check_local_diff(repo: &git2::Repository) -> Result<(), error::Error> {
    let head = match repo.head() {
        Ok(tgt) => match tgt.target() {
            Some(oid) => Some(repo.find_commit(oid)?),
            None => None,
        },
        Err(_) => None,
    };

    let diff = match head {
        Some(ci) => repo.diff_tree_to_workdir_with_index(Some(&ci.tree()?), None),
        None => repo.diff_tree_to_workdir_with_index(None, None),
    }?;

    if diff.deltas().count() > 0 {
        Err(error::Error::HasLocalChanges)
    } else {
        Ok(())
    }
}

fn _main() -> Result<(), error::Error> {
    let opts = app::parse_args();

    let repo = git2::Repository::open(&opts.repo)?;
    check_local_diff(&repo)?;

    // fetch last commits in remote
    sync::update_remote(&repo, &opts)?;

    if opts.bootstrap {
        // bootstrap the branch in the local repo with the state of the branch in the remote repo
        sync::bootstrap_branch_with_remote(&repo, &opts)
    } else {
        // sync local branch with remote by cherry-picking missing commits
        sync::sync_branch_with_remote(&repo, &opts)
    }
}

fn main() {
    std::process::exit(match _main() {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("{}", e);
            // 1 is for clap, 2 for git errors for the moment
            2
        }
    })
}
