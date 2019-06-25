use clap::clap_app;

// for stdout().flush
use std::io::Write;

/// Display a prompt asking for confirmation by the user
///
/// Returns true if the user confirmed, false in all other cases
fn confirm_action() -> bool {
    let mut input = String::new();

    loop {
        print!("Is this ok? [yN] ");
        std::io::stdout().flush().unwrap();

        match std::io::stdin().read_line(&mut input) {
            Err(_) => return false,
            _ => (),
        };

        match input.trim().as_ref() {
            "y" | "Y" => return true,
            "n" | "N" => return false,
            _ => (),
        }
        input.clear();
    }
}

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

fn _main() -> Result<(), git2::Error> {
    let matches = clap_app!(ripit =>
        (version: "0.1")
        (@arg REPO: -r --repo +takes_value
         "Path to the repository (if empty, current directory is used)")
        (@arg REMOTE: +required "Name of the remote containing the commits to cherry-pick")
        (@arg COMMIT: +required "Commit to search in the remote (git revision string expected)")
        (@arg BRANCH: +required "Branch to use in both repo (git revision string expected)")
    )
    .get_matches();

    let remote = matches.value_of("REMOTE").unwrap();

    let repo_path = matches.value_of("REPO").unwrap_or(".");
    let repo = git2::Repository::open(repo_path)?;

    let branch_rev = matches.value_of("BRANCH").unwrap();
    let branch = repo.revparse_single(&format!("{}/{}", remote, branch_rev))?;

    let commit_rev = matches.value_of("COMMIT").unwrap();
    let commit = repo.revparse_single(&format!("{}/{}", remote, commit_rev))?;

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

    if !confirm_action() {
        return Ok(());
    }

    let revwalk = build_revwalk(&repo, &branch, &commit)?;
    for oid in revwalk {
        let ci = repo.find_commit(oid?)?;

        cherrypick(&repo, &ci)?;
    }

    Ok(())
}

fn main() {
    std::process::exit(match _main() {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("{}", e.message());
            // 1 is for clap, 2 for git errors for the moment
            2
        }
    })
}
