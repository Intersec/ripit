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

    env.run_ripit_failure(&[], None); // missing initial commit

    env.local_repo.commit_file("priv", "priv");
    env.run_ripit_failure(&[], None); // missing ripit tag

    env.run_ripit_success(&["--bootstrap"]);
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

    env.run_ripit_success(&["--bootstrap"]);

    env.remote_repo.commit_file("a.txt", "a");
    env.remote_repo.commit_file("b.txt", "b");
    assert_eq!(env.remote_repo.count_commits(), 3); // init + 2 commits

    env.run_ripit_success(&["-y"]); // missing initial commit

    // head tracks master
    let local_head = env.local_repo.head().unwrap();
    assert!(local_head.is_branch(), true);
    assert_eq!(local_head.shorthand().unwrap(), "master");

    assert_eq!(env.local_repo.count_commits(), 3); // bootstrap + 2 synced commits
    env.local_repo.check_file("a.txt", true, true);
    env.local_repo.check_file("b.txt", true, true);

    env.remote_repo.commit_file("c.txt", "c");
    env.run_ripit_success(&["-y"]); // missing initial commit
    env.local_repo.check_file("c.txt", true, true);

    // head tracks master
    let local_head = env.local_repo.head().unwrap();
    assert!(local_head.is_branch(), true);
    assert_eq!(local_head.shorthand().unwrap(), "master");

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
    env.run_ripit_failure(&["--bootstrap"], Some("Aborted"));

    // force checkout, bootstrap should succeed
    env.local_repo.force_checkout_head();
    env.remote_repo.commit_file("a.txt", "a");
    env.run_ripit_success(&["--bootstrap"]);

    // sync should fail due to local changes
    let path = Path::new(env.local_repo.workdir().unwrap()).join("a.txt");
    fs::remove_file(&path).unwrap();
    env.run_ripit_failure(&[], Some("Aborted"));

    env.local_repo.force_checkout_head();
    env.run_ripit_success(&["-y"]);
}

