use crate::error::Error;

/// Parse the commit message to retrieve the SHA-1 stored as a ripit tag
///
/// If the commit message contains the string "rip-it: <sha-1>", the sha-1 is returned
pub fn retrieve_ripit_tag(commit: &git2::Commit) -> Option<(String, bool)> {
    let msg = commit.message()?;
    let tag_index = msg.find("rip-it: ")?;
    let sha1_start = tag_index + 8;

    if msg.len() >= sha1_start + 40 {
        let sha1 = msg[(sha1_start)..(sha1_start + 40)].to_owned();
        let sha1_end = &msg[(sha1_start + 40)..];

        Some((sha1, sha1_end.starts_with(" uprooted")))
    } else {
        None
    }
}

pub fn retrieve_ripit_tag_or_throw(commit: &git2::Commit) -> Result<(String, bool), Error> {
    match retrieve_ripit_tag(&commit) {
        Some(v) => Ok(v),
        // FIXME: this error should mention the commit oid
        None => Err(Error::TagMissing),
    }
}

pub fn format_ripit_tag(commit: &git2::Commit, uprooted: bool) -> String {
    format!(
        "rip-it: {}{}",
        commit.id(),
        if uprooted { " uprooted" } else { "" }
    )
}
