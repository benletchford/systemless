---
id: vampire-chess
kind: game
title: Vampire Chess
summary: Combine and transform vampire pieces in this original strategy board game.
developer: James Burton / John Burton
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: d02a603f native archive-loader candidate
    architecture: 68k
    environment: "Complete unchanged original package, exact vchess.cpt/Vampire Chess executable, explicit 68K, default 25 MHz/800x600/depth 8 without partition override. Actual menu captures confirm original player-versus-computer and untouched Easy AI. Select the human pawn at row 2/column 5, then its highlighted row 3/column 5 destination. After a bounded wait the computer combines its row 8/column 6 pawn into row 7/column 5, creating a bishop. Equal-clock omitted-destination-click control retains the selection and original board. Each fresh repeat passes eight measured causal pixels and seven exact prior screenshots at frontend/guest tick 1036/1628 with zero exhaustion. Actual qualified images inspected; own-run references establish consistency, not hardware accuracy. Native binary SHA-256 d961445d3cbcee316a041419a75a24eaaaf6d0be22bf33ec3d8a4e1a8ac73b5e. The candidate includes the generic original-filename preservation fix in #5834; no game patch. Initial short wait showed only the human move; the longer bounded wait establishes the reply. Earlier assertion-script preparation failures are retained separately. Full games, other AI modes, victory/scoring, persistence and audio fidelity remain unverified. Hosted-original browser evidence follows below; released public play remains unverified."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5835
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 426dbfb3 private browser preview on d02a603f runtime candidate
    architecture: 68k
    environment: >-
      Fresh isolated Chromium/Linux profiles, worker/WebGL and SharedArrayBuffer,
      default 25 MHz/800x600/depth 8, production COEP credentialless and COOP same-origin
      headers. Source stamp, architecture, worker and clock checked. Both unchanged
      20-second samples include menu inspection confirming original player-versus-computer
      and untouched Easy AI, pawn selection and a 780-guest-tick reply wait. Active clicks
      the highlighted destination; control omits that click. Actual captures show the
      pawn advance and computer combination reply versus the selected original board.
      All actions finish within the sample, at 15.719/15.667 seconds. No hidden warm-up
      gameplay, game patch, seed, clock, strength, sound or partition change. Final
      steady guest/host rates are 61.0212/60.0042 and 60.0051/60.0051 per second; maximum
      sampled worker frames 29.4/17.1 ms and minimum audio queues 120.4/132.6 ms. Both
      pass unchanged pacing gates with no captured lifecycle or network errors. These
      snapshots are not whole-game averages. Original hosted archive HTTP 200 matches
      1058324 bytes, SHA-256 b52895c11988cda1fa81a714af1da270f1aa2f43ac2ca8a459b3cf87ad651f07.
      Both main and worker Wasm HTTP 200 bodies match the build, 17950672 bytes,
      SHA-256 d9fb2a79fcbfd3cb21c40de6f046ccacb2189efb53559ea7ef12d625fa4089e3.
      Initial browser attempt timed out before gameplay because its driver waited for
      guest ticks during a correctly frozen menu; retained as a setup failure. Corrected
      driver uses brief real-time menu holds; game, build and all pacing limits unchanged.
      This establishes bounded opening input, computer reply and browser pacing. Full
      games, other AI modes, victory/scoring, persistence, audio fidelity and released
      public play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5835
runtime:
  executable_path: vchess.cpt/Vampire Chess
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: b52895c11988cda1fa81a714af1da270f1aa2f43ac2ca8a459b3cf87ad651f07
    size_bytes: 1058324
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/brd/vampire-chess.hqx
    - https://github.com/benletchford/systemless/issues/5835
    license: Original unchanged free-distribution grant with both instruction files
    rights_holder: James Burton; John Burton; retained original contributors
    permission: >-
      Complete original Simple Instructions/ReadMe explicitly permits free unchanged
      distribution, including uploads to BBS and Internet services, provided both that
      ReadMe and the accompanying MS Word instructions remain unaltered with the game.
      Preserve all original forks, resources, Finder metadata, artwork and credits.
    notes: >-
      Full 6500-byte ReadMe reviewed; complete 117760-byte Word instructions
      retained, with extractable long text reviewed and embedded graphics untouched. Original
      documents identify copyright 1994 James Burton and John Burton graphics, retain
      the Titian Worship of Venus painting credit and credit all vampire models.
      Independent extraction matches three files, six forks and three Finder records. Original
      literal slash in Simple Instructions/ReadMe is preserved reversibly in the guest
      via the existing HFS/VFS codec; it is not a directory or renamed file. Strict KPK
      end 1209824, SHA-256
      4456dbb1487f7411f25fff179e9c57bd4473acd96d4574c511a69e612dcd612b.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 5b41cb1f0711f7c1be6b31fb52805ef34767e4f44abdd159a0c1f7b174facb8a
    size_bytes: 134820
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5835
    permission: Original gameplay capture of the unchanged freely distributable game.
    notes: >-
      Human pawn advance and computer combination reply. Native capture cropped to
      (165,110)-(634,527), 469x417, without resampling; excludes native window framing,
      menu and host interface.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/brd/vampire-chess.hqx
- https://github.com/benletchford/systemless/issues/5835
---

## Build an army of vampires

![A pawn advance and the computer's new bishop](https://assets.systemless.org/catalogue/media/sha256/5b/5b41cb1f0711f7c1be6b31fb52805ef34767e4f44abdd159a0c1f7b174facb8a.png)

The human starts at the top and moves first. Click a yellow-outlined piece, then
one of its blue-outlined destinations. Click the piece again to deselect it.
Friendly pawns can combine into bishops; friendly bishops can combine into queens.
The complete original instructions accompany the game download.
