---
id: jigsaw-r3
kind: game
title: Jigsaw (Release 3)
summary: >-
  Explore Century Park on New Year's Eve 1999 in Graham Nelson's interactive history.
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
      Space began the story in Century Park. Typing LOOK and Return printed the
      location description again at a new prompt. The release browser fetched the
      original archive once and reproduced the command response.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3906
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/adv/jigsaw.hqx
    expected_sha256: 0d03e49151e16035c5ef80ca6f39bb09e20a0b5259209eebabe9b01213feb478
    expected_size: 485090
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=13925
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/adv/jigsaw.hqx
    rights_holder: Graham Nelson
    permission: >-
      The author's Info-Mac submission permits free, unchanged distribution and use
      without profit. This is the complete original BinHex/StuffIt package, offered
      without charge.
    notes: >-
      Original 485,090-byte Info-Mac archive; SHA-256
      0d03e49151e16035c5ef80ca6f39bb09e20a0b5259209eebabe9b01213feb478.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/jigsaw-r3/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3906
    permission: >-
      Fresh Systemless gameplay capture from the unchanged original package.
      Underlying story text remains its author's property.
    notes: >-
      484-by-448 MaxZip story-window crop at (31,121) from an 800-by-600
      deterministic framebuffer after the LOOK command.
references:
- https://info-mac.org/viewtopic.php?t=13925
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/adv/jigsaw.hqx
---

## A century about to turn

![Jigsaw story window after the LOOK command](incoming/jigsaw-r3/gameplay.png)

Press **Space** at the welcome screen to begin in Century Park. Type a command at
the `>` prompt and press **Return**. Try `look` to examine the park before
exploring the paths around it.

This release includes the story and its Macintosh MaxZip interpreter in the
original archive. The author permits unchanged, non-profit distribution.
