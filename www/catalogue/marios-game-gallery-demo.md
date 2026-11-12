---
id: marios-game-gallery-demo
kind: game
title: Mario's Game Gallery Demo
summary: Play Go Fish against Mario in the original Macintosh card-game demo.
developer: Presage Software
publisher: MacPlay
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: dcf21abe native
    architecture: 68k
    environment: >-
      Original unchanged archive, original preferred partition, default machine,
      display and sound. Click the title, hover/click Go Fish and accept the default player
      name. Wait through Mario’s first turn, then select the Boo card. Matched
      active/omission runs start with identical RGB frames. The active run says Go fish! while
      the control gives an idle reminder; final frames differ by 3493 pixels. Both
      complete 4416 frontend / 5016 guest ticks with zero exhausted frames and zero script
      assertions. Actual images and external RGB comparisons inspected. An earlier
      click during Mario’s turn produced identical results and is not positive evidence.
      Bounded game entry and card-selection response only; no game completion, saves,
      audio fidelity, independent hardware or browser pacing claim.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6178
runtime:
  executable_path: Mario's Game Gallery/Mario's Game Gallery
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: eb2d0792afceb37b661fb8b34447fcdd7c09cfe11ae85e9ad41d2b6dffac035e
    size_bytes: 2400571
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/marios-game-gallery
    - >-
      https://discmaster.textfiles.com/browse/12017/IMG%2029%20JulyAug%201995.iso/Demos/Mario%27s%20Game%20Gallery
    - https://archive.org/details/IMG29JulyAug1995
    - https://github.com/benletchford/systemless/issues/6178
    license: Maintainer-accepted promotional demo
    rights_holder: MacPlay, Presage Software, Nintendo and original contributors
    permission: >-
      Accepted under the existing project policy for complete unchanged original
      promotional demos preserved from period cover-disc distribution. This is
      project-policy acceptance, not an affirmative publisher, Nintendo or third-party licence
      grant. No applicable explicit redistribution restriction was identified in the
      reviewed notices. Preserve both files, credits, notices and Go Fish-only demo limits.
    notes: >-
      Complete unchanged two-file web demo. The application’s empty data fork, full
      3319662-byte resource fork with 567 records, type/creator and Finder flags match
      the July/August 1995 Inside Mac Games disc. Readme data is byte-identical; its
      sole styl resource has not independently been compared with the period file, so
      whole-package byte identity is not claimed. Independent production packing matches
      two files, four forks and two Finder records with strict EOF. Full bundled Readme,
      all standard TEXT/STR/STR# and DITL text reviewed; Readme resource contains only
      styl metadata. No game code or opaque scripts decoded.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 61075cf8b328a40e771574a29a3a3d1a453c8b496f5fe9a2747532fa82f7316e
    size_bytes: 32780
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6178
    rights_holder: MacPlay, Presage Software, Nintendo and original contributors
    permission: Real content-only capture of the project-policy-accepted original demo.
    notes: >-
      Unscaled, unretouched 512x384 lossless crop of actual native Go Fish play after
      a card selection; excludes desktop and Classic menu bar. No separate artwork
      licence claimed.
references:
- https://classicmacdemos.com/marios-game-gallery
- >-
  https://discmaster.textfiles.com/browse/12017/IMG%2029%20JulyAug%201995.iso/Demos/Mario%27s%20Game%20Gallery
- https://www.mobygames.com/game/9416/marios-game-gallery/releases/
- https://github.com/benletchford/systemless/issues/6178
---

Play **Go Fish** against Mario in this original Macintosh demo. The other games on the main menu require the full release.

Click the title to reach the menu, choose **Go Fish**, and accept or enter your name. Wait for Mario’s opening turn. On your turn, click a card to ask Mario for that kind; if he says **Go fish!**, click the deck to draw. Collect four matching cards to make a book.

Native game entry and a card-selection response are verified. Browser qualification is pending.

![Playing Go Fish with Mario](https://assets.systemless.org/catalogue/media/sha256/61/61075cf8b328a40e771574a29a3a3d1a453c8b496f5fe9a2747532fa82f7316e.png)
