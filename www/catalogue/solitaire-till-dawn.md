---
id: solitaire-till-dawn
kind: game
title: Solitaire Till Dawn
summary: >-
  Play forty classic solitaire variants with illustrated cards and smart card
  controls.
developer: Rick Holzgrafe
publisher: Semicolon Software
year: 2001
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 238fd95fd419c3f6a59940b43a208268bfa80d03
    architecture: 68k
    environment: >-
      Bounded native frontend-tick replay from the intact original publisher archive.
      Original 100-game notice and Not Yet; Games menu selects Klondike (Easy),
      ordinary tip dismissal, legal four-clubs drag onto five diamonds with revealed five
      spades and move-count increment, then stock draw reveals jack clubs and reduces
      stock count to 23. Actual captures inspected. Repeat passes four measured pixel
      assertions at 1578 frontend ticks with zero budget exhaustion. Browser, full games,
      other variants, sustained play, saves and audio unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4415
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 4ca15a7b72e5ac649ca90e4391e2ccfc608dc723
    architecture: 68k
    status: playable
    environment: >-
      Optimized local release preview in ordinary Chrome with isolated worker,
      WebGL and 25 MHz pacing. Original 100-game notice, Not Yet, Games selects
      Klondike (Easy), welcome tip dismissal, legal seven-hearts drag onto eight-clubs
      increments move count, and stock draw reveals six spades and reduces count
      from 24 to 23. Per-step captures inspected. Preview supplied complete original
      archive bytes with independent SHA-256 and size verification. The CI-promoted
      public archive separately matches the same hash and size. Public route/release,
      full games, other variants, sustained play, saves and audio remain unverified.
    evidence: https://github.com/benletchford/systemless/issues/4415
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 7b0bfb249fb66a03575fdc018a2920a44cc9a679
    architecture: 68k
    status: playable
    environment: >-
      Ordinary public Chrome on systemless.org at v0.90.0, worker/WebGL;
      crossOriginIsolated was false. Original managed archive response HTTP 200,
      SHA-256 and byte size match the publisher distribution. Original 100-game
      notice, Not Yet, Games selects Klondike (Easy), tip dismissal, legal
      two-diamonds drag onto three clubs reveals seven spades and increments move
      count, then stock draw reveals five clubs and reduces count from 24 to 23.
      Actual captures inspected. Full games, other variants, sustained play,
      saves and audio remain unverified.
    evidence: https://github.com/benletchford/systemless/issues/4415
runtime:
  executable_path: Solitaire Till Dawn 4.0.1/Solitaire Till Dawn™
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 89ed4855970a6b84ed01967fc13fb83a802a2aa17d38977bacee4d8a608545fa
    size_bytes: 1736853
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
      permits copying and distribution when no package file is sold or altered and every
      file is included. Register separately permits unchanged distribution. The bundled
      artwork notice states that third-party card designs are included by permission of
      their copyright holders.
    notes: "Complete original publisher download containing all 46 files: game, both guides, Read Me, Register, purchase link and all forty sample games. Preserve the original 100-game shareware trial and all notices. No registration key or bypass. Main executable has a zero-byte data fork, 45 CODE resources and no cfrg/PPC slice."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b8b9e6794cc64d0b0cb86c8812081088950fee9446e0642a4e9d947e1bcb2ce7
    size_bytes: 53569
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4415
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request.
    notes: >-
      Actual native Klondike gameplay after a legal move and stock draw, cropped to
      game content without host UI or menu bar.
references:
- https://www.semicolon.com/old/STD.html
---

![Solitaire Till Dawn gameplay](https://assets.systemless.org/catalogue/media/sha256/b8/b8b9e6794cc64d0b0cb86c8812081088950fee9446e0642a4e9d947e1bcb2ce7.png)

Play the original **Solitaire Till Dawn 4.0.1** shareware distribution. Acknowledge
its original **100-game** trial notice, choose **Not Yet**, then select a variant
from **Games**. Dismiss the ordinary welcome tip. Drag cards onto legal destinations
and click the stock to draw. The **Edit** menu offers undo and redo.

Bounded native verification covers **Klondike (Easy)**, a legal card move, the
revealed covered card and a stock draw. Optimized browser checks also verify a legal move and stock draw.
The public v0.90.0 player also passes a legal move and stock draw from the exact hosted archive.
Full games, the other variants, sustained play, saves and audio remain
unverified. This original executable is **68K only**.

The complete unchanged publisher archive retains both illustrated guides, all
forty sample games, registration information and its original distribution notices.
