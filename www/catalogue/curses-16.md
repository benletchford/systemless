---
id: curses-16
kind: game
title: Curses (Release 16)
summary: Explore an attic and a sprawling family mystery in Graham Nelson's text adventure.
developer: Graham Nelson
publisher: Graham Nelson
year: 1995
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
    systemless_version: 0.73.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged Info-Mac package opened its bundled 68K MaxZip interpreter.
      Space began the story in the attic. Typing LOOK and Return printed the room
      description again and advanced the turn counter from 1 to 2. The release
      browser fetched the original archive once and reproduced the same command
      response and turn advancement.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3902
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/adv/curses-16.hqx
    expected_sha256: c7e62efff2e028299241c7ccec6ace6cca113c40c8f11ba02053675a61521025
    expected_size: 445233
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=13925
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/adv/curses-16.hqx
    rights_holder: Graham Nelson
    permission: >-
      The author's Info-Mac submission permits free, unchanged distribution and use
      without profit. This is the complete original BinHex/StuffIt package, offered
      without charge.
    notes: >-
      Original 445,233-byte Info-Mac archive; SHA-256
      c7e62efff2e028299241c7ccec6ace6cca113c40c8f11ba02053675a61521025.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/curses-16/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3902
    permission: >-
      Fresh Systemless gameplay capture from the unchanged original package.
      Underlying story text remains its author's property.
    notes: >-
      484-by-448 MaxZip story-window crop at (31,121) from an 800-by-600
      deterministic framebuffer after the LOOK command.
references:
- https://info-mac.org/viewtopic.php?t=13925
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/adv/curses-16.hqx
---

## An attic with a way down

![Curses story window after the LOOK command](incoming/curses-16/gameplay.png)

Press **Space** at the welcome screen to begin in the attic. Type a command at
the `>` prompt and press **Return**. Try `look` to examine the room, then
investigate the open trapdoor and the rest of the house.

This release includes the story and its Macintosh MaxZip interpreter in the
original archive. The author permits unchanged, non-profit distribution.
