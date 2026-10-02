---
id: path-to-fortune-r3
kind: game
title: The Path to Fortune (Release 3)
summary: >-
  Explore Windhall and seek a way to save its mayor's house in a fantasy text adventure.
developer: C.E. Forman
publisher: Jeff Cassidy and C.E. Forman
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.74.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged Info-Mac package opened its bundled 68K MaxZip interpreter.
      Answering N to the resume prompt and advancing the introduction reached Central
      Windhall. Typing LOOK and Return printed the town description again at a new
      prompt. The release browser fetched the original archive once and reproduced
      the command response.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3908
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/adv/path-to-fortune.hqx
    expected_sha256: 4b23687e4ccbdf35c05e57747dfcf1acb2992935a8512161197b637fbf909cfa
    expected_size: 475234
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=13925
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/adv/path-to-fortune.hqx
    rights_holder: Jeff Cassidy and C.E. Forman
    permission: >-
      The author-submitted Info-Mac description permits free distribution of the
      game without profit. The separately offered maps and hint book are excluded.
    notes: >-
      Original 475,234-byte Info-Mac archive; SHA-256
      4b23687e4ccbdf35c05e57747dfcf1acb2992935a8512161197b637fbf909cfa.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/path-to-fortune-r3/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3908
    permission: >-
      Fresh Systemless gameplay capture from the unchanged original package.
      Underlying story text remains its authors' property.
    notes: >-
      484-by-448 MaxZip story-window crop at (31,121) from an 800-by-600
      deterministic framebuffer after the LOOK command.
references:
- https://info-mac.org/viewtopic.php?t=13925
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/adv/path-to-fortune.hqx
---

## Windhall needs your help

![The Path to Fortune story window after the LOOK command](incoming/path-to-fortune-r3/gameplay.png)

Answer **N** at the saved-game prompt, then press any key to advance the
introduction. At the `>` prompt in Central Windhall, type a command and press
**Return**. Try `look` to examine the town before exploring its roads.

This release includes the story and its Macintosh MaxZip interpreter in the
original archive. The game may be shared without profit; the separate maps and
hint book are not included.
