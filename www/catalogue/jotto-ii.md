---
id: jotto-ii
kind: game
title: Jotto ][
summary: >-
  Deduce a five- or six-letter secret word from the letter counts returned for
  your guesses.
developer: Mark Pilgrim
publisher: MerriMac Software Group
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 1285257185892f593f30d83599def53e7a005a1b native candidate
    architecture: 68k
    environment: "Unchanged original, exact executable, 68K/default 25 MHz/800x600/depth 8. Accept optional blank personalization, Command-N, wait for original wipe, type TRIES and Return: guess appears in notebook with feedback 2. Equal-clock omitted-Return control retains TRIES in entry boxes and an empty notebook. Before-input images match. Each fresh repeat passes eight measured causal pixels and two exact prior screenshots at frontend 952/guest 1552, zero exhaustion. Actual captures inspected. Own-run consistency is not a hardware oracle. Native binary SHA-256 35cf8fcada95a08951a732ff0d92f0b816c2291aeb1067c6ee84788d94b85f34. Browser pacing, complete games, six-letter mode, persistence and audio fidelity remain unverified; launch disabled pending browser qualification."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5777
runtime:
  executable_path: Jotto ][ 1.2.cpt/Jotto ][
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: c0d26b059b258a8e2b7f05e17007e5191dcd1823ab22d943ff99531f74a2b906
    size_bytes: 309728
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/d/sw/games/jotto/c/jotto-ii-12.hqx
    - https://github.com/benletchford/systemless/issues/5777
    license: GPL-2.0-or-later
    rights_holder: Mark Pilgrim
    permission: >-
      Original TEXT 640 explicitly permits redistribution under GPL version 2 or
      later. Preserve both unchanged complete archives and their identical GPL copies;
      offer corresponding source alongside the binary.
    notes: >-
      Binary preservation matches eight paths, sixteen forks and eight Finder
      records, strict KPK end 434449. Source contains all 95 project C files, 43 headers,
      project and resource inputs. All 121 source resources match the binary exactly; no
      separate custom compiled control/window components found. Only compiler/system SDK
      dependencies remain. Independent source-archive preservation also matches all
      148 paths, 296 forks and 148 Finder records, strict KPK end 755222.
- id: source-code
  role: supplement
  format: hqx
  source:
    type: sha256
    sha256: eac11cc7398fdeddc483c67f920a379b79966ee5e9a40d572c03c5a3f12aa8ac
    size_bytes: 482749
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/d/sw/games/jotto/c/jotto-ii-12-c.hqx
    - https://github.com/benletchford/systemless/issues/5777
    license: GPL-2.0-or-later
    rights_holder: Mark Pilgrim
    permission: >-
      Original TEXT 640 explicitly permits redistribution under GPL version 2 or
      later. Preserve both unchanged complete archives and their identical GPL copies;
      offer corresponding source alongside the binary.
    notes: >-
      Binary preservation matches eight paths, sixteen forks and eight Finder
      records, strict KPK end 434449. Source contains all 95 project C files, 43 headers,
      project and resource inputs. All 121 source resources match the binary exactly; no
      separate custom compiled control/window components found. Only compiler/system SDK
      dependencies remain. Independent source-archive preservation also matches all
      148 paths, 296 forks and 148 Finder records, strict KPK end 755222.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: a45f8b9b402553de5885cd5672336265a0d9f232646d1f2eb99feb87d5f3f8b0
    size_bytes: 15320
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5777
    license: GPL-2.0-or-later
    rights_holder: Mark Pilgrim
    permission: Fresh capture from the unchanged GPL original.
    notes: >-
      Exact native content crop (250,169)-(550,470), 300x301, no resampling or host
      chrome. Shows submitted TRIES and feedback.
references:
- https://www.gryphel.com/c/sw/games/jotto/
- https://github.com/benletchford/systemless/issues/5777
---

![Jotto notebook after submitting TRIES](https://assets.systemless.org/catalogue/media/sha256/a4/a45f8b9b402553de5885cd5672336265a0d9f232646d1f2eb99feb87d5f3f8b0.png)

Accept the optional name dialog and choose New Game with Command-N. Type a valid word and press Return; the feedback tells you how many letters occur in the secret word. You have fifteen guesses. Five-letter and six-letter modes are available in Options.

The original program is free software under GPL version 2 or later. The corresponding original source package is included as a download alongside the game.
