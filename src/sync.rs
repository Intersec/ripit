use crate::util;

/// Build a revwalk to iterate from a commit (excluded), up to the branch's last commit
fn build_revwalk<'a>(
    repo: &'a git2::Repository,
    commit: &git2::Commit,
    branch: &git2::Object,
) -> Result<git2::Revwalk<'a>, git2::Error> {
    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE);
    revwalk.push(branch.id())?;
    revwalk.hide(commit.id())?;
    Ok(revwalk)
}

// {{{ Fetch remote

pub fn update_remote(
    repo: &git2::Repository,
    remote_name: &str,
    branch_rev: &str,
) -> Result<(), git2::Error> {
    let mut remote = repo.find_remote(remote_name)?;

    println!("fetch branch {} in remote {}...", branch_rev, remote_name);
    remote.fetch(&[&branch_rev], None, None)
}

// }}}
// {{{ Sync branch

fn force_checkout_head(repo: &git2::Repository) -> Result<(), git2::Error> {
    let mut opts = git2::build::CheckoutBuilder::new();
    opts.force();
    repo.checkout_head(Some(&mut opts))
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
    force_checkout_head(&repo)?;

    Ok(())
}

/// Parse the commit message to retrieve the SHA-1 stored as a ripit tag
///
/// If the commit message contains the string "rip-it: <sha-1>", the sha-1 is returned
fn retrieve_ripit_tag(commit: &git2::Commit) -> Option<String> {
    let msg = commit.message()?;
    let tag_index = msg.find("rip-it: ")?;
    let sha1_start = tag_index + 8;

    if msg.len() >= sha1_start + 40 {
        Some(msg[(sha1_start)..(sha1_start + 40)].to_owned())
    } else {
        None
    }
}

/// Sync the local repository with the new changes from the given remote
pub fn sync_branch_with_remote(
    repo: &git2::Repository,
    remote: &str,
    branch_rev: &str,
) -> Result<(), git2::Error> {
    // Get SHA-1 of last synced commit
    let local_branch = repo.revparse_single(branch_rev)?;
    let sha1 = match retrieve_ripit_tag(&local_branch.peel_to_commit()?) {
        Some(sha1) => sha1,
        None => return Ok(()),
    };
    println!("found sha-1 {}", sha1);

    // Get the commit related to this SHA-1
    let commit = repo.find_commit(git2::Oid::from_str(&sha1)?)?;

    // Get the branch last commit in the remote
    let remote_branch = repo.revparse_single(&format!("{}/{}", remote, branch_rev))?;

    // Build revwalk from specified commit up to last commit in branch in remote
    let revwalk = build_revwalk(&repo, &commit, &remote_branch)?;

    // print out a summary of what would be cherry-picked
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

    // cherry-pick every commit, and add the rip-it tag in the commits messages
    let revwalk = build_revwalk(&repo, &commit, &remote_branch)?;
    for oid in revwalk {
        let ci = repo.find_commit(oid?)?;

        cherrypick(&repo, &ci)?;
    }

    Ok(())
}

// }}}
// {{{ Bootstrap branch

/// Cherrypick a given commit on top of HEAD, and add the ripit tag
fn commit_bootstrap<'a>(
    repo: &'a git2::Repository,
    remote_commit: &git2::Commit,
    remote: &str,
) -> Result<git2::Commit<'a>, git2::Error> {
    let msg = format!(
        "Bootstrap repository from remote {}\n\nrip-it: {}\n",
        remote,
        remote_commit.id()
    );

    // commit the whole index
    let head = match repo.head() {
        Ok(head) => {
            let oid = head.target().unwrap();
            Some(repo.find_commit(oid)?)
        },
        Err(_) => None,
    };

    let mut parents = vec![];
    if let Some(h) = head.as_ref() {
        parents.push(h);
    }

    let sig = repo.signature()?;
    let commit_oid = repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &msg,
        &remote_commit.tree()?,
        &parents,
    )?;

    force_checkout_head(repo)?;

    Ok(repo.find_commit(commit_oid)?)
}

/// Bootstrap the branch in the local repo with the state of the branch in the remote repo
///
/// Create a commit that will contain the whole index of the remote's branch HEAD, with the
/// appropriate ripit tag.
/// Following this bootstrap, synchronisation between the two repos will be possible.
pub fn bootstrap_branch_with_remote(
    repo: &git2::Repository,
    remote: &str,
    branch_rev: &str,
) -> Result<(), git2::Error> {
    // Get the branch last commit in the remote
    let remote_branch = repo.revparse_single(&format!("{}/{}", remote, branch_rev))?;
    let remote_commit = remote_branch.peel_to_commit()?;

    // build the bootstrap commit from the state of this commit
    let commit = commit_bootstrap(&repo, &remote_commit, remote)?;
    println!("boostrap commit {} created", commit.id());

    Ok(())
}

// }}}
