//! Integration tests for the CLI interface of fd.

mod env;

/// Test that synchronization fails if the local repo is missing a ripit tag
#[test]
fn test_missing_bootstrap() {
    let env = env::TestEnv::new(false);

    env.run_ripit_failure(&["private"]);
    env.run_ripit_success(&["--bootstrap", "private"]);
}
