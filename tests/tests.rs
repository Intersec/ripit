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

    let filename = "local.txt";
    env.local_repo.commit_file(filename, "local");
    let path = Path::new(env.local_repo.workdir().unwrap()).join(filename);

    // bootstrap should fail due to local changes
    fs::remove_file(&path).unwrap();
    env.run_ripit_failure(&["--bootstrap", "private"], Some("Aborted"));

    // force checkout, bootstrap should succeed
    env.local_repo.force_checkout_head();
    env.remote_repo.commit_file("a.txt", "a");
    env.run_ripit_success(&["--bootstrap", "private"]);

    // sync should fail due to local changes
    let path = Path::new(env.local_repo.workdir().unwrap()).join("a.txt");
    fs::remove_file(&path).unwrap();
    env.run_ripit_failure(&["private"], Some("Aborted"));

    env.local_repo.force_checkout_head();
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

/// Test syncing of a merge commit
#[test]
fn test_merge_sync() {
    let env = env::TestEnv::new(false);
    env.setup_branches();

    // start syncing from c4
    let c4 = env.remote_repo.revparse_single("c4").unwrap();
    env.remote_repo
        .reset(&c4, git2::ResetType::Hard, None)
        .unwrap();
    env.run_ripit_success(&["--bootstrap", "private"]);

    // then sync c8: should reproduce the merge commit
    let c8 = env.remote_repo.revparse_single("c8").unwrap();
    env.remote_repo
        .reset(&c8, git2::ResetType::Hard, None)
        .unwrap();
    env.run_ripit_success(&["-y", "private"]);

    env.local_repo.check_file("c4", true, true);
    env.local_repo.check_file("c5", true, true);
    env.local_repo.check_file("c6", true, true);
    env.local_repo.check_file("c7", true, true);

    let head_tgt = env.local_repo.head().unwrap().target().unwrap();
    let head_ci = env.local_repo.find_commit(head_tgt).unwrap();

    assert!(head_ci.summary().unwrap().contains("c8"));
    let parents: Vec<git2::Commit> = head_ci.parents().collect();
    assert_eq!(parents.len(), 2);
    assert!(parents[0].summary().unwrap().contains("c5"));
    assert!(parents[1].summary().unwrap().contains("c7"));

    let parents: Vec<git2::Commit> = parents[1].parents().collect();
    assert_eq!(parents.len(), 1);
    assert!(parents[0].summary().unwrap().contains("c6"));

    let parents: Vec<git2::Commit> = parents[0].parents().collect();
    assert_eq!(parents.len(), 1);
    assert!(parents[0]
        .summary()
        .unwrap()
        .contains("Bootstrap repository"));
}
