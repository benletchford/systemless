# Contributing to Systemless

Thanks for helping improve Systemless. These guidelines apply to every
contribution, whether it is prepared manually or with automated tools.

## Before making changes

- Read the project context and build instructions in `README.md`.
- Use a public issue to record a defect before opening a pull request that
  fixes it.
- Keep issues and pull requests scoped to the standalone Systemless repository
  and its public interfaces.

## Issue and pull-request evidence

- Attach screenshots and other media used only by an issue or pull request to
  that GitHub conversation.
- Do not commit issue-only or pull-request-only evidence to the repository or
  create a repository-hosted discussion-attachment directory.
- Commit images only when project documentation, tests, examples, or shipped
  assets consume them.
- Prefer focused automated tests or fixtures over screenshots when evidence can
  be expressed as a durable regression check.

## Validation

Run checks appropriate to the change. The main project checks are:

```sh
cargo build --release
cargo test --lib
cargo check --no-default-features
cargo package
```

## Commits and pull requests

- Use a one-line Conventional Commit message with no attribution trailer.
- Keep commits focused on one coherent change.
- Use pull requests for changes to the public repository and avoid merge
  commits.
