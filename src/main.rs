use clap::clap_app;

fn _main() -> Result<(), git2::Error> {
    let matches = clap_app!(ripit =>
        (version: "0.1")
        (@arg REPO: -r --repo +takes_value "Path to the repository (if empty, current directory is used)")
        (@arg COMMIT: +required "Commit to search (git revision string expected)")
        (@arg BRANCH: +required "Branch to use (git revision string expected)")
    )
    .get_matches();

    let repo_path = matches.value_of("REPO").unwrap_or(".");
    let repo = git2::Repository::open(repo_path)?;

    let branch_rev = matches.value_of("BRANCH").unwrap();
    let branch = repo.revparse_single(branch_rev)?;

    let commit_rev = matches.value_of("COMMIT").unwrap();
    let commit = repo.revparse_single(commit_rev)?;

    // Build revwalk from specified commit up to specified branch
    let mut revwalk = repo.revwalk()?;
    revwalk.push(branch.id())?;
    revwalk.hide(commit.id())?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE);

    print!("Commits to cherry-pick:\n\n");
    for oid in revwalk {
        let ci = repo.find_commit(oid?)?;

        println!("commit {}", ci.id());
        println!("Author: {}", ci.author());
        println!("{}", ci.summary().unwrap_or(""));
        println!("");
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
