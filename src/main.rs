use clap::clap_app;

fn main() {
    let matches = clap_app!(ripit =>
        (version: "0.1")
        (@arg REPO: -r --repo +takes_value "Path to the repository (if empty, current directory is used)")
        (@arg COMMIT: +required "Commit to search (git revision string expected)")
        (@arg BRANCH: +required "Branch to use (git revision string expected)")
    )
    .get_matches();

    let repo_path = matches.value_of("REPO").unwrap_or(".");
    let commit_rev = matches.value_of("COMMIT").unwrap();
    let branch_rev = matches.value_of("BRANCH").unwrap();

    let repo = match git2::Repository::open(repo_path) {
        Ok(repo) => repo,
        Err(_) => {
            eprintln!("no git repository found in {}", repo_path);
            std::process::exit(1);
        }
    };

    let branch = match repo.revparse_single(branch_rev) {
        Ok(obj) => obj,
        Err(e) => {
            eprintln!("cannot find object from {}: {}", branch_rev, e);
            std::process::exit(1);
        }
    };

    let commit = match repo.revparse_single(commit_rev) {
        Ok(obj) => obj,
        Err(e) => {
            eprintln!("cannot find commit from {}: {}", branch_rev, e);
            std::process::exit(1);
        }
    };

    let mut revwalk = repo.revwalk().unwrap();
    revwalk.push(branch.id()).unwrap();
    revwalk.hide(commit.id()).unwrap();

    for oid in revwalk {
        println!("oid: {}", oid.unwrap());
    }
}