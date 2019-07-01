mod app;
mod error;
mod sync;
mod util;

fn _main() -> Result<(), error::Error> {
    let matches = app::parse_args();

    let repo_path = matches.value_of("REPO").unwrap_or(".");
    let repo = git2::Repository::open(repo_path)?;

    let branch_rev = matches.value_of("BRANCH").unwrap_or("master");
    let remote = matches.value_of("REMOTE").unwrap();

    // fetch last commits in remote
    sync::update_remote(&repo, &remote, &branch_rev)?;

    if matches.is_present("BOOTSTRAP") {
        // bootstrap the branch in the local repo with the state of the branch in the remote repo
        sync::bootstrap_branch_with_remote(&repo, &remote, &branch_rev)
    } else {
        // sync local branch with remote by cherry-picking missing commits
        sync::sync_branch_with_remote(&repo, &remote, &branch_rev)
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
