use std::fmt;

#[derive(Debug)]
pub enum Error {
    // generic git error
    Git(git2::Error),
    // a ripit tag is required but was not found
    TagMissing,
    // the local repo has changes
    HasLocalChanges,
    // invalid config provided. For the moment, only Regex errors can cause this
    InvalidConfig {
        field: &'static str,
        error: regex::Error,
    },
    // the parent of a commit to sync cannot be mapped to a commit in the local repo
    UnknownParent {
        commit_id: git2::Oid,
        parent_id: git2::Oid,
    },
    // A synchronization caused conflicts in the index. The user has to solve them
    HasConflicts {
        summary: String,
    },
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
            Error::InvalidConfig { field, error } => {
                write!(f, "Invalid {} option: {}", field, error)
            }
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
        }
    }
}
