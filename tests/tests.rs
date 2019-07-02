//! Integration tests for the CLI interface of fd.

mod env;

/// Test that synchronization fails unless a boostrap is done
#[test]
fn test_bootstrap() {
    let env = env::TestEnv::new(false);

    env.remote_repo.commit_file("a.txt", "a");
    env.remote_repo.commit_file("b.txt", "b");
    assert_eq!(env.remote_repo.count_commits(), 3); // init + 2 commits

    env.run_ripit_failure(&["private"]); // missing initial commit

    env.local_repo.commit_file("priv", "priv");
    env.run_ripit_failure(&["private"]); // missing ripit tag

    env.run_ripit_success(&["--bootstrap", "private"]);
    assert_eq!(env.local_repo.count_commits(), 2); // priv + bootstrap

    // files from both remote commits were added
    env.local_repo.check_file("a.txt", true, true);
    env.local_repo.check_file("b.txt", true, true);
    // file from local commit was un-indexed, but is still present on the repo.
    env.local_repo.check_file("priv", true, false);
}

/// Test basic syncing of a few commits
#[test]
fn test_basic_sync() {
    let env = env::TestEnv::new(false);

    env.run_ripit_success(&["--bootstrap", "private"]);

    env.remote_repo.commit_file("a.txt", "a");
    env.remote_repo.commit_file("b.txt", "b");
    assert_eq!(env.remote_repo.count_commits(), 3); // init + 2 commits

    env.run_ripit_success(&["-y", "private"]); // missing initial commit

    assert_eq!(env.local_repo.count_commits(), 3); // bootstrap + 2 synced commits
    env.local_repo.check_file("a.txt", true, true);
    env.local_repo.check_file("b.txt", true, true);

    env.remote_repo.commit_file("c.txt", "c");
    env.run_ripit_success(&["-y", "private"]); // missing initial commit
    env.local_repo.check_file("c.txt", true, true);

    // check the tags are valid
    let mut remote_revwalk = env.remote_repo.revwalk().unwrap();
    remote_revwalk.push_head().unwrap();
    let mut local_revwalk = env.local_repo.revwalk().unwrap();
    local_revwalk.push_head().unwrap();

    for (remote_ci, local_ci) in remote_revwalk.zip(local_revwalk) {
        let remote_commit = env.remote_repo.find_commit(remote_ci.unwrap()).unwrap();
        let local_commit = env.local_repo.find_commit(local_ci.unwrap()).unwrap();
        let local_msg = local_commit.message().unwrap();
        let pattern = format!("rip-it: {}", remote_commit.id());

        assert!(local_msg.find(&pattern).is_some());
    }
}
