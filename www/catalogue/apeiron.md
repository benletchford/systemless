---
id: apeiron
kind: game
title: Apeiron
summary: >-
  Defend your crystal from a relentless garden of animated pests in Ambrosia's
  original 30-day Macintosh shareware trial.
developer: Andrew Welch
publisher: Ambrosia Software
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.51.0"
    architecture: 68k
    environment: >-
      Deterministic run from the unchanged 1.0.2 BinHex installer through installation,
      automatic handoff, the unregistered shareware notice, and live Wave 1 gameplay
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2450
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.funet.fi/pub/mirrors/info-mac.org/_Game/arc/apeiron-102.hqx
    download_page: https://ftp.funet.fi/pub/mirrors/info-mac.org/_Game/arc/
    expected_sha256: 80ed7f8c9da216d2163ddd76e326bc72e29b3c2e6963b3a823efebed47f79804
    expected_size: 3320809
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mirrors/info-mac.org/_Game/arc/apeiron-102.hqx
    - https://groups.google.com/g/comp.sys.mac.games/c/89G3QB08HYI
    license: Ambrosia Software Apeiron 30-day shareware license
    rights_holder: Ambrosia Software
    permission: >-
      The included Apeiron License permits non-profit distribution of the complete,
      unmodified software without prior written notice. It grants use of the
      unregistered game for 30 days from receipt, after which registration is required.
      This is the unchanged author-distributed installer, with no registration code.
    notes: >-
      The Info-Mac submission identifies this as Apeiron 1.0.2 and lists its fixes.
      The unchanged 3,320,809-byte BinHex file has SHA-256
      80ed7f8c9da216d2163ddd76e326bc72e29b3c2e6963b3a823efebed47f79804.
      Its 68K StuffIt Installer Maker application installs the game and supporting
      files before Systemless hands off to Apeiron.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/apeiron/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2450
    permission: >-
      Original in-game screenshot captured for this catalogue from the unregistered
      shareware trial. Underlying Apeiron artwork remains Ambrosia's property.
    notes: >-
      Fresh 640x480 game-content capture from the exact installer after choosing
      Play Apeiron in Systemless 0.51.0 on 2026-09-23. PNG SHA-256
      4650a1b2918e2c7490c293c541a95b40f560f188a291637fed7379b97bc4ae13;
      903,705 bytes.
references:
- https://groups.google.com/g/comp.sys.mac.games/c/89G3QB08HYI
- https://ftp.funet.fi/pub/mirrors/info-mac.org/_Game/arc/apeiron-102.hqx
---

## A garden that fights back

![Apeiron shareware trial during Wave 1](incoming/apeiron/gameplay.png)

Apeiron turns a familiar arcade idea into a noisy, colourful contest with a
garden full of moving targets. Protect your crystal, shoot through mushrooms,
and gather power-ups while the Pentipede and its companions close in.

This is Ambrosia's original Macintosh shareware release, not a registered or
unlocked copy. On first launch, select **Install**; Systemless then opens the
installed game. Choose **Not Yet** at the registration notice to try it, then
**Play Apeiron**. Ambrosia's included license limits unregistered use to 30
days from receipt. If you want to keep using Apeiron after that period, its
license requires registration.
