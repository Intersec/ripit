use crate::app;
use crate::error::Error;
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

pub fn update_remote(repo: &git2::Repository, opts: &app::Options) -> Result<(), git2::Error> {
    let mut remote = repo.find_remote(&opts.remote)?;

    if opts.verbose {
        println!("Fetch branch {} in remote {}...", opts.branch, opts.remote);
    }
    remote.fetch(&[&opts.branch], None, None)
}

// }}}
// {{{ Sync branch

fn force_checkout_head(repo: &git2::Repository) -> Result<(), git2::Error> {
    let mut opts = git2::build::CheckoutBuilder::new();
    opts.force();
    repo.checkout_head(Some(&mut opts))
}

fn filter_commit_msg(msg: &str, opts: &app::Options) -> String {
    if opts.commit_msg_filters.len() == 0 {
        return msg.to_owned();
    }

    let new_lines: Vec<&str> = msg
        .lines()
        .filter(|line| {
            if opts.commit_msg_filters.is_match(line) {
                if opts.verbose {
                    println!("  Filtering out line '{}'", line);
                }
                false
            } else {
                true
            }
        })
        .collect();

    new_lines.join("\n")
}

/// Cherrypick a given commit on top of HEAD, and add the ripit tag
fn copy_commit(
    repo: &git2::Repository,
    commit: &git2::Commit,
    opts: &app::Options,
) -> Result<(), git2::Error> {
    if opts.verbose {
        println!("Copying commit {}...", commit.id());
    }

    let new_msg = format!(
        "{}\nrip-it: {}\n",
        filter_commit_msg(commit.message().unwrap_or(""), opts),
        commit.id()
    );

    // cherrypick changes on top of HEAD
    let mut cherrypick_opts = git2::CherrypickOptions::new();
    if commit.parents().len() > 1 {
        // TODO: find the right mainline
        cherrypick_opts.mainline(1);
    }
    repo.cherrypick(&commit, Some(&mut cherrypick_opts))?;

    // commit the changes
    let head_oid = repo.head()?.target().unwrap();
    let head = repo.find_commit(head_oid)?;
    let tree_oid = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;
    let ci_oid = repo.commit(
        Some("HEAD"),
        &commit.author(),
        &commit.committer(),
        &new_msg,
        &tree,
        &[&head],
    )?;

    println!("Created commit {}.", repo.find_commit(ci_oid)?.id());

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
pub fn sync_branch_with_remote(repo: &git2::Repository, opts: &app::Options) -> Result<(), Error> {
    // Get SHA-1 of last synced commit
    let local_branch = repo.revparse_single(&opts.branch)?;
    let sha1 = match retrieve_ripit_tag(&local_branch.peel_to_commit()?) {
        Some(sha1) => sha1,
        None => return Err(Error::TagMissing),
    };
    if opts.verbose {
        println!("Found ripit tag, last synced commit was {}.", sha1);
    }

    // Get the commit related to this SHA-1
    let commit = repo.find_commit(git2::Oid::from_str(&sha1)?)?;

    // Get the branch last commit in the remote
    let remote_branch = repo.revparse_single(&format!("{}/{}", opts.remote, opts.branch))?;

    // Build revwalk from specified commit up to last commit in branch in remote
    let revwalk = build_revwalk(&repo, &commit, &remote_branch)?;
    let mut commits = vec![];
    for oid in revwalk {
        commits.push(repo.find_commit(oid?)?);
    }

    if commits.len() == 0 {
        println!(
            "Nothing to synchronize, already up to date with {}/{}.",
            opts.remote, opts.branch
        );
        return Ok(());
    }

    print!("Commits to cherry-pick:\n\n");
    for ci in &commits {
        print!(
            "Commit {id}\n \
             Author: {author}\n \
             {msg}\n\n",
            id = ci.id(),
            author = ci.author(),
            msg = ci.message().unwrap_or("")
        );
    }

    if !opts.yes && !util::confirm_action() {
        return Ok(());
    }

    // cherry-pick every commit, and add the rip-it tag in the commits messages
    for ci in &commits {
        copy_commit(&repo, &ci, opts)?;
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
        }
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
    opts: &app::Options,
) -> Result<(), Error> {
    // Get the branch last commit in the remote
    let remote_branch = repo.revparse_single(&format!("{}/{}", opts.remote, opts.branch))?;
    let remote_commit = remote_branch.peel_to_commit()?;

    // build the bootstrap commit from the state of this commit
    let commit = commit_bootstrap(&repo, &remote_commit, &opts.remote)?;
    println!("Bootstrap commit {} created.", commit.id());

    Ok(())
}

// }}}
