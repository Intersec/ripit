use clap::clap_app;

pub fn parse_args<'a>() -> clap::ArgMatches<'a> {
    clap_app!(ripit =>
        (version: "0.1")
        (@arg REPO: -r --repo +takes_value
         "Path to the repository (if unset, current directory is used)")
        (@arg BRANCH: -b --branch +takes_value
         "Branch to synchronize (if unset, 'master' is used)")
        (@arg REMOTE: +required "Name of the remote containing the commits to cherry-pick")
    )
    .get_matches()
}
