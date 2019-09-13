use crate::app;
use crate::commits_map::{CommitsMap, SyncedCommit};
use crate::error::Error;
use crate::tag;
use crate::util;
use std::io::Write;
use std::path::Path;

// {{{ Fetch remote

pub fn update_remote(repo: &git2::Repository, opts: &app::Options) -> Result<(), git2::Error> {
    let mut remote = repo.find_remote(&opts.remote)?;

    if opts.verbose {
        println!("Fetch branch {} in remote {}...", opts.branch, opts.remote);
    }
    remote.fetch(&[&opts.branch], None, None)
}

// }}}
// {{{ Find commits to sync */
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

/// Build a list of the commits to synchronize
///
/// In most situations, the commits to synchronize are simply the difference set
/// between the local repo (up to local_commit) and the remote one
/// (up to remote_commit). This is trivially buildable with a revwalk.
///
/// However, if we are in the process of syncing unknown commits, and the
/// local head contains uprooted commits, we must:
/// * rewind to the last non-uprooted commit, so that a sensical revwalk
///   can be built.
/// * ignore the already uprooted commits from the revwalk.
fn find_commits_to_sync<'a>(
    repo: &'a git2::Repository,
    local_commit: git2::Oid,
    remote_commit: &git2::Object,
    commits_map: &CommitsMap,
    opts: &app::Options,
) -> Result<Vec<git2::Commit<'a>>, Error> {
    let mut start = local_commit;
    let mut last_tag;
    let mut cnt = 0;

    // walk backwards until a non-uprooted commit is reached
    loop {
        let ci = repo.find_commit(start)?;
        let (tag, uprooted) = tag::retrieve_ripit_tag_or_throw(&ci)?;
        last_tag = tag;
        if !uprooted {
            // The bootstrap is not uprooted, the loop cannot be infinite
            break;
        }
        cnt += 1;
        start = ci.parent_id(0)?;
    }
    if opts.verbose {
        if cnt > 0 {
            println!("Rewinding {} commits to ignore uprooted ones.", cnt);
        }
        println!("Found ripit tag, last synced commit was {}.", last_tag);
    }

    // Get the commit related to this SHA-1
    let remote_start = repo.find_commit(git2::Oid::from_str(&last_tag)?)?;

    let revwalk = build_revwalk(repo, &remote_start, remote_commit)?;
    let mut commits = vec![];
    for oid in revwalk {
        let oid = oid?;
        if !commits_map.contains_key(oid) {
            commits.push(repo.find_commit(oid)?);
        } else if opts.verbose {
            println!("Ignoring {}: commit already synchronized.", oid);
        }
    }

    Ok(commits)
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

// TODO: use a string builder, to avoid the double alloc
fn update_commit_msg(orig_msg: &str, tag: &str, opts: &app::Options) -> String {
    let orig_msg = filter_commit_msg(orig_msg, &opts);
    if orig_msg.ends_with("\n") {
        format!("{}\n{}\n", orig_msg, tag)
    } else {
        format!("{}\n\n{}\n", orig_msg, tag)
    }
}

/// Append the tag to .git/MERGE_MSG, if it exists
fn update_merge_msg(repo: &git2::Repository, tag: &str, opts: &app::Options) {
    let path = Path::new(repo.path()).join("MERGE_MSG");
    let msg = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error when reading the MERGE_MSG file: {}", e);
            "".to_owned()
        }
    };

    // TODO: use a string builder
    let msg = update_commit_msg(&msg, tag, opts);

    if let Ok(mut file) = std::fs::File::create(&path) {
        if let Err(e) = write!(file, "{}", &msg) {
            eprintln!("Error when adding rip-it tag to MERGE_MSG: {}", e);
        }
    }
}

