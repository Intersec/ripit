use clap::clap_app;

pub fn parse_args<'a>() -> clap::ArgMatches<'a> {
    clap_app!(ripit =>
        (version: "0.1")
        (@arg REPO: -r --repo +takes_value
         "Path to the repository (if empty, current directory is used)")
        (@arg REMOTE: +required "Name of the remote containing the commits to cherry-pick")
        (@arg COMMIT: +required "Commit to search in the remote (git revision string expected)")
        (@arg BRANCH: +required "Branch to use in both repo (git revision string expected)")
    )
    .get_matches()
}
