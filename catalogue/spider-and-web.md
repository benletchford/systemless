---
id: spider-and-web
kind: game
title: Spider and Web
summary: >-
  Infiltrate an enemy stronghold, deploy covert gadgets, and reconstruct tactical
  decisions under interrogation in Andrew Plotkin's award-winning espionage
  interactive fiction masterpiece.
developer: Andrew Plotkin
publisher: Andrew Plotkin
year: 1998
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
    evidence: https://github.com/benletchford/systemless.org/issues/315
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 8d8881ca30e44e2fc4718fafb4811588d1c220df3aa8c0dd1f118db9f21db885
    size_bytes: 388084
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/spider-and-web.hqx
    license: Freeware
    rights_holder: Andrew Plotkin
    permission: "Spider And Web is distributed under an explicit freeware grant: \"Spider And Web is copyright 1997-8 by Andrew Plotkin. It may be copied, distributed, and played freely.\" The author also states \"This game is free.\" The bundled MaxZip runtime by Andrew Plotkin and Mark Howell is likewise freeware (\"1995-8 Andrew Plotkin. Freeware -- enjoy... the entirety of the unbound MaxZip application can be distributed and used freely\"), submitted to Info-Mac without commercial or CD-ROM exclusions."
    notes: >-
      Untouched Info-Mac BinHex distribution archive retrieved from
      mirrors.nic.funet.fi. The 388,084-byte file has SHA-256
      8d8881ca30e44e2fc4718fafb4811588d1c220df3aa8c0dd1f118db9f21db885. Created by Andrew Plotkin (Zarf) in 1998, bundled with
      the native PowerPC MaxZip interpreter.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: f0348d24ea8969ecfd3a0b5c4b67ce64649063235e413b7eb91f663b0f7ecb0b
    size_bytes: 11919
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/315
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork and text remain the property of their rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged distribution archive on
      2026-09-19, cropped exactly to the MaxZip story window's 480-by-428 content surface. It
      excludes the Classic Mac menu bar, window title bar, browser, website, host
      desktop and emulator chrome.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/spider-and-web.hqx
---

## Espionage narrative and interactive framing

![Spider and Web](https://assets.systemless.org/catalogue/media/sha256/f0/f0348d24ea8969ecfd3a0b5c4b67ce64649063235e413b7eb91f663b0f7ecb0b.png)

Released in 1998 by prolific interactive fiction designer Andrew Plotkin (known within the community as *Zarf*), *Spider and Web* is widely recognized as one of the pinnacle achievements in interactive storytelling and puzzle design. Sweeping the 1998 XYZZY Awards with honors for Best Game, Best Writing, Best Individual Puzzle, Best Individual PC, and Best Individual NPC, the work departed sharply from traditional fantasy dungeon crawls to deliver a cerebral, tension-soaked espionage thriller.

The narrative unfolds through an ingenious non-linear framing device: the protagonist has already been captured and is bound to a high-tech interrogation chair in an enemy fortress. As an interrogator probes your memory, gameplay consists of interactive flashbacks where you reenact your infiltration into the compound. If your account contradicts physical evidence or leads to failure, the interrogator abruptly catches you in the lie, rewinding the simulation and challenging you to tell the real sequence of events.

## Spycraft gadgets and environmental deduction

Rather than relying on classic inventory accumulation or lock-and-key puzzles, *Spider and Web* centers on understanding and deploying specialized spy equipment:
- **Sophisticated Tools:** Experiment with subtle electronic gadgets, fiber-optic probes, acoustic resonators, and explosive devices whose operation must be deduced through careful observation.
- **Organic Dialogue System:** In lieu of cumbersome conversation menus or keyword parser prompts, interrogation exchanges are streamlined into concise affirmative, negative, or silent responses, heightening dramatic immersion.
- **Layered Puzzles:** Puzzles demand rigorous spatial thinking and tactical timing, culminating in what critics and players consider one of the most brilliant climactic puzzle sequences in video game history.

## Emulation and execution in Systemless

Systemless executes *Spider and Web* via PowerPC CPU emulation directly in modern web browsers. The runtime emulates the native PowerPC PEF executable bundled with Plotkin's MaxZip interpreter, executing QuickDraw text rendering, AppleEvents process dispatching, and asynchronous event streams with zero plugin dependencies.
