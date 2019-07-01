use std::env;
use std::path::PathBuf;
use std::process;
use std::str;

// {{{ ripit exec handling

fn find_ripit_exec() -> PathBuf {
    // Tests exe is in target/debug/deps, the *ripit* exe is in target/debug
    let root = env::current_exe()
        .expect("tests executable")
        .parent()
        .expect("tests executable directory")
        .parent()
        .expect("fd executable directory")
        .to_path_buf();

    root.join("ripit")
}

// }}}
// {{{ Test environment setup

pub struct TestEnv {
    // temporary directory containing the git repo to sync
    local_dir: tempfile::TempDir,
    // the git repo to sync with its remote
    local_repo: git2::Repository,

    // temporary directory containing the remote repo
    remote_dir: tempfile::TempDir,
    // the git repo to sync with its remote
    remote_repo: git2::Repository,

    // path to ripit executable
    ripit_exec: PathBuf,
}

impl TestEnv {
    /// Create a new git repo in a tmp directory
    pub fn new(first_commit_in_local: bool) -> Self {
        // git init in tmp directory for remote
        let remote_dir = tempfile::tempdir().unwrap();
        let remote_repo = git2::Repository::init(remote_dir.path()).unwrap();

        // git init in tmp directory for remote
        let local_dir = tempfile::tempdir().unwrap();
        let local_repo = git2::Repository::init(local_dir.path()).unwrap();

        // Setup remote repo as remote named "private" of local repo
        println!("{:?}", remote_dir.path());
        let url = format!("file://{}", remote_dir.path().to_str().unwrap());
        local_repo.remote("private", &url).unwrap();

        for repo in &[&remote_repo, &local_repo] {
            // Set up default cfg
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Foo").unwrap();
            config.set_str("user.email", "Bar").unwrap();
        }

        let repos = if first_commit_in_local {
            vec![&remote_repo, &local_repo]
        } else {
            vec![&remote_repo]
        };
        for repo in &repos {
            // Create initial commit
            let tree_oid = repo.index().unwrap().write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = repo.signature().unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[]).unwrap();
        }

        TestEnv {
            local_dir,
            local_repo,
            remote_dir,
            remote_repo,
            ripit_exec: find_ripit_exec(),
        }
    }

    fn run_ripit(&self, successful: bool, args: &[&str]) {
        let mut cmd = process::Command::new(&self.ripit_exec);
        cmd.current_dir(self.local_dir.path());
        cmd.args(args);

        let output = cmd.output().expect("ripit command");
        println!("stdout: {}", str::from_utf8(&output.stdout).unwrap());
        println!("stderr: {}", str::from_utf8(&output.stderr).unwrap());

        assert!(output.status.success() == successful);
    }

    pub fn run_ripit_failure(&self, args: &[&str]) {
        self.run_ripit(false, args)
    }

    pub fn run_ripit_success(&self, args: &[&str]) {
        self.run_ripit(true, args)
    }
}

// }}}
