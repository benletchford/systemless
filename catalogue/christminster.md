---
id: christminster
kind: game
title: Christminster
summary: >-
  Investigate a missing scholar and unravel an alchemical collegiate conspiracy
  across the historic courtyards and catacombs of Biblioll College in Gareth Rees's
  celebrated interactive mystery.
developer: Gareth Rees
publisher: Gareth Rees
year: 1995
architectures:
- ppc
default_architecture: ppc
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-19
    tester: Catalogue maintainer
    systemless_version: 0.41.13
    architecture: ppc
    environment: Deterministic headless run from the Info-Mac mirror distribution archive
    status: playable
    evidence: https://github.com/benletchford/systemless.org/issues/317
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 0b5f9f01438a76e0aec70569de605aac1c04256ba1aa08fc5a1a26894de73e36
    size_bytes: 401199
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/christminster-30.hqx
    license: Freeware
    rights_holder: Gareth Rees
    permission: "Christminster is distributed under an explicit open redistribution licence: \"2. You may copy and distribute verbatim copies of 'Christminster' in any medium.\" The game is provided free of charge without warranty or fees, and the bundled PowerPC MaxZip interpreter by Andrew Plotkin is explicitly marked \"Freeware -- enjoy.\" The archive was submitted to Info-Mac without commercial or CD-ROM exclusions."
    notes: >-
      Untouched Info-Mac BinHex distribution archive retrieved from
      mirrors.nic.funet.fi. The 401,199-byte file has SHA-256
      0b5f9f01438a76e0aec70569de605aac1c04256ba1aa08fc5a1a26894de73e36. Created by Gareth Rees in 1995 and bundled with Andrew
      Plotkin's PowerPC MaxZip interpreter.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: de553fb9e8f4155ccb9e4ff8f1a25d3308fea4aa5ac5377965676508eb612cfa
    size_bytes: 9848
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/317
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork and text remain the property of their rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged distribution archive on
      2026-09-19, cropped exactly to the MaxZip story window's 480-by-428 content surface. It
      excludes the Classic Mac menu bar, window title bar, browser, website, host
      desktop and emulator chrome.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/christminster-30.hqx
---

## Academic intrigue and collegiate mystery

![Christminster](https://assets.systemless.org/catalogue/media/sha256/de/de553fb9e8f4155ccb9e4ff8f1a25d3308fea4aa5ac5377965676508eb612cfa.png)

Released in 1995 by Gareth Rees, *Christminster* is universally acclaimed as one of the defining interactive fiction works of the 1990s. Set against the sunlit spires and shadowed quads of the fictional university town of Christminster (inspired by Oxford and Cambridge), the game casts players as Christabel Spencer. Arriving on a summer morning to visit her brother Malcolm—a fellow at prestigious Biblioll College—she discovers his rooms ransacked, his fellowship colleagues tight-lipped, and a sinister academic conspiracy quietly unfolding behind locked doors.

Rather than relying on arbitrary item combinations, *Christminster* immerses players in meticulous historical atmosphere and intellectual deduction:
- **Rich Campus Geography:** Explore Victorian-era chapel crypts, ancient collegiate libraries, dining halls, faculty chambers, and cobblestone quads steeped in centuries of academic ritual.
- **Hermetic and Alchemical Lore:** Uncover lost alchemical manuscripts, translate archaic Latin inscriptions, brew experimental reagents, and decipher obscure cosmological diagrams left by medieval scholars.
- **Memorable Cast:** Converse with collegiate porters, eccentric deans, haughty fellows, and town locals using nuanced contextual interaction.

## Groundbreaking puzzle architecture

*Christminster* was among the first major works created with Graham Nelson's Inform compiler to demonstrate that interactive fiction could match the literary depth of classic Infocom titles while introducing fair, modern puzzle design:
- **Fair Deductions:** Obstacles are grounded in physical logic and thematic scholarship, avoiding the arbitrary dead ends common in early text games.
- **Branching Investigation:** Solve multifaceted collegiate mysteries at your own pace, gathering evidence and unraveling the dark truth behind Malcolm's sudden disappearance.

## Emulation and execution in Systemless

Systemless executes *Christminster* directly in web browsers via native PowerPC emulation. The runtime executes Andrew Plotkin's MaxZip interpreter, delivering crisp QuickDraw typography, responsive keyboard parsing, and authentic Classic Mac OS document windowing without native host dependencies.
