use crate::error::Error;
use crate::tag;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

pub struct SyncedCommit<'a> {
    pub commit: git2::Commit<'a>,
    pub uprooted: bool,
}

type Map<'a> = HashMap<git2::Oid, SyncedCommit<'a>>;

pub struct CommitsMap<'a> {
    // map of Oid in remote repo to Commit in local repo
    map: Map<'a>,
}

impl<'a> CommitsMap<'a> {
    pub fn new() -> Self {
        Self { map: Map::new() }
    }

    pub fn fill_from_commit(
        &mut self,
        repo: &'a git2::Repository,
        commit_id: git2::Oid,
    ) -> Result<(), Error> {
        // build revwalk from the first commit of the repo up to the provided commit
        let mut revwalk = repo.revwalk()?;
        revwalk.push(commit_id)?;

        for oid in revwalk {
            let oid = oid?;
            let commit = repo.find_commit(oid)?;

            // a commit missing a tag could be an error too. By ignoring it, it will lead to errors
            // if it is a parent of a commit to sync.
            let (tag, uprooted) = match tag::retrieve_ripit_tag(&commit) {
                Some(tag) => tag,
                None => continue,
            };
            let remote_oid = git2::Oid::from_str(&tag)?;

            if !self.insert(remote_oid, SyncedCommit { commit, uprooted }) {
                // entry was already in the map, no need to continue
                break;
            }
        }

        Ok(())
    }

    pub fn contains_key(&self, oid: git2::Oid) -> bool {
        self.map.contains_key(&oid)
    }

    pub fn get(&self, oid: git2::Oid) -> Option<&SyncedCommit> {
        self.map.get(&oid)
    }

    pub fn insert(&mut self, oid: git2::Oid, val: SyncedCommit<'a>) -> bool {
        match self.map.entry(oid) {
            Entry::Occupied(_) => false,
            Entry::Vacant(v) => {
                v.insert(val);
                true
            }
        }
    }
}
