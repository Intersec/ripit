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
    pub fn force_checkout_head(&self) {
        let mut opts = git2::build::CheckoutBuilder::new();
        self.checkout_head(Some(&mut opts.force())).unwrap();
    }

    pub fn do_commit(&self, msg: &str) -> git2::Commit {
        let mut index = self.index().unwrap();
        let tree = self.find_tree(index.write_tree().unwrap()).unwrap();

        let head = match self.head() {
            Ok(tgt) => Some(self.find_commit(tgt.target().unwrap()).unwrap()),
            Err(_) => None,
        };
        let sig = self.signature().unwrap();

        let commit_oid = match head {
            Some(ci) => self.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[&ci]),
            None => self.commit(Some("HEAD"), &sig, &sig, msg, &tree, &[]),
        }
        .unwrap();

        self.force_checkout_head();

        self.find_commit(commit_oid).unwrap()
    }

    fn write_and_add_file(&self, filename: &str, content: &str) {
        let path = Path::new(self.workdir().unwrap()).join(filename);
        fs::File::create(&path)
            .unwrap()
            .write_all(content.as_bytes())
            .unwrap();

        let mut index = self.index().unwrap();
        index.add_path(Path::new(filename)).unwrap();
    }

    pub fn commit_file(&self, filename: &str, commit_msg: &str) -> git2::Commit {
        self.write_and_add_file(filename, commit_msg);
        self.do_commit(commit_msg)
    }

    pub fn resolve_conflict_and_commit(&self, filename: &str) -> git2::Commit {
        // overwrite file containing conflicts, and add it to the index
        self.write_and_add_file(filename, "resolved conflict");

        // do a commit, but get the commit msg from the .git/MERGE_MSG file.
        // This is to simulate what "git commit" would do
        let path = Path::new(self.path()).join("MERGE_MSG");
        let content = std::fs::read_to_string(path).unwrap();
        self.do_commit(&content)
    }

    /// Commit a file, and tag the commit (the tag name and the files content are the same)
    fn commit_file_and_tag(&self, filename: &str, content: &str) -> git2::Commit {
        let ci = self.commit_file(filename, content);
        self.tag_lightweight(content, ci.as_object(), true).unwrap();
        ci
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

    /// Do a commit-merge of the given commit in HEAD
    fn do_merge(&self, theirs: &git2::Commit, content: &str) -> git2::Commit {
        let annotated_theirs = self.find_annotated_commit(theirs.id()).unwrap();
        self.merge(&[&annotated_theirs], None, None).unwrap();

        let mut index = self.index().unwrap();

        /* resolve conflicts with dummy content.
         * Only changes in existing file is handled for this routine.
         */
        let mut resolved_paths = Vec::new();
        for conflict in index.conflicts().unwrap() {
            let filepath = conflict.unwrap().our.unwrap().path;
            let strpath = std::str::from_utf8(&filepath).unwrap();
            let abspath = Path::new(self.workdir().unwrap()).join(strpath);

            /* overwrite file with dummy content */
            fs::File::create(&abspath)
                .unwrap()
                .write_all(b"resolved conflict!")
                .unwrap();

            /* must accumulate the paths, we cannot call add_path and modify the index
             * while we iterate on it */
            resolved_paths.push(filepath);
        }
        for filepath in resolved_paths {
            let path = std::str::from_utf8(&filepath).unwrap();
            index.add_path(Path::new(path)).unwrap();
        }

        let tree = self.find_tree(index.write_tree().unwrap()).unwrap();

        let head = self.head().unwrap().target().unwrap();
        let head_ci = self.find_commit(head).unwrap();
        let sig = self.signature().unwrap();

        let commit_oid = self
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                content,
                &tree,
                &[&head_ci, &theirs],
            )
            .unwrap();
        let ci = self.find_commit(commit_oid).unwrap();
        self.tag_lightweight(content, ci.as_object(), true).unwrap();

        self.force_checkout_head();
        ci
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
        let url = remote_dir.path().to_str().unwrap();
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

    /// Setup multiple branches with conflicting commits and merges.
    ///
    ///                     --> C6 --> C7 -   // feature branch merged in master
    ///                    /               \
    /// C1 -> C2 -> C3 -> C4 -> C5 ---------> C8 (master)
    ///  \     \
    ///   \     --> C9 -------> C10 (branch1) // second stable branch
    ///    \                /
    ///     -> C11 -> C12 -- (branch0) // stable branch
    ///
    /// C12 conflicts with C9
    ///
    pub fn setup_branches(&self) {
        let c1 = self.remote_repo.commit_file_and_tag("c1", "c1");
        let c2 = self.remote_repo.commit_file_and_tag("c2", "c2");
        self.remote_repo.commit_file_and_tag("c3", "c3");
        let c4 = self.remote_repo.commit_file_and_tag("c4", "c4");

        self.remote_repo.commit_file_and_tag("c6", "c6");
        let c7 = self.remote_repo.commit_file_and_tag("c7", "c7");

        self.remote_repo
            .reset(c4.as_object(), git2::ResetType::Hard, None)
            .unwrap();
        self.remote_repo.commit_file_and_tag("c5", "c5");
        self.remote_repo.do_merge(&c7, "c8");

        self.remote_repo.branch("branch0", &c1, true).unwrap();
        self.remote_repo.set_head("refs/heads/branch0").unwrap();
        self.remote_repo.force_checkout_head();

        self.remote_repo.commit_file_and_tag("c11", "c11");
        let c12 = self.remote_repo.commit_file_and_tag("c12", "c12");

        self.remote_repo.branch("branch1", &c2, true).unwrap();
        self.remote_repo.set_head("refs/heads/branch1").unwrap();
        self.remote_repo.force_checkout_head();
        // have c9 conflict with c12 by writing in the same file, with a different content
        self.remote_repo.commit_file_and_tag("c12", "c9");
        self.remote_repo.do_merge(&c12, "c10");

        self.remote_repo.set_head("refs/heads/master").unwrap();
        self.remote_repo.force_checkout_head();
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
