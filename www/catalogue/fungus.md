---
id: fungus
kind: game
title: Fungus 2.0
summary: Grow connected pieces around your fungus head and surround rival colonies.
developer: Ryan Koopmans
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: f9712bfca41d87823175e38def2f5eeb06f85db1 native release-base candidate
    architecture: 68k
    environment: "Complete unchanged original, exact executable, 68K, 800x600/depth 8. Original Play, red Human, green CPU and Start enter the board; the default unchecked Turn Time Limit and original player profiles remain intact. Pick up the red L-shaped preview at (569,348), drag to (274,296) and release. With the complete region-offset correction in #5745, red growth connects to its head, the green CPU adds growth and the next preview becomes T-shaped. Equal-frontend control retains pointer moves and omits only button actions, leaving the board intact. Initial board images match exactly; final images differ in 3746 pixels. Fresh active/control repeats each pass twelve measured pixels and six exact own-run captures, eighteen checks at 786 frontend ticks and 1314/1338 guest ticks, zero exhaustion. The retained 24-tick drag accounts for the guest-clock difference; this is not a same-guest-tick comparison or hardware oracle. Native SHA-256 1e927387546f854ae55878cef45f5580cb381dc11831892536f2d01b0d855a0d. Fresh release-base repeats match every earlier native capture exactly. The runtime src tree equals rebased candidate 0af1a962aa3d4de82ac7932121d0787d49ecc5d3; only the three public verification records differ. Optional turn-timer callbacks remain unimplemented (#5742), so timed play is unqualified. Browser input/pacing, rotation, captures, full matches, persistence, sound fidelity and released public play remain pending. Launch stays disabled until browser approval."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5726
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 8ce6821ec8c2a9714a980bd1644eadf3e88b8458 private preview
    architecture: 68k
    environment: >-
      Fresh Chromium 156/Linux aarch64 profiles with worker/WebGL and SharedArrayBuffer,
      default 25 MHz/depth 8, production isolation headers and verified private source
      stamp. Original hosted archive HTTP 200 matches 475554 bytes and SHA-256
      3adbc029cf43b701deacb177d25045392508279142e4a5ecb09718a67c0e3d86.
      Main and worker Wasm HTTP 200 responses match the independently hashed build:
      17839661 bytes, SHA-256
      a1bb32bb15318335e2e57f77da86303970b2c58f4a3b2feea89722d5c3ffdced.
      Ordinary Play, red Human, green CPU and Start preserve the original unchecked
      turn timer. Visible-board analysis chooses legal connected drops; trusted mouse
      input commits three pieces and receives three CPU responses. Red occupied cells
      increase 1/5/9/13 and green cells 1/6/12/17. The pointer-only control omits button
      actions and retains one red and one green cell. Two unchanged 20-second samples
      include two further placements and CPU turns in the active case. Final steady
      guest/host snapshots are 61.0085/59.9917 and 60.9409/59.9252 per second; maximum
      sampled worker frames 26.2/6.2 ms and minimum audio queues 127.8/129.0 ms.
      All pacing gates pass; no captured lifecycle or network errors. These snapshots
      are not full-match averages. Rotation, captures, full matches, persistence,
      timed turns, sound fidelity and released public play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5726
runtime:
  executable_path: Fungus20.cpt/Fungus 2.0
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 3adbc029cf43b701deacb177d25045392508279142e4a5ecb09718a67c0e3d86
    size_bytes: 475554
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/fungus-20.hqx
    - https://github.com/benletchford/systemless/issues/5726
    license: Original About screen permits unmodified non-profit distribution
    rights_holder: Ryan Koopmans
    permission: >-
      The original About screen identifies Fungus 2.0, copyright 1992/1994 Ryan
      Koopmans, calls it freeware and expressly permits making and distributing unmodified
      copies, with a prohibition on selling the game for profit. Preserve the whole
      unchanged distribution, player data, Color Design Team artwork/music credits,
      copyright and notices; no charge or registration patch.
    notes: >-
      Independent production packing matches two original paths, four forks and two
      supported Finder records; strict KPK end 780930, pack SHA-256
      c2d794ac16e8951758836d38496e19754d321e1ff64ff82a206d6b0e4e41eb47. The original BinHex wraps
      Fungus20.cpt (349790 bytes, SHA-256
      20b694975dfb64cb5cfeafbfdddb9c1058dab1320432ae718011cc14d60db820). The independent inner extraction uses that decoded outer filename as
      the path prefix, matching production without altering forks or Finder metadata.
      Preserve both the resource-only application and original 294-byte Fungus Players
      file. Actual application has five CODE resources and no cfrg/native PowerPC slice.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 53978491eca5ca523030743a6968d3524405d5755efe311c5ef3b84c8a012f2a
    size_bytes: 6287
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5726
    rights_holder: Ryan Koopmans
    permission: Original gameplay capture from the permitted unchanged distribution.
    notes: >-
      Exact 512x342 content crop at (144,86), excluding the window frame, Classic
      menu bar and host UI; no resampling. Shows a committed red piece, green CPU response
      and next-piece preview. Not an independent hardware reference.
references:
- https://github.com/benletchford/systemless/issues/5726
- https://github.com/benletchford/systemless/issues/5745
---

![Fungus gameplay](https://assets.systemless.org/catalogue/media/sha256/53/53978491eca5ca523030743a6968d3524405d5755efe311c5ef3b84c8a012f2a.png)

Connect new pieces to the fungus around your head, surround enemy growth on two
sides to capture it, and protect your head from rival colonies.

Choose **Play**, select a red **Human** and green **CPU**, then choose **Start**.
Drag a piece from the preview into a connected position on the board. **Space**
rotates the next piece before placement.

Default untimed Human-versus-CPU play has been tested natively and in a private
browser candidate, including three connected placements and CPU responses. Timed
turns remain unqualified.
