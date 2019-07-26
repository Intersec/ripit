use crate::error;
use serde::Deserialize;

pub struct Options {
    // path to the local repo
    pub repo: String,
    // name of the branch to synchronize
    pub branch: String,
    // full ref name for the local branch
    pub branch_ref: String,
    // name of the remote to synchronize from
    pub remote: String,

    pub commit_msg_filters: regex::RegexSet,

    pub bootstrap: bool,
    pub uproot: bool,
    pub verbose: bool,
    pub yes: bool,
}

#[derive(Deserialize)]
struct YamlCfg {
    repo: Option<String>,
    remote: String,
    branch: Option<String>,
    filters: Option<Vec<String>>,
}

pub fn parse_args() -> Result<Options, error::Error> {
    let yaml = clap::load_yaml!("cli.yml");
    let matches = clap::App::from_yaml(yaml)
        .setting(clap::AppSettings::ColoredHelp)
        .get_matches();

    let path = matches.value_of("config_file").unwrap();
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(error) => {
            return Err(error::Error::FailedOpenCfg {
                path: path.to_owned(),
                error,
            })
        }
    };

    let cfg: YamlCfg = match serde_yaml::from_reader(file) {
        Ok(cfg) => cfg,
        Err(error) => {
            return Err(error::Error::FailedParseCfg {
                path: path.to_owned(),
                error,
            })
        }
    };
    let branch = cfg.branch.unwrap_or("master".to_owned());
    let branch_ref = format!("refs/heads/{}", &branch);

    let filters = cfg.filters.unwrap_or(vec![]);
    let commit_msg_filters = match regex::RegexSet::new(&filters) {
        Ok(set) => set,
        Err(regex_err) => {
            return Err(error::Error::InvalidConfig {
                field: "filter",
                error: regex_err,
            });
        }
    };

    Ok(Options {
        repo: cfg.repo.unwrap_or(".".to_owned()),
        remote: cfg.remote,
        branch,
        branch_ref,
        commit_msg_filters,

        bootstrap: matches.is_present("bootstrap"),
        uproot: matches.is_present("uproot"),
        verbose: !matches.is_present("quiet"),
        yes: matches.is_present("yes"),
    })
}
