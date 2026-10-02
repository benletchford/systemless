---
id: sunset-over-savannah
kind: game
title: Sunset Over Savannah
summary: >-
  Walk a Savannah beach on the last day of a vacation in Ivan Cockrum's
  existential text adventure.
developer: Ivan Cockrum
publisher: Ivan Cockrum
year: 1997
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
      The unchanged Info-Mac package opened its bundled 68K MaxTADS interpreter at
      Beach Path. Space cleared the introductory page. Typing FREEWARE displayed the
      author's distribution terms; LOOK and Return repeated the beach description at a new
      prompt. The release browser fetched the original archive once and reproduced
      these command responses.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3910
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 9b2a5c68aeb46393ff095aa9f4303f017688ba2a766d9d1f10416849dacf867b
    size_bytes: 734853
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=11456
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/sunset-over-savannah.hqx
    rights_holder: Ivan Cockrum
    permission: >-
      The game's FREEWARE command permits distribution without profit while retaining
      copyright. The author requests advance notice for CD-ROM collections; this
      browser distribution is not a CD-ROM collection.
    notes: >-
      Original 734,853-byte Info-Mac archive; SHA-256
      9b2a5c68aeb46393ff095aa9f4303f017688ba2a766d9d1f10416849dacf867b.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 77c8edb280b338174aa517ceb0f6aa5c23e012fe5d855465a4030f75a3dabc1c
    size_bytes: 29083
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3910
    permission: >-
      Fresh Systemless gameplay capture from the unchanged original package.
      Underlying story text remains its author's property.
    notes: >-
      484-by-537 MaxTADS window crop at (31,33) from an 800-by-600 deterministic
      framebuffer after the FREEWARE and LOOK commands.
references:
- https://info-mac.org/viewtopic.php?t=11456
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/sunset-over-savannah.hqx
---

## One last walk on the beach

![Sunset Over Savannah after the LOOK command](https://assets.systemless.org/catalogue/media/sha256/77/77c8edb280b338174aa517ceb0f6aa5c23e012fe5d855465a4030f75a3dabc1c.png)

Press **Space** to clear the introductory text. At the `>` prompt, type a
command and press **Return**. Try `look` to examine Beach Path, or `help` for
online hints. Type `freeware` to read the author's distribution terms.

The original archive contains the story and its Macintosh MaxTADS interpreter.
The author permits non-profit distribution.
