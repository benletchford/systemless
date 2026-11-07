---
id: achi
kind: game
title: Achi 1.0
summary: Place and slide four pieces to make a line of three against the Macintosh.
developer: William C. Hale
year: 1992
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
    environment: >-
      Complete unchanged original, exact executable Achi, explicit 68K, default 25
      MHz, 800x600/depth 8. Dragging the first white reserve at (77,293) onto the central
      point (134,175) commits it and receives a black computer placement at the
      top-middle point. Both reserves decrease from four to three and the move log records
      both placements. Matched pointer-only control retains both pointer movements and
      omits only button actions, leaving the board and reserves intact. Initial captures
      match exactly; final images differ by 1338 pixels. Fresh active/control repeats
      each pass eight measured pixel assertions and three exact earlier own-run
      captures, eleven checks at 372 frontend/972 guest ticks, zero exhaustion. Own-run
      consistency is not hardware-oracle accuracy. Native binary SHA-256
      1e927387546f854ae55878cef45f5580cb381dc11831892536f2d01b0d855a0d. Runtime src equals current merged
      master 3c52cb37. Complete games, sliding phase, win detection, two-player input,
      persistence, optional speech/sound fidelity, browser pacing and released public
      play remain unverified. Launch stays disabled until browser approval.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5753
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 9ba0c5e9ca7754fa0adfc0054befe70b51ed75b4 private browser candidate
    architecture: 68k
    environment: >-
      Clean private HTTPS frontend on merged runtime 3c52cb37, Chromium
      156.0.8078.79/Linux aarch64, worker/WebGL/SAB, default 25 MHz and 800x600/depth 8.
      Production isolation headers and actual private build stamp, architecture and clock
      checked. Original hosted archive HTTP 200 matches its catalogue size/hash. Main and
      worker Wasm HTTP 200 responses match the independently hashed build: 17847979 bytes,
      SHA-256 e575dc146678e254485d2131fb8b862ff7168dd7de1f3e80cd8af78ef7e36ac9. Three
      trusted ordinary white-reserve drags commit placements at points 5,7,1; the default
      computer replies at 8,6,4. Board counts grow from zero to three for each side, while
      reserves decrease from four to one. The pointer-only control replays the same three
      paths and omits button actions, leaving the board empty and all four reserves per
      side intact. Both unchanged 20-second samples include the placement/CPU work. Final
      steady guest/host snapshots are 61.8715/59.8429 and 59.0679/60.0864 per second;
      maximum sampled worker frames 43.3/8.7 ms, minimum audio queues 118.7/132.5 ms. All
      pacing gates pass with zero captured lifecycle/network errors. Snapshots are not
      full-game averages or exact cross-case pixel comparisons. Approve bounded default
      placement play. Sliding phase, complete games, win detection, two-player input,
      persistence, optional speech/sound fidelity and released public play remain
      unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5753
runtime:
  executable_path: Achi
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: e6d8cd954bf9493f36dd3d9ce2ae47dea780ee2954726dbebb7feff1fe35e107
    size_bytes: 80077
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/achi.hqx
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/00brd-abstracts.txt
    - https://github.com/benletchford/systemless/issues/5753
    license: Original application permits distribution without sale
    rights_holder: William C. Hale
    permission: >-
      The original application notice identifies Achi 1.0, copyright 1992 William C.
      Hale, states that the software is free and permits distribution but prohibits
      sale. Preserve the entire unchanged distribution, copyright, credits and complete
      Read Me. The author-submitted Info-Mac abstract identifies it as freeware.
    notes: >-
      Independent unar/lsar extraction matches both original paths, four forks and
      two Finder records against the unchanged production packing code, strict KPK end
      96003 and pack SHA-256
      6653b5684cfb67872447d4e33c756d987b32c49422362ce6d09617d269fefd0f. The unar output directory named original is a synthetic extraction wrapper,
      not a path inside the archive. Preserve the 72387-byte application resource fork
      and entire 23552-byte Read Me. Actual app has ten CODE resources and no cfrg or
      native PPC slice. The credited Macintalk interface is retained; external
      Macintalk is not bundled or supplied, and speech is unverified.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 542392ed10dcbd1c1a801c1d71918e7f1823947d558f76cd6888fca15b922eca
    size_bytes: 3548
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5753
    rights_holder: William C. Hale
    permission: Fresh gameplay capture from the unchanged freely distributable game.
    notes: >-
      Exact 492x276 native content crop at (4,40), excluding the Classic menu bar and
      window frame; no resampling. Shows the committed white centre piece, black
      computer response, remaining reserves and move log. Not an independent hardware
      reference.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/00brd-abstracts.txt
- https://github.com/benletchford/systemless/issues/5753
---

![Achi board after a player move and computer response](https://assets.systemless.org/catalogue/media/sha256/54/542392ed10dcbd1c1a801c1d71918e7f1823947d558f76cd6888fca15b922eca.png)

Drag a white reserve piece onto a vacant point. Make a line of three to win.
After both sides place their four pieces, slide a piece along a line into the
empty point.
