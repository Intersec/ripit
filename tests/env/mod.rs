use std::env;
use std::fs;
use std::io::Write;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;
use std::process;
use std::str;

// to use to pause  the execution, so that the states of the test repos can be checked
pub fn _pause() {
    use std::io::Read;

    let mut stdout = std::io::stdout();
    stdout.write(b"Press Enter to continue...").unwrap();
    stdout.flush().unwrap();
    std::io::stdin().read(&mut [0]).unwrap();
}

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
// {{{ Test repo

pub struct TestRepo(git2::Repository);

impl Deref for TestRepo {
    type Target = git2::Repository;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TestRepo {
    pub fn commit_file(&self, filename: &str, commit_msg: &str) {
        let path = Path::new(self.workdir().unwrap()).join(filename);
        fs::File::create(&path)
            .unwrap()
            .write_all(filename.as_bytes())
            .unwrap();

        let mut index = self.index().unwrap();
        index.add_path(Path::new(filename)).unwrap();
        let tree = self.find_tree(index.write_tree().unwrap()).unwrap();

        let head = match self.head() {
            Ok(tgt) => Some(self.find_commit(tgt.target().unwrap()).unwrap()),
            Err(_) => None,
        };
        let sig = self.signature().unwrap();

        match head {
            Some(ci) => self.commit(Some("HEAD"), &sig, &sig, commit_msg, &tree, &[&ci]),
            None => self.commit(Some("HEAD"), &sig, &sig, commit_msg, &tree, &[]),
        }
        .unwrap();

        let mut opts = git2::build::CheckoutBuilder::new();
        self.checkout_head(Some(&mut opts.force())).unwrap();
    }

    pub fn check_file(&self, filename: &str, file_present: bool, file_in_index: bool) {
        let path = Path::new(self.workdir().unwrap()).join(filename);
        assert_eq!(path.exists(), file_present);

        self.index().unwrap().read(true).unwrap();
        let index_elem = self.index().unwrap().get_path(Path::new(filename), 0);
        assert_eq!(index_elem.is_some(), file_in_index);
    }

    pub fn count_commits(&self) -> usize {
        let mut revwalk = self.revwalk().unwrap();
        revwalk.push_head().unwrap();

        revwalk.count()
    }
}

// }}}
// {{{ Test environment setup

pub struct TestEnv {
    // temporary directories containing the git repos to sync
    local_dir: tempfile::TempDir,
    _remote_dir: tempfile::TempDir,

    // the git repo to sync with its remote
    pub local_repo: TestRepo,
    // the git repo holding commits to sync
    pub remote_repo: TestRepo,

    // path to ripit executable
    ripit_exec: PathBuf,
}

impl TestEnv {
    /// Create a new git repo in a tmp directory
    pub fn new(first_commit_in_local: bool) -> Self {
        // git init in tmp directory for remote
        let remote_dir = tempfile::tempdir().unwrap();
        println!("remote dir: {:?}", remote_dir);
        let remote_repo = TestRepo(git2::Repository::init(remote_dir.path()).unwrap());

        // git init in tmp directory for remote
        let local_dir = tempfile::tempdir().unwrap();
        println!("local dir: {:?}", local_dir);
        let local_repo = TestRepo(git2::Repository::init(local_dir.path()).unwrap());

        // Setup remote repo as remote named "private" of local repo
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
            repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
                .unwrap();
        }

        TestEnv {
            local_dir,
            local_repo,
            _remote_dir: remote_dir,
            remote_repo,
            ripit_exec: find_ripit_exec(),
        }
    }

    fn run_ripit(&self, successful: bool, args: &[&str], err_msg: Option<&str>) {
        let mut cmd = process::Command::new(&self.ripit_exec);
        cmd.current_dir(self.local_dir.path());
        cmd.args(args);

        let output = cmd.output().expect("ripit command");
        println!("stdout: {}", str::from_utf8(&output.stdout).unwrap());

        let stderr = str::from_utf8(&output.stderr).unwrap();
        if let Some(msg) = err_msg {
            assert!(stderr.contains(msg));
        }
        println!("stderr: {}", str::from_utf8(&output.stderr).unwrap());

        assert!(output.status.success() == successful);

        // reload index for both repos, as the execution might have changed the state of the
        // repo
        self.local_repo.index().unwrap().read(true).unwrap();
        self.remote_repo.index().unwrap().read(true).unwrap();
    }

    pub fn run_ripit_failure(&self, args: &[&str], err_msg: Option<&str>) {
        self.run_ripit(false, args, err_msg)
    }

    pub fn run_ripit_success(&self, args: &[&str]) {
        self.run_ripit(true, args, None)
    }
}

// }}}
