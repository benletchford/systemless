---
id: battalion
kind: game
title: Battalion
summary: >-
  Stomp through a town as a giant monster and use its weapon against the military
  in Andrew Johnson's original Macintosh game.
developer: Andrew Johnson
year: 1996
architectures:
- ppc
default_architecture: ppc
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 874054b7 native; same complete tree as d6e331ce
    architecture: ppc
    environment: >-
      Complete unchanged original Battalion 1.4 package and exact
      battalion1.4.cpt/battalion1.4/battalion executable. Explicit PPC, default 25 MHz/800x600/depth 8 and
      original partition. Space then 6 selects the original Googelon character. Active
      and equal-clock idle share a centered pointer and identical ready image. Moving
      the pointer changes the terrain view; held left click visibly fires a blue beam
      absent from the idle run. Each repeat passes six measured causal pixels and five
      exact prior screenshots at 398 frontend/guest ticks with zero exhausted frames.
      Actual qualified images inspected; own-run references establish consistency, not
      hardware accuracy. Native binary SHA-256
      7c63e26a5cdca03565e5033ec8bb73098e7fd65bed902ef85573cdc1b6f8b93c. Original Low Detail is checked and Music unchecked; no
      setting is changed. Volume has no visible selected mark. About credits Andrew
      Johnson 1994-1996. An earlier arrow/Control pair produced identical active/control
      images and is not input evidence. Scoring, complete games, other monsters, 68K play,
      persistence, audio fidelity and released public play remain
      unverified. Hosted-original browser evidence follows below.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5847
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: b361e032 private browser preview on merged 5654afa7 runtime
    architecture: ppc
    environment: >-
      Fresh isolated Chromium/Linux profiles, worker/WebGL and SharedArrayBuffer,
      default 25 MHz/800x600/depth 8, original partition and settings. Production
      COEP credentialless and COOP same-origin headers. Actual private source stamp,
      architecture and clock checked. Space then 6 selects Googelon inside each
      unchanged 20-second sample. Active moves the pointer and holds left click;
      the omitted-input control keeps the pointer centered. Actual ready, steering,
      firing and idle captures inspected: a long blue weapon beam appears only
      with the held button, with a changed terrain view under steering. A bounded
      pixel-only connected-component reader confirms the beam without guest-memory
      access. Actions finish at 4.690/4.774 seconds inside the samples. Original Low
      Detail checked and Music unchecked remain unchanged; no game, seed, clock,
      depth, detail, audio, partition or hidden gameplay warm-up change.
      Final steady guest/host rates are 56.8782/56.8782 and 56.9100/59.9587 per
      second; maximum sampled worker frames 41.9/37.3 ms and minimum audio queues
      135.8/160.9 ms. Both unchanged pacing gates pass with no captured lifecycle or
      network errors. These are final steady snapshots, not whole-game averages.
      The actual original archive HTTP 200 matches 1810128 bytes, SHA-256
      58453501a2049e6b046f1af7c6b46b81e8999fa62ed7f37c8210dc34d22c0385.
      Both main and worker Wasm HTTP 200 responses match 17945218 bytes, SHA-256
      3e54e7877fb05a10d3d29def7f4c84e3b192d49958306344abc6dff24a011092.
      Initial driver 01 stopped at its image assertion before completed pacing
      qualification: the beam pointed left, outside its native-derived right-side
      detection region. Retained as setup failure; driver 02 inspects the full game
      viewport and validates both native directions and the actual browser capture.
      No game, build or pacing-limit change. An earlier private build had incorrect
      setup paths and was stopped before browser use; the corrected build and exact
      source are retained. Runtime 5654afa7 has the same complete tree as native
      874054b7. This establishes bounded steering/firing and browser pacing only.
      Scoring, complete games, other monsters, 68K play, persistence, audio fidelity
      and released public play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5847
runtime:
  executable_path: battalion1.4.cpt/battalion1.4/battalion
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 58453501a2049e6b046f1af7c6b46b81e8999fa62ed7f37c8210dc34d22c0385
    size_bytes: 1810128
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/battalion-14.hqx
    - https://github.com/benletchford/systemless/issues/5847
    license: Original complete unchanged noncommercial-distribution grant
    rights_holder: Andrew Johnson and retained third-party contributors
    permission: >-
      The original readme explicitly permits complete unchanged copying and
      distribution for noncommercial purposes, with its copyright notice in every copy. It
      requires the entire package and defines distribution with any product as commercial.
      Preserve this restriction; no paid bundle or modified game is authorized. Retain
      all original documentation, assets, music and third-party credits with the free
      online original-game archive.
    notes: >-
      Version 1.4, September 1996. Full 7141-byte HTML readme, all eleven supplied
      GIF help screens and actual About dialog reviewed. Mesa, Mac graphics driver, tk
      library and asynchronous sound-helper credits remain intact. Independent extraction
      matches all 44 files, 88 forks and 44 Finder records against production packing
      with an explicit Compact Pro wrapper mapping. Strict KPK end 2235755, SHA-256
      82ba175c1adc3c37616ec648614130fc7af82488ebc27338a3726fd86983eec4. Four monster
      choices are one game, not four catalogue additions.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b02ca7e8e52ad208773a2df9c11cb7086c44680f7f21355a7be7ea0c0f72a712
    size_bytes: 2984
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5847
    permission: Original gameplay capture of the unchanged freely distributed game.
    notes: >-
      Googelon fires its blue beam; score remains zero. Native crop
      (20,50)-(372,310), 352x260, without resampling; excludes host interface.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/arc/battalion-14.hqx
- https://github.com/benletchford/systemless/issues/5847
---

## Monsters in town

![Googelon fires its weapon](https://assets.systemless.org/catalogue/media/sha256/b0/b02ca7e8e52ad208773a2df9c11cb7086c44680f7f21355a7be7ea0c0f72a712.png)

Press Space at the title, then 6 to select Googelon. Move the mouse to steer and
hold the left mouse button to use its weapon. Keys 1, 2 and 3 select the original
camera views. The complete original help and other monster choices accompany
the game archive.
