use crate::app;
use crate::error::Error;
use crate::util;
use std::collections::HashMap;

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
// {{{ Build commits map */
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

fn build_commits_map<'a>(
    repo: &'a git2::Repository,
    last_commit: &git2::Commit,
) -> Result<HashMap<git2::Oid, git2::Commit<'a>>, Error> {
    let mut map = HashMap::new();

    // build revwalk from the first commit of the repo up to the provided commit
    let mut revwalk = repo.revwalk()?;
    revwalk.push(last_commit.id())?;

    for oid in revwalk {
        let commit = repo.find_commit(oid?)?;

        // a commit missing a tag could be an error too. By ignoring it, it will lead to errors
        // if it is a parent of a commit to sync.
        let tag = match retrieve_ripit_tag(&commit) {
            Some(tag) => tag,
            None => continue,
        };
        let remote_oid = git2::Oid::from_str(&tag)?;

        map.insert(remote_oid, commit);
    }

    Ok(map)
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

fn do_cherrypick<'a, 'b>(
    repo: &'a git2::Repository,
    commit: &'b git2::Commit,
    local_parents: &Vec<&'b git2::Commit>,
    opts: &app::Options,
) -> Result<git2::Commit<'a>, Error> {
    // checkout parent, then cherrypick on top of it
    repo.set_head_detached(local_parents[0].id())?;
    force_checkout_head(&repo)?;

    let new_msg = format!(
        "{}\nrip-it: {}\n",
        filter_commit_msg(commit.message().unwrap_or(""), opts),
        commit.id()
    );

    // cherrypick changes on top of HEAD
    let mut cherrypick_opts = git2::CherrypickOptions::new();
    if local_parents.len() > 1 {
        // TODO: find the right mainline
        cherrypick_opts.mainline(1);
    }
    repo.cherrypick(&commit, Some(&mut cherrypick_opts))?;

    // commit the changes
    let tree_oid = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;
    let ci_oid = repo.commit(
        Some("HEAD"),
        &commit.author(),
        &commit.committer(),
        &new_msg,
        &tree,
        &local_parents,
    )?;

    let new_commit = repo.find_commit(ci_oid)?;
    println!("Created commit {}.", new_commit.id());

    // make the working directory match HEAD
    force_checkout_head(&repo)?;

    Ok(new_commit)
}

/// Cherrypick a given commit on top of HEAD, and add the ripit tag
fn copy_commit<'a, 'b>(
    repo: &'a git2::Repository,
    commit: &'b git2::Commit,
    commits_map: &'b HashMap<git2::Oid, git2::Commit>,
    opts: &app::Options,
) -> Result<git2::Commit<'a>, Error> {
    let head;

    if opts.verbose {
        println!("Copying commit {}...", commit.id());
    }

    // Find parent of the commit in local repo
    let mut local_parents = Vec::new();
    for parent_id in commit.parent_ids() {
        match commits_map.get(&parent_id) {
            Some(parent_ci) => local_parents.push(parent_ci),
            None => {
                if !opts.uproot {
                    return Err(Error::UnknownParent {
                        commit_id: commit.id(),
                        parent_id,
                    });
                }
            }
        }
    }

    if local_parents.len() == 0 {
        assert!(opts.uproot);
        // uproot the commit on HEAD
        println!("Uproot commit {}.", commit.id());
        // XXX: head *has* a target, because we have at least the bootstrap
        // commit.
        head = repo.find_commit(repo.head().unwrap().target().unwrap())?;
        local_parents.push(&head);
    }

    do_cherrypick(repo, commit, &local_parents, opts)
}

/// Sync the local repository with the new changes from the given remote
pub fn sync_branch_with_remote(repo: &git2::Repository, opts: &app::Options) -> Result<(), Error> {
    let local_commit = repo.revparse_single(&opts.branch)?.peel_to_commit()?;

    // Build map of remote commit sha-1 => local commit
    //
    // This is used to find the parents of each commits to sync, and thus properly
    // recreate the same topology.
    // FIXME: we really should not do this on every execution. We should either build a database,
    // or have a "daemon" behavior. This is broken because commits not directly addressable from
    // the branch may be synced but won't be remapped in this map.
    let mut commits_map = build_commits_map(repo, &local_commit)?;

    // Get SHA-1 of last synced commit
    let sha1 = match retrieve_ripit_tag(&local_commit) {
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

    print!("Commits to synchronize:\n");
    for ci in &commits {
        print!(
            "  Commit {id}\n    {author}\n    {summary}\n\n",
            id = ci.id(),
            author = ci.author(),
            summary = ci.summary().unwrap_or("")
        );
    }

    if !opts.yes && !util::confirm_action() {
        return Ok(());
    }

    // cherry-pick every commit, and add the rip-it tag in the commits messages
    for ci in &commits {
        let copied_ci = copy_commit(&repo, &ci, &commits_map, opts)?;

        // add mapping for this new pair
        commits_map.insert(ci.id(), copied_ci);
    }

    /* FIXME: we should update the local branch ref too */

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
