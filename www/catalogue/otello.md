---
id: otello
kind: game
title: Otello 2.0
summary: >-
  Play a classic disc-flipping strategy game against the Macintosh or another
  player.
developer: Ron Hayter
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: Combined 2353ccaa and PRs 5388, 5389, 5391
    architecture: 68k
    environment: >-
      The exact original Info-Mac archive ran natively at depth 8. A legal white
      placement flipped a black disc, and the Macintosh replied; inspected captures show
      three discs for each side. Five measured before-and-after pixel assertions passed
      at frontend tick 422 and guest tick 1022 with zero exhausted frames. Full games,
      settings, saves and audio remain unverified. Hosted browser qualification is
      pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5394
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 057ab64e7c85d406c2c8a339f6aaa809e9015afb with preview launch flag
    architecture: 68k
    environment: >-
      Three release-mode Chromium 156 worker/WebGL previews at the production
      origin used a locally substituted frontend and worker assets. The actual
      managed archive response matched its pinned size and SHA-256. A legal
      white move, flipped disc and Macintosh reply are visible and confirmed by
      measured pixels. Twenty-second samples reached 60.045, 60.271 and 61.008
      guest ticks per second without recorded console errors. The page and worker
      were verified to load the same locally built Wasm. This approves bounded
      browser launch; an unmodified deployed replay, complete games, saves,
      independent hardware parity and audio fidelity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5394
runtime:
  executable_path: Otello
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 116f491d310a10aacb1ca6eb91b88442f7c7e6f304cbe2e93ef201b5918265d5
    size_bytes: 60244
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/otello-20.hqx
    - https://github.com/benletchford/systemless/issues/5394
    license: Original noncommercial unchanged-distribution notice
    rights_holder: Ron Hayter
    permission: >-
      The original About Otello document permits distribution on networks in
      unmodified form together with that document. Distribution for profit requires written
      permission. Preserve the complete unchanged archive and its notices, separately from
      emulator software.
    notes: >-
      Historical Info-Mac submission includes the original application and About
      Otello document. Independent extraction identified three CODE resources and no cfrg
      in the resource-only application. A later SIT package has different resource-fork
      bytes and is not this archive.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 3bcfe492621e56a859eb996a8924a8dbeb8257e3ec83933c7ad29bcf499652a2
    size_bytes: 2001
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5394
    permission: >-
      Original gameplay capture from the permitted unchanged archive, cropped to the
      game board and score display.
    notes: >-
      Native legal move and computer reply; excludes the menu bar, window frame,
      desktop, emulator framing and host interface.
references:
- https://github.com/benletchford/systemless/issues/5394
- https://www.ffothello.org/informatique/telecharger-des-programmes/
---

## Flip the discs

![Otello board after a move and computer reply](https://assets.systemless.org/catalogue/media/sha256/3b/3bcfe492621e56a859eb996a8924a8dbeb8257e3ec83933c7ad29bcf499652a2.png)

Place a disc to surround an opposing line and turn those discs to your colour.
Play against the Macintosh or another player; the side with more discs at the
end wins.
