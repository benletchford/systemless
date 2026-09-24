---
id: out-of-this-world
kind: game
title: Out of This World
summary: Escape an alien world in MacPlay's limited cinematic platformer demo.
developer: Delphine Software
publisher: MacPlay
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.56.0"
    architecture: 68k
    environment: >-
      Deterministic play of the unchanged 68k MacPlay demo at 800 by 600. The welcome
      card and 640-by-400 window-size selector render. After the opening cinematic
      returns to the welcome card, Return starts the underwater level. Holding keypad 8
      swims the player to the surface and reaches the outdoor area in a deterministic
      run. A local browser preview of the same archive rendered the startup dialogs and
      underwater level, accepted Return and Shift, and displayed the game's
      death/continue flow.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2605
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: cb93f4ae64a70a1b00f7bab9e9ba1ce66bb16e2238f74302a605331d3f2a01c9
    size_bytes: 754038
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/out-of-this-world-another-world
    - >-
      https://static.classicmacdemos.com/demos/out-of-this-world-another-world/README.txt
    rights_holder: Delphine Software, MacPlay/Interplay, and successors
    permission: >-
      This is the unchanged, intentionally limited MacPlay promotional demo. Its own
      startup card calls it a fully playable version of the first segment, says that
      play returns to the demo card at the end, and invites purchase of the full game. It
      is not a retail copy or modern port; the archive does not state a broader
      redistribution licence.
    notes: >-
      Downloaded 754,038-byte StuffIt 5 archive, SHA-256
      cb93f4ae64a70a1b00f7bab9e9ba1ce66bb16e2238f74302a605331d3f2a01c9. The archive contains Out of this World Demo
      1.0.1 and a limited set of game data files. Its outer StuffIt packaging may be
      an archival repack; no retail data or altered executable was identified.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 7d136db59265aa8342d9576d1c493670bef171ffe2ae194a25f1c16edcd9d564
    size_bytes: 20075
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2605
    permission: >-
      Fresh gameplay capture made for this catalogue from the unchanged demo.
      Underlying game artwork remains the property of its rights holders.
    notes: >-
      Deterministic capture after the player surfaces with keypad 8, cropped from the
      800-by-600 framebuffer to the 640-by-400 game content surface without altering
      game pixels. PNG SHA-256
      7d136db59265aa8342d9576d1c493670bef171ffe2ae194a25f1c16edcd9d564, 20,075 bytes.
references:
- https://classicmacdemos.com/out-of-this-world-another-world
- https://github.com/benletchford/systemless/issues/2605
---

## A playable first chapter

![Out of This World demo outdoor gameplay](https://assets.systemless.org/catalogue/media/sha256/7d/7d136db59265aa8342d9576d1c493670bef171ffe2ae194a25f1c16edcd9d564.png)

MacPlay's original Macintosh demo offers the opening segment of Eric Chahi's
cinematic platformer. The startup card explains the movement and action keys
and directs players to the retail game for the full adventure.

The unchanged demo reaches the outdoor area in Systemless. Press Return on the
welcome card, choose 640 × 400, and press Return during or after the opening
cinematic to reach the welcome card again. Press Return once more to enter the
level. Hold keypad 8 or the Up arrow on the first playable screen to swim
upward; brief taps may not be enough. Shift continues after a death.
