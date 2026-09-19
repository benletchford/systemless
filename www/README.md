# Systemless web frontend

The browser frontend for Systemless is a private workspace package built with
Leptos and Trunk. It compiles the runtime directly from the repository root;
there is no separately versioned website or published-crate update step.

The authoritative catalogue lives in [`../catalogue/`](../catalogue/). Each
Markdown file contains YAML metadata followed by the page content. Native
validation, page generation, asset promotion, and R2 maintenance live in the
`systemless-catalogue-tools` package under `../tools/catalogue/`.

## Validate the catalogue

From the repository root:

```sh
cargo run --locked -p systemless-catalogue-tools -- check
cargo run --locked -p systemless-catalogue-tools -- check --production
```

Pending assets are staged under `catalogue/incoming/<entry-id>/`. An entry
records the repository-relative source path, such as
`catalogue/incoming/example/game.sit`; Markdown in the same entry refers to it
as `incoming/example/game.sit`. Promotion rewrites both references to the
immutable asset URL and removes the staged file.

An interrupted promotion records
`catalogue/.promotion/transaction.json`. Recover it with:

```sh
cargo run --locked -p systemless-catalogue-tools -- assets recover
```

## Develop and test

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked --version 0.21.14
cd www
trunk serve --port 8080
```

Workspace checks can be run from the repository root:

```sh
cargo test --locked -p systemless-catalogue-tools
cargo test --locked -p systemless-org
cargo check --locked -p systemless-org --target wasm32-unknown-unknown
./www/scripts/build-pages.sh
```

Production output is written to `www/dist/`. The build validates that all
hosted assets have already been promoted.

## Optional browser probes

The scripts under `www/scripts/verify-*-cdp.mjs` accept tester-provided cases
rather than embedding game fixtures. Set `SYSTEMLESS_BROWSER_CASES` to a JSON
array containing the catalogue ID, route, archive URL, and local archive path.
Menu, save, keyboard, and runtime probes accept their additional checkpoints
and thresholds through that case data. The single-case runtime probe instead
uses `SYSTEMLESS_RUNTIME_ROUTE`, `SYSTEMLESS_RUNTIME_ARCHIVE_URL`, and
`SYSTEMLESS_RUNTIME_ARCHIVE_PATH`.

The catalogue tooling retains its MIT license and notice under
`tools/catalogue/`. The runtime and browser frontend use the repository's root
license.
