mod app;
mod sync;
mod util;

fn _main() -> Result<(), git2::Error> {
    let matches = app::parse_args();

    let repo_path = matches.value_of("REPO").unwrap_or(".");
    let repo = git2::Repository::open(repo_path)?;

    let branch_rev = matches.value_of("BRANCH").unwrap();
    let remote = matches.value_of("REMOTE").unwrap();

    // fetch last commits in remote
    sync::update_remote(&repo, &remote, &branch_rev)?;

    // sync local branch with remote by cherry-picking missing commits
    sync::sync_branch_with_remote(&repo, &remote, &branch_rev)
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
