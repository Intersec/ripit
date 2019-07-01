//! Integration tests for the CLI interface of fd.

mod env;

/// Test that synchronization fails if the local repo is missing a ripit tag
#[test]
fn test_missing_bootstrap() {
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
