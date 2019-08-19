use crate::error;

pub struct Options {
    // path to the local repo
    pub repo: String,
    // name of the branch to synchronize
    pub branch: String,
    // full ref name for the local branch
    pub branch_ref: String,
    // name of the remote to synchronize from
    pub remote: String,

    pub bootstrap: bool,

    pub commit_msg_filters: regex::RegexSet,

    pub uproot: bool,
    pub verbose: bool,
    pub yes: bool,
}

pub fn parse_args() -> Result<Options, error::Error> {
    let yaml = clap::load_yaml!("cli.yml");
    let matches = clap::App::from_yaml(yaml)
        .setting(clap::AppSettings::ColoredHelp)
        .get_matches();

    let filters = match matches.values_of("filters") {
        Some(list) => list.collect(),
        None => vec![],
    };
    let commit_msg_filters = match regex::RegexSet::new(&filters) {
        Ok(set) => set,
        Err(regex_err) => {
            return Err(error::Error::InvalidConfig {
                field: "filter",
                error: regex_err,
            });
        }
    };

    let branch = matches.value_of("branch").unwrap_or("master").to_owned();
    let branch_ref = format!("refs/heads/{}", &branch);

    Ok(Options {
        repo: matches.value_of("repo").unwrap_or(".").to_owned(),
        branch,
        branch_ref,
        remote: matches.value_of("remote").unwrap().to_owned(),

        bootstrap: matches.is_present("bootstrap"),

        commit_msg_filters,

        uproot: matches.is_present("uproot"),
        verbose: matches.is_present("verbose"),
        yes: matches.is_present("yes"),
    })
}
