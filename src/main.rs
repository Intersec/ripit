mod app;
mod sync;
mod util;

fn _main() -> Result<(), git2::Error> {
    let matches = app::parse_args();

    let remote = matches.value_of("REMOTE").unwrap();

    let repo_path = matches.value_of("REPO").unwrap_or(".");
    let repo = git2::Repository::open(repo_path)?;

    let branch_rev = matches.value_of("BRANCH").unwrap();
    let branch = repo.revparse_single(&format!("{}/{}", remote, branch_rev))?;

    let commit_rev = matches.value_of("COMMIT").unwrap();
    let commit = repo.revparse_single(&format!("{}/{}", remote, commit_rev))?;

    sync::sync_branch_with_remote(&repo, &branch, &commit)
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
