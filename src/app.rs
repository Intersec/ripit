use clap::clap_app;

pub struct Opts {
    pub repo: String,
    pub branch: String,
    pub remote: String,

    pub bootstrap: bool,
    pub verbose: bool,
}

pub fn parse_args() -> Opts {
    let matches = clap_app!(ripit =>
        (version: "0.1")
        (@arg repo: -r --repo +takes_value
         "Path to the repository (if unset, current directory is used)")
        (@arg branch: -b --branch +takes_value
         "Branch to synchronize (if unset, 'master' is used)")
        (@arg bootstrap: --bootstrap
         "Bootstrap the local repository")
        (@arg remote: +required "Name of the remote containing the commits to cherry-pick")
        (@arg verbose: -v --verbose "Print verbose logs")
    )
    .get_matches();

    Opts {
        repo: matches.value_of("repo").unwrap_or(".").to_owned(),
        branch: matches.value_of("branch").unwrap_or("master").to_owned(),
        remote: matches.value_of("remote").unwrap().to_owned(),
        bootstrap: matches.is_present("bootstrap"),
        verbose: matches.is_present("verbose"),
    }
}
