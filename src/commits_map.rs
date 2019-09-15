use crate::error::Error;
use crate::tag;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::BufRead;
use std::io::Write;
use std::path::PathBuf;

pub struct SyncedCommit<'a> {
    pub commit: git2::Commit<'a>,
    pub uprooted: bool,
}

type Map<'a> = HashMap<git2::Oid, SyncedCommit<'a>>;

pub struct CommitsMap<'a> {
    // map of Oid in remote repo to Commit in local repo
    map: Map<'a>,

    cache_file: std::fs::File,
}

impl<'a> CommitsMap<'a> {
    pub fn new(repo: &'a git2::Repository) -> Result<Self, Error> {
        // FIXME: reject bare repositories
        let filename = repo.workdir().unwrap().join(".ripit-cache");
        let mut map = Map::new();

        // fill map from cache file
        match std::fs::File::open(&filename) {
            Ok(f) => fill_map_from_cache_file(&mut map, f, repo, &filename)?,
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => (),
                _ => return Err(Error::CacheOpenError { err, filename }),
            },
        };

        // open cache file for writing
        let mut opts = std::fs::OpenOptions::new();
        opts.create(true).append(true);
        let cache_file = match opts.open(&filename) {
            Ok(f) => f,
            Err(err) => return Err(Error::CacheOpenError { err, filename }),
        };

        Ok(Self { map, cache_file })
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
            Entry::Occupied(_) => return false,
            Entry::Vacant(v) => {
                write_id_in_cache_file(&mut self.cache_file, val.commit.id());
                v.insert(val);
                true
            }
        }
    }
}

fn write_id_in_cache_file(file: &mut std::fs::File, id: git2::Oid) {
    if let Err(err) = writeln!(file, "{}", id) {
        eprintln!("error when writing in cache file: {}", err);
    }
}

fn fill_map_from_cache_file<'a>(
    map: &mut Map<'a>,
    file: std::fs::File,
    repo: &'a git2::Repository,
    filename: &PathBuf,
) -> Result<(), Error> {
    let reader = std::io::BufReader::new(&file);
    let mut line_number = 0;

    for line in reader.lines() {
        line_number += 1;
        let line = match line {
            Ok(line) => line,
            Err(err) => {
                return Err(Error::CacheReadError {
                    err,
                    filename: filename.to_owned(),
                });
            }
        };

        match parse_cache_mapping(&line, repo) {
            Ok((remote_oid, commit)) => {
                map.insert(remote_oid, commit);
            }
            Err(desc) => {
                return Err(Error::CacheInvalidLine {
                    desc,
                    filename: filename.to_owned(),
                    line,
                    line_number,
                })
            }
        };
    }

    Ok(())
}

fn parse_cache_mapping<'a>(
    line: &str,
    repo: &'a git2::Repository,
) -> Result<(git2::Oid, SyncedCommit<'a>), String> {
    let commit = match commit_from_mapping(line, repo) {
        Ok(ci) => ci,
        Err(e) => return Err(e.message().to_owned()),
    };
    let (tag, uprooted) = match tag::retrieve_ripit_tag(&commit) {
        Some(tag) => tag,
        None => return Err("Commit does not have a ripit tag".to_owned()),
    };

    let remote_oid = match git2::Oid::from_str(&tag) {
        Ok(oid) => oid,
        Err(e) => return Err(e.message().to_owned()),
    };

    Ok((remote_oid, SyncedCommit { commit, uprooted }))
}

fn commit_from_mapping<'a>(
    line: &str,
    repo: &'a git2::Repository,
) -> Result<git2::Commit<'a>, git2::Error> {
    repo.find_commit(git2::Oid::from_str(line)?)
}
