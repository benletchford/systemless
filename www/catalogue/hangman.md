---
id: hangman
kind: game
title: HangMan
summary: >-
  Guess letters to uncover the secret word, earn points and continue through
  successive rounds.
developer: Al Staffieri Jr.
year: 2003
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
    environment: "Complete unchanged original, exact executable, 68K/default 25 MHz/800x600/depth 8. Click Begin Game, type E and Return: two E letters appear and score increases 0 to 20. Equal-clock omitted-E control retains the blank word and score 0. Before-input captures match. Fresh repeats each pass eight measured causal pixels and two exact prior captures at frontend 552/guest 1148, zero exhausted frames. Actual captures inspected. Own-run consistency is not hardware accuracy. Native binary SHA-256 35cf8fcada95a08951a732ff0d92f0b816c2291aeb1067c6ee84788d94b85f34. Full games, tournaments, high-score persistence, audio fidelity, browser pacing and released public play remain unverified. Launch disabled pending browser qualification."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5779
runtime:
  executable_path: HANGMAN folder/HANGMAN 1.6
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 2ae16af42c0e4c28b2b96361a7435d352de0fc5c10ae093388276a75d400408b
    size_bytes: 53376
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/d/sw/games/hangman/c/HANGMAN.sit
    - https://www.gryphel.com/c/sw/games/hangman/
    - https://github.com/benletchford/systemless/issues/5779
    license: Original unchanged fee-free distribution terms
    rights_holder: Al Staffieri Jr.
    permission: >-
      The complete original ReadMe explicitly permits any number of copies provided
      the application is unchanged, no fee is charged and the ReadMe accompanies it.
      Preserve the whole original archive and notices. The 4772-byte ReadMe was read in
      full.
    notes: >-
      Independent extraction versus production packing matches two paths, four forks
      and two Finder records, strict KPK end 90141, pack SHA-256
      ea99b63faf71921cf013be3b025c89a95dbcde01ebdb75000f2902bc24dcb0ed. Published MD5 matches. Original
      version 1.6 is dated 6 April 2003; original classic compiled FutureBASIC runtime is
      embedded, no external runtime supplied.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 6a67a41250b4f7bb215dc15a3383da24e06468ddf388b9b35a6a074828c9547b
    size_bytes: 2396
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5779
    rights_holder: Al Staffieri Jr.
    permission: Fresh content-only capture from the unchanged distributable original.
    notes: >-
      Exact crop (147,42)-(653,338), 506x296, excluding host and Classic window
      chrome, no resampling. Shows two E letters and score 20 after an ordinary guess.
references:
- https://www.gryphel.com/c/sw/games/hangman/
- https://github.com/benletchford/systemless/issues/5779
---

![HangMan after guessing E](https://assets.systemless.org/catalogue/media/sha256/6a/6a67a41250b4f7bb215dc15a3383da24e06468ddf388b9b35a6a074828c9547b.png)

Click Begin Game, type a letter and press Return. Correct guesses reveal letters and earn points; wrong guesses add to the drawing. Complete the word to advance to the next round. The File menu also offers a tournament mode.
