# ripit

**ripit** is an executable that automates the copy of commits between two
git repositories.
Its main purpose is to be able to publish a repository while being able to
control what is made public. For example:
* Only publish after a given commit, so that the previous history is kept
  private.
* Remove specific blocks or tags from commit messages (for example,
  paragraphs marked as private, or tags referencing internal tickets).
* Redact author of commits to keep anonymity if desired.

## Installation

**ripit** is written in rust and uses
[cargo](https://github.com/rust-lang/cargo "cargo"). To build it, simply do:

```console
$ cargo build
$ cargo install
```

## Use

**ripit** works by adding a tag in every copied commits that references the
SHA-1 of the original commit. This makes it possible to know the last commit
synchronized from the source repository. Syncing the two repositories then
means copying every new commits created on top of this commit from the
source repository.

For the moment, **ripit** only works on a single branch, and do not handle
merge commits. Both of those limitations are planned to be removed.

**ripit** works from a local clone of the destination repository, with a
remote tracking the source repository. Before the two repositories can be
synchronized, a initial commit must be bootstrapped on the destination
repository:

```console
$ mkdir public && cd public
$ git init
$ git remote add private <...>
$ ripit --bootstrap private
```

This initial commit will import the current state of the source repository,
and add a tag in the commit message. After this step, synchronization is a
simple command:

```console
$ ripit private # -b <branch> for a specific branch, 'master' is the default
```

This command will:
* fetch the up-to-date version of the remote's branch
* display a list of new commits to synchronize, and ask for confirmation
* copy the commits in the local repo

## Features

* bootstrap a repository with the state of another one
* copy commits from a repository

