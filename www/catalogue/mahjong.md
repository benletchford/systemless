---
id: mahjong
kind: game
title: Mahjong
summary: Play a four-player Mahjong deal against three computer opponents.
developer: Matti Larborn
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: f1110f94ca3f7492c360a0fa357659160b650504
    architecture: 68k
    environment: >-
      Optimized native build with STi3 extraction repair; unchanged original 1.0.5
      International archive, exact runtime selector below, explicit 68K/depth 8. Normal
      DEMO dismissal, New Game and three computer players deal a table. Player discard
      produces computer responses and a visible North-wind meld. Fresh replay passes
      fourteen measured before/after assertions at 1167 frontend / 1767 guest ticks, zero
      exhausted frames. Captures inspected. Production extraction matches all five
      installed files, ten original forks and supported Finder metadata against independent
      unar. Six focused extraction tests pass. Full deals/games, saves and audio
      fidelity remain unverified. Browser qualification, asset promotion, launch approval,
      human release and independent public replay remain pending. Trial restrictions are
      retained without patches.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4674
runtime:
  executable_path: Mahjong 1.0.5/Mahjong 1.0.5/Mahjong 1.0.5 INT
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 8bcbe73e845f41a2318a00d50ebf55b977fb4bdee9b28e7603f8212736c30fa6
    size_bytes: 201942
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/m/
    - https://www.vintageapplemac.com/files/games/Mahjong%201.0.5.sit
    - https://github.com/benletchford/systemless/issues/4674
    license: Original noncommercial shareware-package distribution permission
    rights_holder: Matti Larborn
    permission: >-
      Original About Mahjong expressly permits distribution in shareware packages
      without commercial interest. It also permits magazine CD-ROM/diskette distribution;
      other commercial redistribution requires written permission. Preserve the
      complete original package, notices and trial restrictions.
    notes: "Unchanged original archive: 201942 bytes, SHA-256 8bcbe73e845f41a2318a00d50ebf55b977fb4bdee9b28e7603f8212736c30fa6. Gratis noncommercial preservation delivery of the unchanged distribution, separately from the emulator binary. All five installed files/ten forks and Finder metadata match independent extraction. Actual game has resource-only CODE 0/1/2/3 and no cfrg or native PPC slice. Trial allows one round, retains reminders and locks saves; no registration patch or key. This is a distinct four-player game, not MacTaipei solitaire or Exitdata's Mah Jong Parlour."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 54512df4f4980ad507ef9b04a887568a0a357292c1995db853ded37e1664232d
    size_bytes: 9561
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4674
    license: Original noncommercial shareware-package distribution permission
    rights_holder: Matti Larborn
    permission: Original game content accompanying the complete permitted package.
    notes: >-
      Exact original native archive after ordinary discard and computer responses;
      cropped to the game's content without desktop, window frame/title or menu bar.
references:
- https://www.vintageapplemac.com/software/games/m/
- https://github.com/benletchford/systemless/issues/4672
- https://github.com/benletchford/systemless/issues/4674
---

Play **Mahjong 1.0.5 International**, Matti Larborn's four-player game.
Choose **DEMO** in the original registration dialog, then **File → New Game**.
Leave Player 1 human and mark the other three players as Computer.
Click a tile in your hand to discard it and respond to any offered bids.

Trial setup, a player discard and computer responses are verified natively.
The original one-round trial, registration reminders and locked saves remain.
Browser launch is awaiting qualification.

![Mahjong after a discard and computer responses](https://assets.systemless.org/catalogue/media/sha256/54/54512df4f4980ad507ef9b04a887568a0a357292c1995db853ded37e1664232d.png)
