---
id: ns-shaft
kind: game
title: NS-SHAFT
summary: Steer between moving platforms and descend a cave while avoiding spikes.
developer: NAGI-P SOFT
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 7ee2672626e3098f39e9376bcb60d3e99d523947
    architecture: 68k
    environment: >-
      Bounded native v0.95.2 with the merged SetPort correction, exact original
      English 1.2 executable, explicit 68K slice and literal arrows, 800-by-600/8-bit
      display. Return dismisses the unchanged US$10 shareware notice; an ordinary click
      starts New Game. Right moves the character, Left reverses it, and further input lands
      on lower platforms with B0001F advancing to B0002F. Fresh replay passes twenty
      measured pixel assertions at 947 frontend / 1547 guest ticks with zero exhausted
      frames. A matched no-arrow run retains the original horizontal position. All ten
      original data/resource forks match independent extraction. Some title-dialog pixels
      remain outside the shaft after starting. Browser/public gameplay, high-score
      entry, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4475
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: f457c9fc817caa8646c3f9bc1861429be1095b96
    architecture: 68k
    environment: >-
      Ordinary Chrome preview v0.95.2 with the merged SetPort correction, actual
      68K worker/WebGL, 800-by-600/8-bit display, 25 MHz, maximum two ticks per paint,
      literal arrows and crossOriginIsolated true. Return dismisses the unchanged
      shareware notice. Ordinary mouse New Game and Left/Right input move the
      character in both directions. A deliberate second run leaves the starting
      platform and lands on a lower platform. A later B0003F end-of-run/high-score
      prompt was also inspected; storage persistence is not claimed. Only localhost
      archive delivery uses the integrity-checked unchanged original byte fixture;
      the managed download independently matches the original hash. No guest state
      or archive bytes are patched. Title-dialog remnants reproduce in the browser
      and are tracked in issue 4477. Public replay, long runs, saved scores and audio
      remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4475
runtime:
  executable_path: NS-SHAFT 1.2/NS-SHAFT
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: ced6dfcab13f4fd7a4a7f949467bcc4de0f7abd519dae379bf351761a8efcba1
    size_bytes: 382228
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/NS-SHAFT%201.2.sit
    - https://www.nagi-p.com/v1/eng/nsshaft.html
    - https://github.com/benletchford/systemless/issues/4475
    license: Original unchanged-package distribution permission
    rights_holder: NAGI-P SOFT / Akihiko Kusanagi
    permission: >-
      The original English 1.2 Read Me First and the author's matching Macintosh
      information page permit freely distributing the unchanged package. The separate
      other-media example requires contact for CD-ROM distribution. This free online
      preservation distribution retains all five original files, documentation, registration
      program and the original shareware notice. No registration bypass or permission
      for other media is inferred.
    notes: >-
      Original 382228-byte StuffIt archive, SHA-256
      ced6dfcab13f4fd7a4a7f949467bcc4de0f7abd519dae379bf351761a8efcba1, not repacked or patched. All ten original forks
      match unar extraction. The application has four CODE resources, an empty data
      fork and no cfrg/PEF; this package contains no native PPC game slice.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 86e82d659bf504f96fcf00c99e5522e164f4ff89d4e0fbe2bca7fc5dba2ed710
    size_bytes: 84418
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4475
    permission: Systemless gameplay capture requested by the maintainer.
    notes: >-
      Lossless crop of the inspected original game content after reaching B0002F,
      including its HUD and shaft, without host UI, window title or Classic menu bar. The
      original title-dialog remnants are retained; no redraw or guest patch.
references:
- https://www.nagi-p.com/v1/eng/nsshaft.html
---

![NS-SHAFT character on a lower platform at B0002F](https://assets.systemless.org/catalogue/media/sha256/86/86e82d659bf504f96fcf00c99e5522e164f4ff89d4e0fbe2bca7fc5dba2ed710.png)

Dismiss the original shareware notice and choose New Game. Use Left and Right
to move between platforms and descend the cave. Avoid spikes and falling past
the bottom of the screen. Ordinary platforms restore life.

This is the complete original English **NS-SHAFT 1.2** shareware package,
including Read Me First and Register. The original author asks for US$10 from
users who enjoy the game; its documentation states that unregistered users have
no protected functions. This application is 68K-only.

Native testing verifies movement in both directions and descent to B0002F.
Ordinary browser testing also verifies starting a game, movement in both
directions and landing on a lower platform. [Some title-dialog remnants remain
outside the shaft](https://github.com/benletchford/systemless/issues/4477).
Long runs, saved high scores and audio remain unverified.
