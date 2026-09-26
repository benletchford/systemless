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

Every game must include a real gameplay screenshot before it is approved or
released. Capture it from the exact archive tested with Systemless, crop it to
the game's content surface, and exclude emulator framing, host UI, and the
Classic Mac menu bar. Record it as a `screenshot` artifact with
`content_only: true`, and include it in the entry's Markdown description.

Stage small assets, such as screenshots, under
`catalogue/incoming/<stable-id>/` and include them in your PR. Do not commit
large software archives: use a `type: url` source with an HTTPS `url`,
`expected_sha256`, and `expected_size` instead. Only submit assets that may be
redistributed. An incoming source uses `type: incoming` and a website-relative
`path`, such as `catalogue/incoming/example/screenshot.png`; Markdown in the
same entry refers to it as `incoming/example/screenshot.png`.

Run preview validation below, then open a pull request with the entry and its
small incoming assets. **Local R2 credentials and manual uploads are not
required.** Keep `launch_enabled: false` until browser testing is approved. Use
`trunk serve --port 8080` from this directory to inspect the generated route;
asset promotion and browser launch approval are separate steps.

### How assets reach production storage

The [promotion approval workflow](../.github/workflows/promotion-approval.yml)
starts the trusted [promotion workflow](../.github/workflows/promote-review.yml)
for eligible PRs targeting the default branch:

- For a branch in this repository, a maintainer with write access approves the
  current PR revision. Owner-authored PRs also qualify when the repository
  owner opens, updates, reopens, or marks them ready for review.
- CI validates and previews the submission before using repository R2 secrets
  to upload its incoming assets and managed downloads. It checks the PR head
  and authorization again before uploading.
- CI commits immutable SHA-256 asset URLs back to the PR branch, updates
  Markdown references, removes promoted incoming files, and reruns website CI.
- Fork PRs need a maintainer to run the
  [manual promotion workflow](../.github/workflows/promote-assets.yml) against
  the exact PR head and apply the generated catalogue changes from its output
  artifact. The automatic workflow cannot commit back to fork branches.

A contributor can submit pending assets for review. Production validation is
required after promotion, before release; it is not a prerequisite for opening
the contribution PR.

## Validate the catalogue

From the repository root, run preview validation while preparing a PR:

```sh
cargo run --locked -p systemless-catalogue-tools -- --root www check
```

Preview validation accepts incoming assets and integrity-pinned HTTPS sources.
After CI has promoted the managed assets and committed the generated source
rewrites, production validation should pass:

```sh
cargo run --locked -p systemless-catalogue-tools -- --root www check --production
```

### Maintainer recovery

Local asset-promotion commands are optional maintainer tools, not contributor
setup steps. An interrupted promotion records
`www/catalogue/.promotion/transaction.json`. A maintainer with the appropriate
storage access can recover it with:

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
`SYSTEMLESS_RUNTIME_ARCHIVE_PATH`. It detects worker-backed catalogue games and
measures their frame replies as well as main-thread runtime frames.

The catalogue tooling retains its MIT license and notice under
`tools/catalogue/`. The runtime and browser frontend use the repository's root
license.

The runtime pacing probe reports p50/p95/p99 distributions as well as maxima.
It keeps GPU-disabled fallback coverage by default; set `SYSTEMLESS_RUNTIME_GPU=1`
to allow GPU rendering for a separate comparable run. This enables the browser
GPU but does not assert which renderer the runtime selected. Keep warm-up,
archive, inputs, initial saves and guest progress equal when comparing results.
Set `SYSTEMLESS_RUNTIME_DEBUG=0` to measure without the probe's debug overlay.
Optional `SYSTEMLESS_RUNTIME_TRACE_PATH` and `SYSTEMLESS_RUNTIME_SCREENSHOT_PATH`
write raw bounded samples and the final browser screenshot to tester-chosen
local paths. Reports include the actual renderer, display scale, CPU setting
and guest instruction/tick endpoints; wall-time samples alone are insufficient
to establish equal guest progress.

The save probe verifies gameplay-created saves, download, removal from IndexedDB,
and re-import with identical data and resource forks. Cases can set `requireWorker`
to require worker execution. For installed-plugin coverage, supply
`selectedPluginIds` and `pluginAssets` (each with `url` and local `path`). Optional
`expectedMetadata` and `forkLengths` check the transferred plugin metadata and both
forks. `workerBootFailure: true` injects a startup failure after transfer to check
compatibility fallback. `SYSTEMLESS_SAVE_SMOKE_SCREENSHOT_DIR` retains the final
browser image on success or failure.

Worker protocol and lifecycle tests run without browser fixtures:

```sh
node --test www/tests/emulator-worker.test.cjs www/tests/worker-lifecycle.test.cjs
```

Worker commands carry a runtime generation and monotonic command sequence. The
bridge allows eight commands in flight and 256 pending commands (plus one reserved shutdown); consecutive
pending mouse moves can coalesce, but key/button/save boundaries stay ordered.
Queue exhaustion stops the runtime visibly. Normal navigation stops display and
audio immediately, then asks the owner to flush saves before terminating it.
Shutdown failures appear in a dismissible notice even after leaving the game.

`verifyShutdown: true` checks navigation cleanup and save-flush acknowledgement.
`verifyRestart: true` also immediately reopens the same game and checks that its
new owner starts after the previous save flush completes. Other games can start
independently while an earlier game finishes saving.

Catalogue `runtime.worker` defaults to `true`; set it explicitly to `false` to
use compatibility execution. This applies to installed-plugin launches too.
Startup worker failures retain the downloaded archive and plugin forks for
compatibility fallback, reported on the canvas's `data-runtime-fallback`
attribute. A failure after startup stops the game visibly without restarting it.
The service worker fetches runtime worker scripts and binding snippets from the
network first; offline stale code is checked by the runtime protocol handshake.

For comparisons over matching guest-time intervals, set
`SYSTEMLESS_RUNTIME_TARGET_TICK` on the runtime pacing probe. The sample duration
then acts as a timeout. Reports include the requested and observed tick and
instruction endpoint; a frame can pass the requested tick, so inspect the raw
traces and compare their common interval. This does not force matching retired
instruction counts or establish image correctness by itself.

See [browser responsiveness measurements](RESPONSIVENESS.md) for the qualified
workloads, cold-start tradeoff, lifecycle checks and coverage limits.
