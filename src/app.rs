use crate::error;
use serde::Deserialize;

pub struct Branch {
    // name of the branch to synchronize
    pub name: String,
    // full ref name for the local branch
    pub refname: String,
}

pub struct Options {
    // path to the local repo
    pub repo: String,
    // name of the remote to synchronize from
    pub remote: String,

    // branches to synchronize
    pub branches: Vec<Branch>,

    pub commit_msg_filters: regex::RegexSet,

    pub bootstrap: bool,
    pub uproot: bool,
    pub verbose: bool,
    pub yes: bool,
    pub fetch: bool,
}

#[derive(Deserialize)]
struct YamlCfg {
    repo: Option<String>,
    remote: String,
    // TODO:  add uproot option per branch
    branch: Option<String>,
    branches: Option<Vec<String>>,
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
    // backward compatibility on legacy branch option
    let branch = cfg.branch.unwrap_or("master".to_owned());
    let mut branches = cfg.branches.unwrap_or(vec![]);
    if branches.len() == 0 {
        branches.push(branch);
    }
    let branches = branches
        .into_iter()
        .map(|name| {
            let refname = format!("refs/heads/{}", name);
            Branch { name, refname }
        })
        .collect();

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
        branches,
        commit_msg_filters,

        bootstrap: matches.is_present("bootstrap"),
        uproot: matches.is_present("uproot"),
        verbose: !matches.is_present("quiet"),
        yes: matches.is_present("yes"),
        fetch: !matches.is_present("nofetch"),
    })
}
