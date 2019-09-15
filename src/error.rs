use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    // generic git error
    Git(git2::Error),
    // a ripit tag is required but was not found
    TagMissing,
    // the local repo has changes
    HasLocalChanges,
    // the parent of a commit to sync cannot be mapped to a commit in the local repo
    UnknownParent {
        commit_id: git2::Oid,
        parent_id: git2::Oid,
    },
    // A synchronization caused conflicts in the index. The user has to solve them
    HasConflicts {
        summary: String,
    },

    // error when opening the config file
    FailedOpenCfg {
        path: String,
        error: std::io::Error,
    },
    // error when parsing the config file
    FailedParseCfg {
        path: String,
        error: serde_yaml::Error,
    },
    // invalid config provided. For the moment, only Regex errors can cause this
    InvalidConfig {
        field: &'static str,
        error: regex::Error,
    },
    // Cannot setup the merge context after conflicts
    CannotSetupMergeCtx,
    // I/O Error whe opening cache file
    CacheOpenError {
        err: std::io::Error,
        filename: PathBuf,
    },
    // I/O Error while reading cache file
    CacheReadError {
        err: std::io::Error,
        filename: PathBuf,
    },
    // Invalid line in cache file
    CacheInvalidLine {
        desc: String,
        filename: PathBuf,
        line: String,
        line_number: u32,
    }
}

impl From<git2::Error> for Error {
    fn from(err: git2::Error) -> Self {
        Error::Git(err)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Git(e) => write!(f, "{}", e.message()),
            Error::TagMissing => write!(
                f,
                "Cannot find any ripit tag in the local repository.\n\
                 Run with the `--bootstrap` option to setup the repository."
            ),
            Error::HasLocalChanges => write!(
                f,
                "The repository contains non committed changes.\nAborted."
            ),
            Error::UnknownParent {
                commit_id,
                parent_id,
            } => write!(
                f,
                "Cannot synchronize commit {}: its parent {} cannot be found in the \
                 local repository",
                commit_id, parent_id
            ),
            Error::HasConflicts { summary } => write!(
                f,
                "Cannot synchronize the following commit due to conflicts:\n  {}\n\
                 Solve the conflicts and commit the resolutions, \
                 then run the synchronization again.",
                summary
            ),
            Error::FailedOpenCfg { path, error } => {
                write!(f, "Cannot open configuration file {}: {}", path, error)
            }
            Error::FailedParseCfg { path, error } => {
                write!(f, "Invalid configuration file {}: {}", path, error)
            }
            Error::InvalidConfig { field, error } => {
                write!(f, "Invalid {} option: {}", field, error)
            }
            Error::CannotSetupMergeCtx => write!(
                f,
                "Cannot setup the environment for the resolution of conflicts.\n\
                 Solve the errors listed above, then abort the current commit \
                 and run the synchronization again."
            ),
            Error::CacheOpenError { err, filename } => {
                write!(f, "Cannot open cache file {}: {}", filename.display(), err)
            },
            Error::CacheReadError { err, filename } => {
                write!(f, "Error while reading cache file {}: {}", filename.display(), err)
            },
            Error::CacheInvalidLine { desc, filename, line, line_number } => {
                write!(f, "{}:{}: line \"{}\" is invalid: {}", filename.display(), line_number,
                line, desc)
            }
        }
    }
}
