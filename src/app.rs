pub struct Options {
    pub repo: String,
    pub branch: String,
    pub remote: String,

    pub bootstrap: bool,
    pub verbose: bool,
    pub yes: bool,
}

pub fn parse_args() -> Options {
    let yaml = clap::load_yaml!("cli.yml");
    let matches = clap::App::from_yaml(yaml)
        .setting(clap::AppSettings::ColoredHelp)
        .get_matches();

    Options {
        repo: matches.value_of("repo").unwrap_or(".").to_owned(),
        branch: matches.value_of("branch").unwrap_or("master").to_owned(),
        remote: matches.value_of("remote").unwrap().to_owned(),
        bootstrap: matches.is_present("bootstrap"),
        verbose: matches.is_present("verbose"),
        yes: matches.is_present("yes"),
    }
}
