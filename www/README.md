# Systemless website

This directory contains the public [systemless.org](https://systemless.org/)
website: the browser frontend, community catalogue, catalogue maintenance
tools, and production build scripts. The frontend is built with Leptos and
Trunk as the `systemless-org` member of this repository's Cargo workspace. It
uses the runtime from the repository root and shares the repository release
version; the website package itself is not published to crates.io.

The authoritative catalogue lives in [`catalogue/`](catalogue/). Each
Markdown file contains YAML metadata followed by the page content. Native
validation, page generation, asset promotion, and R2 maintenance live in the
`systemless-catalogue-tools` package under `tools/catalogue/`.

Optional plugins are declared separately under
[`catalogue/plugins/`](catalogue/plugins/). Each YAML file has
`schema_version`, the target `entry`, its external supplement `artifacts`, and
its `plugins`. Multiple files may target the same entry, so keep large
collections in numbered chunks such as `example-01.yaml` and `example-02.yaml`.
Plugin and artifact IDs remain unique within the resolved entry. Plugin
artifacts currently stay with their original HTTPS provider; the catalogue
asset-promotion pipeline does not copy them into managed storage.

## Contribute a catalogue entry

Create `catalogue/<stable-id>.md` by following the structure of an existing
entry: YAML metadata between the `---` markers, followed by the public Markdown
description. Keep the stable ID, filename, route, and referenced artifact IDs
consistent. Put large optional plugin collections in one or more
`catalogue/plugins/<stable-id>-NN.yaml` files instead of expanding the entry.

Stage assets that still need promotion under
`catalogue/incoming/<stable-id>/`; do not commit large software archives. Run
the preview and production validation commands below, then use
`trunk serve --port 8080` from this directory to inspect the generated route
before opening a pull request.

## Validate the catalogue

From the repository root:

```sh
cargo run --locked -p systemless-catalogue-tools -- --root www check
cargo run --locked -p systemless-catalogue-tools -- --root www check --production
```

Pending assets are staged under `www/catalogue/incoming/<entry-id>/`. An entry
records the website-relative source path, such as
`catalogue/incoming/example/game.sit`; Markdown in the same entry refers to it
as `incoming/example/game.sit`. Promotion rewrites both references to the
immutable asset URL and removes the staged file.

An interrupted promotion records
`www/catalogue/.promotion/transaction.json`. Recover it with:

```sh
cargo run --locked -p systemless-catalogue-tools -- --root www assets recover
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

## Deployment

Pull requests and commits run the website checks only when they change `www/`
or an input consumed by the frontend and catalogue tooling. Production Pages
are built and deployed only after release-please creates a release. The deploy
workflow checks out that release's exact tag before validating the catalogue,
building `www/dist/`, and publishing it to Cloudflare Pages.

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
