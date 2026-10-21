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

## WebGPU presentation

Append `?renderer=wgpu` to a game URL (or `&renderer=wgpu` when it already has
query parameters) to select the Rust/WASM wgpu presenter. It works with both
68k and PowerPC and with worker or compatibility execution. Use HTTPS or
localhost and a browser exposing WebGPU and OffscreenCanvas. The existing
WebGL/Canvas2D selection remains the default.

The presenter uploads complete RGBA guest frames to a reusable wgpu texture
and renders them without filtering or an extra sRGB conversion. It holds frame
credit until asynchronous GPU completion, retaining at most the current frame
and one newest pending frame. Unsupported WebGPU, startup failure, device loss,
and failed submissions use the existing Canvas2D recovery path without restarting
the guest. The input canvas stays on the host, preserving focus and input handlers.
Inspect its `data-render-backend` attribute for `offscreen-wgpu`; recovery records
its reason in `data-render-fallback`.

## WebGPU compute

`?renderer=wgpu` also runs guest-visible QuickDraw and QuickDraw 3D work on the
GPU with the same wgpu kernels as the desktop `--gpu` path, for both 68k and
PowerPC guests. Select the dispatch explicitly with
`gpu_compute=adaptive|force|off`; `adaptive` uses the desktop heuristics and
keeps small operations on the CPU, while `force` sends every supported
operation to the GPU for equivalence checks. Compute works only in worker
execution mode.

Guest memory stays authoritative. QuickDraw and QD3D results must be committed
before the guest resumes, but WebGPU completes readback only on the event loop
of the thread that owns the device. A dedicated compute worker
(`gpu-compute-worker.js`) therefore owns the device. The emulation worker posts
a complete snapshot of the operation's input, blocks in `Atomics.wait`, and
reads the reply from a `SharedArrayBuffer` before committing pixels. Requests
run one at a time on the shared scratch buffers.

`SharedArrayBuffer` requires a cross-origin isolated page. `public/_headers`
sets `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: credentialless` for Cloudflare Pages. A local
`trunk serve` does not send them, so compute reports itself unavailable there;
serve `dist` with those headers instead. Browsers without `credentialless`
support (Safari), without WebGPU, or without isolation keep CPU rendering.

Failures never alter guest pixels: a declined operation falls back to the CPU,
and a device loss, worker failure or reply timeout (5 s by default,
`gpu_compute_timeout_ms` to adjust) leaves all later work on the CPU for the
session. The main thread records the compute worker's state in
`window.__systemlessGpuComputeStatus` and per-kind `[completed, failed]`
request counts in `window.__systemlessGpuCompute`.

Each GPU operation blocks the guest for one round trip; in Chrome on an Apple
M1 this measured about 0.6 ms on average, including transfer and readback.
That is why `adaptive` keeps small drawing on the CPU. Enabling compute also
loads a second copy of the WASM bundle in the compute worker.

Validate the browser integration after building:

```sh
cd www
trunk build
cd ..
node --test www/tests/*.test.cjs
node www/scripts/verify-renderer-wgpu-cdp.mjs www/dist
node www/scripts/verify-gpu-compute-cdp.mjs www/dist
```

The browser probe requires Chrome/Chromium with WebGPU (set `CHROME_BIN` if
needed). It fails if wgpu is unavailable, compares exact pixels across resizes
and both architecture slices of the committed Toolbox showcase, and injects
GPU device loss. It serves only a local temporary test site and cleans up its
browser profile. This is integration evidence on the tested browser/host,
not a claim of universal browser support or a performance improvement.

The compute probe serves pages with `public/_headers` and drives the production
emulation worker through the desktop play scenarios
(`tests/play/toolbox-showcase*.json`): 68k UI, PowerPC deep-mask sprites and
PowerPC QD3D. Each runs on the CPU and with every supported operation forced
through the compute worker; captures must match (QD3D within eight levels per
component, as on the desktop) and request counters must prove GPU execution. It
also runs adaptive dispatch and injects device loss mid-scenario, requiring the
guest to finish on the CPU with identical output.

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