/// Test filtering of commits
#[test]
fn test_commits_filtering() {
    let env = env::TestEnv::new(false);

    env.run_ripit_success(&["--bootstrap"]);

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

    env.run_ripit_success(&["-y"]);

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
    env.remote_repo.reset_hard(&c4);
    env.run_ripit_success(&["--bootstrap"]);

    // then sync c8: should reproduce the merge commit
    let c8 = env.remote_repo.revparse_single("c8").unwrap();
    env.remote_repo.reset_hard(&c8);
    env.run_ripit_success(&["-y"]);

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

/// Test uprooting of commits with unknown parents
#[test]
fn test_uproot_sync() {
    let env = env::TestEnv::new(false);
    env.setup_branches();

    // start syncing from c5
    let c5 = env.remote_repo.revparse_single("c5").unwrap();
    env.remote_repo.reset_hard(&c5);
    env.run_ripit_success(&["--bootstrap"]);

    // then try to sync c8: should fail because of unknown parent
    let c8 = env.remote_repo.revparse_single("c8").unwrap();
    env.remote_repo.reset_hard(&c8);
    env.run_ripit_failure(&["-y"], Some("cannot be found in the local repository"));

    // sync c8 with uprooting, should work
    env.run_ripit_success(&["-yu"]);

    let head_tgt = env.local_repo.head().unwrap().target().unwrap();
    let head_ci = env.local_repo.find_commit(head_tgt).unwrap();

    assert!(head_ci.summary().unwrap().contains("c8"));
    assert!(!head_ci.message().unwrap().contains("uprooted"));

    let parents: Vec<git2::Commit> = head_ci.parents().collect();
    assert_eq!(parents.len(), 2);
    assert!(parents[0].summary().unwrap().contains("Bootstrap"));
    assert!(parents[1].summary().unwrap().contains("c7"));
    assert!(parents[1].message().unwrap().contains("uprooted"));

    let parents: Vec<git2::Commit> = parents[1].parents().collect();
    assert_eq!(parents.len(), 1);
    assert!(parents[0].summary().unwrap().contains("c6"));
    assert!(parents[0].message().unwrap().contains("uprooted"));

    let parents: Vec<git2::Commit> = parents[0].parents().collect();
    assert_eq!(parents.len(), 1);
    assert!(parents[0].summary().unwrap().contains("Bootstrap"));
}

/// Test uprooting with conflicts
#[test]
fn test_uproot_sync_with_conflicts() {
    let env = env::TestEnv::new(false);
    env.setup_branches();

    // start syncing from c9
    let c9 = env.remote_repo.revparse_single("c9").unwrap();
    env.remote_repo.reset_hard(&c9);
    env.run_ripit_success(&["--bootstrap"]);

    // then sync c10: should try to reproduce the merge by cherry-picking the unknown commits.
    // As there is a conflict, the sync should fail
    let c10 = env.remote_repo.revparse_single("c10").unwrap();
    env.remote_repo.reset_hard(&c10);
    env.run_ripit_failure(&["-yu"], Some("due to conflicts"));

    // Resolve conflict and do a commit
    env.local_repo.resolve_conflict_and_commit("c12");

    // check the committed file contains a rip-it tag
    let head_tgt = env.local_repo.head().unwrap().target().unwrap();
    let head_ci = env.local_repo.find_commit(head_tgt).unwrap();
    assert!(head_ci.message().unwrap_or("").contains("rip-it:"));

    // check the committed file does not contained filter out lines.
    // As the commit was done by the user, the filtering process is different than
    // when syncing.
    assert!(!head_ci.message().unwrap_or("").contains("test"));

    // Go-on with the synchronization, now that the conflict is
    // solved.
    env.run_ripit_success(&["-yu"]);

    let head_tgt = env.local_repo.head().unwrap().target().unwrap();
    let head_ci = env.local_repo.find_commit(head_tgt).unwrap();

    assert!(head_ci.summary().unwrap().contains("c10"));
    assert!(!head_ci.message().unwrap().contains("uprooted"));

    let parents: Vec<git2::Commit> = head_ci.parents().collect();
    assert_eq!(parents.len(), 2);
    assert!(parents[0].summary().unwrap().contains("Bootstrap"));
    assert!(parents[1].summary().unwrap().contains("c12"));
    assert!(parents[1].message().unwrap().contains("uprooted"));

    let parents: Vec<git2::Commit> = parents[1].parents().collect();
    assert_eq!(parents.len(), 1);
    assert!(parents[0].summary().unwrap().contains("c11"));
    assert!(parents[0].message().unwrap().contains("uprooted"));

    let parents: Vec<git2::Commit> = parents[0].parents().collect();
    assert_eq!(parents.len(), 1);
    assert!(parents[0].summary().unwrap().contains("Bootstrap"));
}

/// Test uproot of merge commit with an unknown parent
///
/// Make sure that if we reach a merge commit with an unknown parent, it is
/// properly uprooted with the proper mainline
///
///             --> C3 --
///            /         \
///    --> C1 -------------> C4 ---
///   /        \                   \
/// C0 ----------> C2 (bootstrap) ----> C5 (sync)
///
#[test]
fn test_uproot_merge() {
    let env = env::TestEnv::new(false);
    env.setup_merge_uproot(false);

    // start syncing from c2
    let c2 = env.remote_repo.revparse_single("c2").unwrap();
    env.remote_repo.reset_hard(&c2);
    env.run_ripit_success(&["--bootstrap"]);

    // then sync c5, should uproot C4 which is a merge commit
    let c5 = env.remote_repo.revparse_single("c5").unwrap();
    env.remote_repo.reset_hard(&c5);
    env.run_ripit_success(&["-yu"]);

    let head_tgt = env.local_repo.head().unwrap().target().unwrap();
    let head_ci = env.local_repo.find_commit(head_tgt).unwrap();

    assert!(head_ci.summary().unwrap().contains("c5"));
    assert!(!head_ci.message().unwrap().contains("uprooted"));

    // master should point to C5
    let local_head = env.local_repo.head().unwrap();
    assert!(local_head.is_branch(), true);
    assert_eq!(local_head.shorthand().unwrap(), "master");

    let parents: Vec<git2::Commit> = head_ci.parents().collect();
    assert_eq!(parents.len(), 2);
    assert!(parents[0].summary().unwrap().contains("Bootstrap"));
    assert!(parents[1].summary().unwrap().contains("c4"));
    assert!(parents[1].message().unwrap().contains("uprooted"));

    let parents: Vec<git2::Commit> = parents[1].parents().collect();
    assert_eq!(parents.len(), 1);
    assert!(parents[0].summary().unwrap().contains("c3"));
    assert!(parents[0].message().unwrap().contains("uprooted"));

    let parents: Vec<git2::Commit> = parents[0].parents().collect();
    assert_eq!(parents.len(), 1);
    assert!(parents[0].summary().unwrap().contains("Bootstrap"));
}

/// Test resync from uprooted merge
///
/// When syncing from an uprooted merge, the comparison of the list of commits
/// to sync will include commits from both parents of the merge. They must
/// be properly considered as already synced.
///
///             --> C3 --
///            /         \
///    --> C1 ----> CX ----> C4 ---
///   /        \                   \
/// C0 ----------> C2 (bootstrap) ----> C5 (sync)
///
#[test]
fn test_resync_uprooted_merge() {
    let env = env::TestEnv::new(false);
    env.setup_merge_uproot(true);

    // start syncing from c2
    let c2 = env.remote_repo.revparse_single("c2").unwrap();
    env.remote_repo.reset_hard(&c2);
    env.run_ripit_success(&["--bootstrap"]);

    // then sync c5, should uproot C4 which is a merge commit, but keep it as a merge commit
    let c5 = env.remote_repo.revparse_single("c5").unwrap();
    env.remote_repo.reset_hard(&c5);
    env.run_ripit_success(&["-yu"]);

    let head_tgt = env.local_repo.head().unwrap().target().unwrap();
    let head_ci = env.local_repo.find_commit(head_tgt).unwrap();
    assert!(head_ci.summary().unwrap().contains("c5"));
    assert!(!head_ci.message().unwrap().contains("uprooted"));

    let parents: Vec<git2::Commit> = head_ci.parents().collect();
    assert_eq!(parents.len(), 2);
    assert!(parents[0].summary().unwrap().contains("Bootstrap"));
    assert!(parents[1].summary().unwrap().contains("c4"));
    assert!(parents[1].message().unwrap().contains("uprooted"));
    let c4 = &parents[1];

    let parents: Vec<git2::Commit> = parents[1].parents().collect();
    assert_eq!(parents.len(), 2);
    assert!(parents[0].summary().unwrap().contains("cx"));
    assert!(parents[0].message().unwrap().contains("uprooted"));
    assert!(parents[1].summary().unwrap().contains("c3"));
    assert!(parents[1].message().unwrap().contains("uprooted"));

    // when syncing from C4, (as if a conflict was present in C4), we should not
    // consider C3 as to be synced.
    env.local_repo.set_head_detached(c4.id()).unwrap();
    env.local_repo.branch("master", &c4, true).unwrap();
    env.run_ripit_success(&["-y"]);

    // The sync will have created a new C5
    let head_tgt = env.local_repo.head().unwrap().target().unwrap();
    let head_ci = env.local_repo.find_commit(head_tgt).unwrap();
    assert!(head_ci.summary().unwrap().contains("c5"));
    assert!(!head_ci.message().unwrap().contains("uprooted"));
}

/// Test sync of merge solving conflicts
#[test]
fn test_merge_solving_conflicts() {
    let env = env::TestEnv::new(false);
    env.setup_merge_solving_conflicts();

    let c0 = env.remote_repo.revparse_single("c0").unwrap();
    env.remote_repo.reset_hard(&c0);
    env.run_ripit_success(&["--bootstrap"]);

    // then sync c3, should sync all commits properly, without issues
    let c3 = env.remote_repo.revparse_single("c3").unwrap();
    env.remote_repo.reset_hard(&c3);
    env.run_ripit_success(&["-y"]);

    let head_tgt = env.local_repo.head().unwrap().target().unwrap();
    let head_ci = env.local_repo.find_commit(head_tgt).unwrap();
    assert!(head_ci.summary().unwrap().contains("c3"));

    let parents: Vec<git2::Commit> = head_ci.parents().collect();
    assert_eq!(parents.len(), 2);
    assert!(parents[0].summary().unwrap().contains("c2"));
    assert!(parents[1].summary().unwrap().contains("c1"));

    let parents0: Vec<git2::Commit> = parents[0].parents().collect();
    assert_eq!(parents0.len(), 1);
    assert!(parents0[0].summary().unwrap().contains("Bootstrap"));

    let parents1: Vec<git2::Commit> = parents[1].parents().collect();
    assert_eq!(parents1.len(), 1);
    assert!(parents1[0].summary().unwrap().contains("Bootstrap"));
}

/// Test uproot of merge with conflicts
///
/// Test the behavior when uprooting a merge commit bring conflicts.
///
/// Remote is:
///        -> C1 --
///       /        \
///      ---> C2 -----> C3 -
///     /                   \
///   C0 -------> C4 ---------> C5
///
/// Bootstrap is on C4, then sync on C5. Uprooting C3 will bring conflicts.
/// End result should be:
///             -> C1 --
///            /        \
///      ---> C2 ---------> C3 -
///     /                       \
///    B --------------------------> C5
#[test]
fn test_uproot_merge_with_conflicts() {
    let env = env::TestEnv::new(false);
    env.setup_merge_solving_conflicts();

    let c4 = env.remote_repo.revparse_single("c4").unwrap();
    env.remote_repo.reset_hard(&c4);
    env.run_ripit_success(&["--bootstrap"]);

    let c5 = env.remote_repo.revparse_single("c5").unwrap();
    env.remote_repo.reset_hard(&c5);

    // conflicts on C2
    env.run_ripit_failure(&["-yu"], Some("due to conflicts"));
    env.local_repo.resolve_conflict_and_commit("c1");

    // conflicts on C1
    env.run_ripit_failure(&["-yu"], Some("due to conflicts"));
    env.local_repo.resolve_conflict_and_commit("c1");

    // conflicts on C3
    env.run_ripit_failure(&["-yu"], Some("due to conflicts"));
    env.local_repo.resolve_conflict_and_commit("c1");

    // sync C5
    env.run_ripit_success(&["-y"]);

    let head_tgt = env.local_repo.head().unwrap().target().unwrap();
    let head_ci = env.local_repo.find_commit(head_tgt).unwrap();
    assert!(head_ci.summary().unwrap().contains("c5"));

    let parents: Vec<git2::Commit> = head_ci.parents().collect();
    assert_eq!(parents.len(), 2);
    assert!(parents[0].summary().unwrap().contains("Bootstrap"));
    assert!(parents[1].summary().unwrap().contains("c3"));

    let parents: Vec<git2::Commit> = parents[1].parents().collect();
    assert_eq!(parents.len(), 2);
    assert!(parents[0].summary().unwrap().contains("c2"));
    assert!(parents[1].summary().unwrap().contains("c1"));

    let parents: Vec<git2::Commit> = parents[1].parents().collect();
    assert_eq!(parents.len(), 1);
    assert!(parents[0].summary().unwrap().contains("c2"));

    let parents: Vec<git2::Commit> = parents[0].parents().collect();
    assert_eq!(parents.len(), 1);
    assert!(parents[0].summary().unwrap().contains("Bootstrap"));
}
