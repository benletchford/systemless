---
id: solitaire-till-dawn
kind: game
title: Solitaire Till Dawn
summary: Play forty classic solitaire variants with illustrated cards and smart card controls.
developer: Rick Holzgrafe
publisher: Semicolon Software
year: 2001
architectures: [68k]
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 238fd95fd419c3f6a59940b43a208268bfa80d03
    architecture: 68k
    status: playable
    environment: >-
      Bounded native frontend-tick replay from the intact original publisher archive.
      Original 100-game notice and Not Yet; Games menu selects Klondike (Easy),
      ordinary tip dismissal, legal four-clubs drag onto five diamonds with revealed
      five spades and move-count increment, then stock draw reveals jack clubs and
      reduces stock count to 23. Actual captures inspected. Repeat passes four
      measured pixel assertions at 1578 frontend ticks with zero budget exhaustion.
      Browser, full games, other variants, sustained play, saves and audio unverified.
    evidence: https://github.com/benletchford/systemless/issues/4415
runtime:
  show_menu_bar: true
  executable_path: Solitaire Till Dawn 4.0.1/Solitaire Till Dawn™
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://github.com/benletchford/systemless/releases/download/catalogue-intake-20261010/solitaire-till-dawn-401.sit.hqx
    expected_sha256: 89ed4855970a6b84ed01967fc13fb83a802a2aa17d38977bacee4d8a608545fa
    expected_size: 1736853
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.semicolon.com/old/DownloadPage.html
    - https://www.semicolon.com/Downloads/SolitaireTillDawn401.sit.hqx
    - https://github.com/benletchford/systemless/issues/4415
    license: Semicolon Software complete unchanged package distribution licence
    rights_holder: Rick Holzgrafe and Semicolon Software
    permission: >-
      The bundled Games Guide Copyright and License chapter (TEXT resource 10988)
      permits copying and distribution when no package file is sold or altered and
      every file is included. Register separately permits unchanged distribution.
      The bundled artwork notice states that third-party card designs are included
      by permission of their copyright holders.
    notes: >-
      Complete original publisher download containing all 46 files: game, both guides,
      Read Me, Register, purchase link and all forty sample games. Preserve the original
      100-game shareware trial and all notices. No registration key or bypass. Main
      executable has a zero-byte data fork, 45 CODE resources and no cfrg/PPC slice.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/solitaire-till-dawn/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4415
    permission: Original gameplay screenshot captured for this catalogue at the maintainer's request.
    notes: Actual native Klondike gameplay after a legal move and stock draw, cropped to game content without host UI or menu bar.
references:
- https://www.semicolon.com/old/STD.html
---

![Solitaire Till Dawn gameplay](incoming/solitaire-till-dawn/gameplay.png)

Play the original **Solitaire Till Dawn 4.0.1** shareware distribution. Acknowledge
its original **100-game** trial notice, choose **Not Yet**, then select a variant
from **Games**. Dismiss the ordinary welcome tip. Drag cards onto legal destinations
and click the stock to draw. The **Edit** menu offers undo and redo.

Bounded native verification covers **Klondike (Easy)**, a legal card move, the
revealed covered card and a stock draw. Browser qualification and publication are
pending. Full games, the other variants, sustained play, saves and audio remain
unverified. This original executable is **68K only**.

The complete unchanged publisher archive retains both illustrated guides, all
forty sample games, registration information and its original distribution notices.