fn do_cherrypick<'a, 'b>(
    repo: &'a git2::Repository,
    commit: &'b git2::Commit,
    local_parents: &Vec<&'b git2::Commit>,
    is_merge: bool,
    uprooted: bool,
    opts: &app::Options,
) -> Result<git2::Commit<'a>, Error> {
    let branch_id = repo.refname_to_id(&opts.branch_ref)?;
    let update_branch = local_parents[0].id() == branch_id;

    // checkout parent, then cherrypick on top of it
    if update_branch {
        repo.set_head(&opts.branch_ref)?;
    } else {
        repo.set_head_detached(local_parents[0].id())?;
    }
    force_checkout_head(&repo)?;

    let tag = tag::format_ripit_tag(commit, uprooted);

    // cherrypick changes on top of HEAD
    let mut cherrypick_opts = git2::CherrypickOptions::new();
    if is_merge {
        // TODO: find the right mainline
        cherrypick_opts.mainline(1);
    }
    repo.cherrypick(&commit, Some(&mut cherrypick_opts))?;

    if repo.index()?.has_conflicts() {
        // The commit message is written in .git/MERGE_MSG, and will be
        // used when the user commits the changes.
        // It must thus be updated to:
        //  - apply the filters
        //  - add the ripit-tag
        update_merge_msg(repo, &tag, &opts);

        return Err(Error::HasConflicts {
            summary: commit.summary().unwrap_or("").to_owned(),
        });
    }

    let new_msg = match commit.message() {
        Some(orig_msg) => update_commit_msg(orig_msg, &tag, opts),
        None => tag,
    };
    // if the first parent is the branch's head, then directly
    // update the branch when committing
    let update_ref = if update_branch {
        &opts.branch_ref
    } else {
        "HEAD"
    };

    // commit the changes
    let tree_oid = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_oid)?;
    let ci_oid = repo.commit(
        Some(update_ref),
        &commit.author(),
        &commit.committer(),
        &new_msg,
        &tree,
        &local_parents,
    )?;

    let new_commit = repo.find_commit(ci_oid)?;
    if uprooted {
        println!("Uprooted commit {}.", new_commit.id());
    } else {
        println!("Created commit {}.", new_commit.id());
    }

    // if one of the following parents was the local branch, then update it.
    //
    // This can happen when syncing merge commits, as we will first synchronize the second
    // branch, and update the local branch, then synchronize the merge commit. We need to
    // fix the local branch back to the merge commit.
    if !update_branch && local_parents.iter().any(|p| p.id() == branch_id) {
        repo.branch(&opts.branch, &new_commit, true)?;
        repo.set_head(&opts.branch_ref)?;
    }

    // make the working directory match HEAD
    force_checkout_head(&repo)?;

    Ok(new_commit)
}

/// Cherrypick a given commit on top of HEAD, and add the ripit tag
fn copy_commit<'a, 'b>(
    repo: &'a git2::Repository,
    commit: &'b git2::Commit,
    commits_map: &'b CommitsMap,
    opts: &app::Options,
) -> Result<SyncedCommit<'a>, Error> {
    let head;

    if opts.verbose {
        println!("Copying commit {}...", commit.id());
    }

    // Find parent of the commit in local repo
    let mut local_parents = Vec::new();
    let mut uprooted = true;
    let is_merge = commit.parent_count() > 1;
    for parent_id in commit.parent_ids() {
        match commits_map.get(parent_id) {
            Some(parent_ci) => {
                local_parents.push(&parent_ci.commit);
                // A commit with uprooted parents is uprooted
                if !parent_ci.uprooted {
                    uprooted = false;
                }
            }
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
        // XXX: head *has* a target, because we have at least the bootstrap
        // commit.
        head = repo.find_commit(repo.head().unwrap().target().unwrap())?;
        local_parents.push(&head);
    }

    Ok(SyncedCommit {
        commit: do_cherrypick(repo, commit, &local_parents, is_merge, uprooted, opts)?,
        uprooted,
    })
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
    let mut commits_map = CommitsMap::new(repo, local_commit.id())?;

    // Get the branch last commit in the remote
    let remote_branch = repo.revparse_single(&format!("{}/{}", opts.remote, opts.branch))?;

    // Build revwalk from specified commit up to last commit in branch in remote
    let commits = find_commits_to_sync(
        &repo,
        local_commit.id(),
        &remote_branch,
        &commits_map,
        &opts,
    )?;

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
