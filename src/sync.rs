use crate::util;

/// Build a revwalk to iterate from a commit (excluded), up to the branch's last commit
fn build_revwalk<'a>(
    repo: &'a git2::Repository,
    branch: &git2::Object,
    commit: &git2::Object,
) -> Result<git2::Revwalk<'a>, git2::Error> {
    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE);
    revwalk.push(branch.id())?;
    revwalk.hide(commit.id())?;
    Ok(revwalk)
}

/// Cherrypick a given commit on top of HEAD, and add the ripit tag
fn cherrypick(repo: &git2::Repository, commit: &git2::Commit) -> Result<(), git2::Error> {
    let new_msg = format!(
        "{}\nrip-it: {}\n",
        commit.message().unwrap_or(""),
        commit.id()
    );

    // commit the changes
    let head_oid = repo.head()?.target().unwrap();
    let head = repo.find_commit(head_oid)?;
    repo.commit(
        Some("HEAD"),
        &commit.author(),
        &commit.committer(),
        &new_msg,
        &commit.tree()?,
        &[&head],
    )?;

    // make the working directory match HEAD
    let mut opts = git2::build::CheckoutBuilder::new();
    opts.force();
    repo.checkout_head(Some(&mut opts))?;

    Ok(())
}

/// Sync the local repository with the new changes from the given remote
pub fn sync_branch_with_remote(
    repo: &git2::Repository,
    branch: &git2::Object,
    commit: &git2::Object,
) -> Result<(), git2::Error> {
    // Build revwalk from specified commit up to specified branch
    let revwalk = build_revwalk(&repo, &branch, &commit)?;

    print!("Commits to cherry-pick:\n\n");
    for oid in revwalk {
        let ci = repo.find_commit(oid?)?;

        print!(
            "Commit {id}\n \
             Author: {author}\n \
             {msg}\n\n",
            id = ci.id(),
            author = ci.author(),
            msg = ci.message().unwrap_or("")
        );
    }

    if !util::confirm_action() {
        return Ok(());
    }

    let revwalk = build_revwalk(&repo, &branch, &commit)?;
    for oid in revwalk {
        let ci = repo.find_commit(oid?)?;

        cherrypick(&repo, &ci)?;
    }

    Ok(())
}
