# Contributing to Systemless

Thanks for helping improve Systemless. These guidelines apply to every
contribution, whether it is prepared manually or with automated tools.

## Contributor terms

Systemless has some short [contributor terms](./CLA.md).

You keep ownership of your work and Systemless remains GPL-licensed. The terms
also allow contributions to be included in separately licensed paid releases,
such as a Mac App Store app, which can help support continued development of
the project.

Existing GPL rights remain unaffected.

### Accept once

Before a contribution is merged, each contributor must explicitly accept
[CLA version 1.0](./CLA.md). That acceptance covers past, present, and future
contributions as described in the agreement; you do not need to sign every PR.

When CLA Assistant is enabled, follow its link on your first pull request and
accept using your own GitHub account. Later PRs are checked automatically for
the same agreement version. A PR author cannot accept on behalf of other
contributors unless authorised to do so.

If you have already agreed by email or in a previous GitHub conversation, tell
the maintainer where that agreement was recorded. Do not post private emails
or personal information publicly. The maintainer can recognise an existing
agreement after checking its scope and your identity.

If no CLA Assistant prompt appears, ask the maintainer to confirm your
acceptance before merge. Opening a PR or leaving this template in place does
not itself constitute acceptance.

Maintainers: see [CLA administration](./.github/CLA_ADMIN.md) for activation,
existing agreements, and verification.

If you are not comfortable with those terms, that's completely fine. Please
open an issue instead and we can discuss the change without accepting
contributed code.

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