Game archives must be hosted by Systemless. Visitor-supplied archives and
external archive links are not accepted as game sources.

Run preview validation below, then open a pull request with the entry and its
small incoming assets. **Local R2 credentials and manual uploads are not
required.** Keep `launch_enabled: false` until browser testing is approved. Use
`trunk serve --port 8080` from this directory to inspect the generated route;
incoming assets are served from the preview origin for local browser checks;
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

Managed archive uploads set `Content-Disposition` to the catalogue ID and file
extension (for example, `escape-velocity-nova.sit`). When multiple entries
reference identical archive bytes, the alphabetically first catalogue ID is
used because one R2 object can have only one response filename. The manual R2
audit can update existing archive metadata without changing keys
or file bytes. Run the default-branch `R2 connection check` workflow with
`apply: false` to review the inventory and orphan plan, then run it with
`apply: true` to update filenames and delete orphaned objects under the two
catalogue-managed prefixes. The reconciliation verifies the current catalogue
and bucket inventory immediately before deletion and leaves objects younger
than 24 hours alone.

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
Large diagnostic reports are retrieved in bounded CDP chunks after sampling,
without dropping samples. Connection failures and unresponsive CDP commands fail
the probe explicitly instead of leaving an unresolved report request.

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
node --test www/tests/*.test.cjs
```

The experimental indexed GPU presenter also has a fixture-free differential
probe. It runs in a real OffscreenCanvas worker and compares GPU readback with
scalar RGBA for padded indexed rows, all palette indices, palette-only changes,
odd dimensions, typed-array offsets and RGBA/indexed transitions. It also checks
compact retained-image expansion at scales 1–4 against reference vectors verified
by the Rust `CompactPresentation` resolver, plus detail-texture row boundaries.
A direct MessagePort check sends 1,000 packets through the production owner and
renderer with an injected 80 ms paint delay. It verifies exact submitted pixels,
mode/palette changes, newest-pending coalescing and bounded returned buffers.
The bitmap path adds two 500-frame bursts with delayed host acknowledgements,
exact readback and at most one outstanding ImageBitmap. Composited screenshot
comparisons cover four clipped integer/fractional layouts against an ordinary
DOM WebGL canvas:

```sh
node www/scripts/verify-renderer-gpu-cdp.mjs
```

Set `CHROME_BIN` if Chrome/Chromium is not in a standard installation location.
This validates conversion and bounded transport in a real browser. It does not
measure normal performance or physical display latency; see
[measured gameplay and remaining coverage](RESPONSIVENESS.md).

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


The GPU renderer also has an experimental bitmap presentation path:
`?renderer=worker&renderer_gpu=1&renderer_direct=1&renderer_bitmap=1`.
The renderer expands owned packets on a detached OffscreenCanvas and transfers
an ImageBitmap to a DOM WebGL canvas for host submission. Frame credit waits for
that submission; stale, failed and submitted bitmaps are closed. This avoids the
transferred-display-canvas sampling difference in the tested clipped layouts.
Same-build 1×/3× gameplay comparisons include the additional host bitmap
callback and show reduced high-DPI host work without material guest-progress
regression in the tested workload. It remains opt-in; see
[RESPONSIVENESS.md](RESPONSIVENESS.md) for measurements and coverage limits.

### Separate original archives by architecture

An entry with separate original ports may declare multiple `archive` artifacts and
bind their IDs with `architecture_archives`, for example:

```yaml
architecture_archives:
  68k: classic-archive
  ppc: powerpc-archive
```

Bindings must cover exactly the entry's supported architectures and reference every
archive artifact. Each archive retains its own provenance, rights, digest and asset
promotion. An empty mapping preserves the existing single-archive behavior, including
FAT applications. The launch and download selection follow the chosen architecture;
static pages and prefetch use the default architecture. Distinct original archives
cannot share a single web pack. The compiled catalogue schema is version 3.
