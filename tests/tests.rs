//! Integration tests for the CLI interface of fd.

use std::fs;
use std::path::Path;

mod env;

/// Test that synchronization fails unless a boostrap is done
#[test]
fn test_bootstrap() {
    let env = env::TestEnv::new(false);

    env.remote_repo.commit_file("a.txt", "a");
    env.remote_repo.commit_file("b.txt", "b");
    assert_eq!(env.remote_repo.count_commits(), 3); // init + 2 commits

    env.run_ripit_failure(&["private"], None); // missing initial commit

    env.local_repo.commit_file("priv", "priv");
    env.run_ripit_failure(&["private"], None); // missing ripit tag

    env.run_ripit_success(&["--bootstrap", "private"]);
    assert_eq!(env.local_repo.count_commits(), 2); // priv + bootstrap

    // files from both remote commits were added
    env.local_repo.check_file("a.txt", true, true);
    env.local_repo.check_file("b.txt", true, true);
    env.local_repo.check_file("priv", false, false);
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

        assert!(local_msg.contains(&pattern));
    }
}

/// Test that exec is aborted if local changes are present
#[test]
fn test_abort_on_local_changes() {
    let env = env::TestEnv::new(false);
    let mut opts = git2::build::CheckoutBuilder::new();
    opts.force();

    let filename = "local.txt";
    env.local_repo.commit_file(filename, "local");
    let path = Path::new(env.local_repo.workdir().unwrap()).join(filename);

    // bootstrap should fail due to local changes
    fs::remove_file(&path).unwrap();
    env.run_ripit_failure(&["--bootstrap", "private"], Some("Aborted"));

    // force checkout, bootstrap should succeed
    env.local_repo.checkout_head(Some(&mut opts)).unwrap();
    env.remote_repo.commit_file("a.txt", "a");
    env.run_ripit_success(&["--bootstrap", "private"]);

    // sync should fail due to local changes
    let path = Path::new(env.local_repo.workdir().unwrap()).join("a.txt");
    fs::remove_file(&path).unwrap();
    env.run_ripit_failure(&["private"], Some("Aborted"));

    env.local_repo.checkout_head(Some(&mut opts)).unwrap();
    env.run_ripit_success(&["-y", "private"]);
}

/// Test filtering of commits
#[test]
fn test_commits_filtering() {
    let env = env::TestEnv::new(false);

    env.run_ripit_success(&["--bootstrap", "private"]);

    let c1 = env.remote_repo.commit_file(
        "a.txt",
        "\
brief

test line 1
Toto Test Refs

tt test",
    );
    let c2 = env.remote_repo.commit_file(
        "b.txt",
        "\
Not even a brief
Refs:
 Refs: b",
    );
    assert_eq!(env.remote_repo.count_commits(), 3); // init + 2 commits

    env.run_ripit_success(&["-vy", "-C", "^Refs", "-C", "test", "private"]);

    let mut revwalk = env.local_repo.revwalk().unwrap();
    revwalk.push_head().unwrap();
    let commits: Vec<git2::Commit> = revwalk
        .map(|oid| env.local_repo.find_commit(oid.unwrap()).unwrap())
        .collect();
    assert_eq!(commits.len(), 3);

    assert_eq!(
        commits[0].message().unwrap(),
        format!(
            "\
Not even a brief
 Refs: b
rip-it: {}
",
            c2.id()
        )
    );
    assert_eq!(
        commits[1].message().unwrap(),
        format!(
            "\
brief

Toto Test Refs

rip-it: {}
",
            c1.id()
        )
    );
}
