---
id: abuse
kind: game
title: Abuse
summary: Run, aim and fight through the opening levels of Crack dot Com's dark science-fiction action game.
developer: Crack dot Com
publisher: Bungie
year: 1996
architectures: [68k]
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-20
    tester: Catalogue maintainer
    systemless_version: 0.42.1
    architecture: 68k
    environment: Deterministic headless gameplay run from the original StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 77a8512d02f84c5e973167aeaed023a1fc63b147c79730c24455d319a8ed1521
    size_bytes: 3290510
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/abuse
    - https://sources.debian.org/src/abuse/2.00-12/README
    license: Crack dot Com public-domain shareware-data release
    rights_holder: Crack dot Com; sound effects by Bobby Prince
    permission: >-
      Crack dot Com's release notice places the shareware data other than its
      third-party WAV files in the public domain. A notice included in this
      demo permits its bundled Lisp, sound effects and artwork to be
      distributed only as a complete package with no files missing or changed.
    notes: >-
      Unchanged 3,290,510-byte Macintosh demo archive, SHA-256
      77a8512d02f84c5e973167aeaed023a1fc63b147c79730c24455d319a8ed1521.
references:
- https://classicmacdemos.com/abuse
- https://static.classicmacdemos.com/demos/abuse/README.txt
- https://sources.debian.org/src/abuse/2.00-12/README
---

## Escape the prison

Abuse combines keyboard movement with independent mouse aiming in a fast,
side-scrolling action game. This is the original Macintosh demo package,
preserved unchanged. Systemless reaches a new game and responds to sustained
movement through the first level in a deterministic gameplay run.
