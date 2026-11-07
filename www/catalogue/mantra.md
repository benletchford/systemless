---
id: mantra
kind: game
title: Mantra 1.02
summary: >-
  Explore the island kingdom of Zarin, talk to its people and recover your lost
  master.
developer: The Syzygy Cult
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
compatibility:
  status: unknown
  verified:
  - date: "2026-11-06"
    tester: Catalogue maintainer
    systemless_version: afd924752115b3ebbb16da4f06228f07e37e9856 private composed candidate
    architecture: 68k
    environment: >-
      Local native 68K/depth-8 candidate based on 93170ec4 with the archive-name,
      covered-window and PICT Origin fixes from PRs 5659, 5672 and 5678. Production
      packing independently matches nine original paths, eighteen forks and nine supported
      Finder records, with strict end-of-pack validation. The complete story and continue
      prompt display. Walking into the sailor gives one dagger; inventory selection
      equips it and Space visibly thrusts it. Matched controls omit the sailor
      interaction or the thrust input. Seven gameplay pixel assertions and twenty-one exact
      reference captures pass in both thrust and no-thrust runs at 2412 frontend / 2985
      guest ticks with zero exhaustion. A separate bounded replay enters the next map. This
      is private composed evidence requiring the three fixes. Browser qualification is
      recorded separately below; released/live behavior remains unverified. Combat hits, progression, saving and audio
      fidelity remain unverified. Launch stays disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5656
  - date: "2026-11-06"
    tester: Catalogue maintainer
    systemless_version: 84c12efc79e2d32638c99e1663531e124d414b0a private composed browser candidate
    architecture: 68k
    environment: >-
      Chrome 156.0.8078.73 on Linux aarch64, worker/WebGL, 25 MHz. Private runtime
      based on 31f8f70b with all three fixes, actual hosted original archive body pinned
      to 625078 bytes and its SHA-256, both main/worker Wasm loads checked. Wasm SHA-256
      80836477139a4ccc745a5af778d330d721c1a4135b457f2755f34b29cde3b30b.
      HTTPS production origin, deployed COEP credentialless and COOP same-origin,
      cross-origin isolation and SharedArrayBuffer verified. The sailor supplies one
      dagger; inventory equips it; Space thrusts, while omission leaves it retracted.
      Final steady snapshots for separate 20-second active/control samples are
      61.223/61.188 guest ticks per second and 60.203/60.168 host frames per second;
      maximum frames 22.2/28.1 ms, minimum audio queues 303.3/281.8 ms, zero captured
      lifecycle errors. Wall-time movement leaves different positions; no exact
      browser pixel parity is claimed. Bounded hosted-browser launch approved in issue
      comment 6696366522, conditional on required runtime fixes. PICT Origin PR 5678
      remains outstanding; launch stays disabled until it lands. Released/live replay,
      combat hits, longer progression, saving and audio fidelity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5656
runtime:
  executable_path: Mantra v1.02/Mantra v1.02
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 8888ff1e28807fd2f02c6c694aa49d53f4477781598d9b0543a5d88ca9e13d3e
    size_bytes: 625078
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Mantra%20v1.02.sit
    - https://github.com/benletchford/systemless/issues/5656
    license: Original freeware distribution notice
    rights_holder: The Syzygy Cult
    permission: >-
      The original legal dialog, STR# resource 1500, says Mantra is FreeWare and asks
      users to distribute it widely. The bundled Read Me describes no fees,
      obligations or time limits. Preserve the complete unchanged package and notices.
    notes: >-
      Original 1.02 package dated January 14, 1995. Legal STR# 1500 is 155 bytes,
      SHA-256 c14d0b0d62c2308889f22f3427928b9e2830917eb959188ee625d9294b591025. Read Me is
      7848 bytes, SHA-256
      63cf9c8b85f6230ccf19da9b57630ef6d5b23164f1635a238a2f6b23cebdee2d. Application has an empty data fork and 955823-byte resource fork, SHA-256
      175653635b8b458ec8a0f34a9df7c99a90d9b17b9d0a17524e050954389da6f3. Seven CODE
      resources and no cfrg establish a 68K executable. The Read Me's PowerPC compatibility
      statement does not establish a native PPC slice.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 738e2c37ca8b029b8cdaf91e36b7c24aee9ec21a55b2e6fc33872b40dadf3425
    size_bytes: 14278
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5656
    permission: Original gameplay capture from the permitted unchanged freeware package.
    notes: >-
      Native private candidate afd92475 after receiving and equipping the sailor's
      dagger, during a thrust. Cropped to the 512x342 playfield and HUD; excludes menu
      bar, desktop, window framing, emulator and host UI.
references:
- https://github.com/benletchford/systemless/issues/5656
---

![Mantra's beach with a dagger equipped](https://assets.systemless.org/catalogue/media/sha256/73/738e2c37ca8b029b8cdaf91e36b7c24aee9ec21a55b2e6fc33872b40dadf3425.png)

Saric survives a shipwreck on the way to Zarin. Explore the island, talk to its
inhabitants and find the Ambassador of Kieh.

Move with the arrow keys or numeric keypad. Hold **Command** while moving to
run, using stamina. Walk into people or signs to interact. Press **I** to open
inventory, select a weapon, then use **Space** to attack. **Shift** uses the
alternate action.

Private native and hosted-browser candidates verify the opening story, sailor
interaction, dagger equipment and thrust; the native candidate also enters the
next map. Launch awaits the remaining runtime prerequisite and released-runtime
qualification. Combat hits, longer progression, saving and audio fidelity remain
unverified.
