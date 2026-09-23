---
id: harry-the-handsome-executive
kind: game
title: Harry the Handsome Executive
summary: >-
  Scoot through ScumCo in a swivel chair in Ambrosia's original 30-day Macintosh
  shareware trial.
developer: Ben Spees
publisher: Ambrosia Software
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.51.0 + PR #2457"
    architecture: 68k
    environment: >-
      Deterministic run from the unchanged 1.0.0 BinHex installer through
      installation, automatic handoff, the unregistered shareware notice, the New Game story and
      live first-level gameplay with a Space input
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2458
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 6cc2aa5f1c4fc5f8950f9ba7d9af9b7d909f98ffcefc4d576c4cd1d36f02fc15
    size_bytes: 8993578
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/harry-10.hqx
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    license: Ambrosia Software Harry 30-day shareware license
    rights_holder: Ambrosia Software, Inc.
    permission: >-
      The included Harry License permits non-profit distribution of the complete,
      unmodified software without prior written notice. It grants use of the unregistered
      game for 30 days from receipt; continued use after that period requires
      registration. This is the unchanged author-submitted installer, without a registration key.
    notes: >-
      The Info-Mac abstract is submitted from help@ambrosiasw.com and identifies
      Harry as Ben Spees's shareware game. The unchanged 8,993,578-byte BinHex archive has
      SHA-256 6cc2aa5f1c4fc5f8950f9ba7d9af9b7d909f98ffcefc4d576c4cd1d36f02fc15. Its
      installer creates the game and supporting files before Systemless hands off to the
      installed application.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 56224fcaaf1c0906dc43f3121b0e4073e4a997a4d60a0572f3417878dca764b8
    size_bytes: 287018
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2458
    permission: >-
      Original in-game screenshot captured for this catalogue from the unregistered
      shareware trial. Underlying Harry artwork remains Ambrosia's property.
    notes: >-
      Fresh 640x480 game-content capture from the exact installer in the first
      playable ScumCo level on 2026-09-23. The surrounding desktop was cropped away without
      changing the game pixels. PNG SHA-256
      56224fcaaf1c0906dc43f3121b0e4073e4a997a4d60a0572f3417878dca764b8; 287,018 bytes.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
- https://ftp.funet.fi/pub/mac/info-mac/game/arc/harry-10.hqx
---

## The corporate ladder has wheels

![Harry in the first playable ScumCo office](https://assets.systemless.org/catalogue/media/sha256/56/56224fcaaf1c0906dc43f3121b0e4073e4a997a4d60a0572f3417878dca764b8.png)

Harry does not walk to work: he scoots, kicks and swivels through ScumCo in
his office chair. Explore the building, dodge hostile coworkers and keep an
eye on comfort and corporate favor as the first assignment begins.

This is Ambrosia's original Macintosh shareware release, not a registered or
unlocked copy. Choose **Install** on first launch; Systemless then opens the
installed game. At the registration notice, choose **Not Yet** to try it.
Ambrosia's included license limits unregistered use to 30 days from receipt;
continued use after that period requires registration.
