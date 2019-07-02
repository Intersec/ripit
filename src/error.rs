use std::fmt;

#[derive(Debug)]
pub enum Error {
    // generic git error
    Git(git2::Error),
    // a ripit tag is required but was not found
    TagMissing,
    // the local repo has changes
    HasLocalChanges,
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
        }
    }
}
