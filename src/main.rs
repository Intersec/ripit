mod app;
mod error;
mod sync;
mod util;

fn _main() -> Result<(), error::Error> {
    let opts = app::parse_args();

    let repo = git2::Repository::open(opts.repo)?;

    // fetch last commits in remote
    sync::update_remote(&repo, &opts.remote, &opts.branch, opts.verbose)?;

    if opts.bootstrap {
        // bootstrap the branch in the local repo with the state of the branch in the remote repo
        sync::bootstrap_branch_with_remote(&repo, &opts.remote, &opts.branch)
    } else {
        // sync local branch with remote by cherry-picking missing commits
        sync::sync_branch_with_remote(&repo, &opts.remote, &opts.branch, opts.verbose)
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
