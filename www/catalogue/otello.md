---
id: otello
kind: game
title: Otello 2.0
summary: >-
  Play a classic disc-flipping strategy game against the Macintosh or another player.
developer: Ron Hayter
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: Combined 2353ccaa and PRs 5388, 5389, 5391
    architecture: 68k
    environment: >-
      The exact original Info-Mac archive ran natively at depth 8. A legal white
      placement flipped a black disc, and the Macintosh replied; inspected
      captures show three discs for each side. Five measured before-and-after
      pixel assertions passed at frontend tick 422 and guest tick 1022 with
      zero exhausted frames. Full games, settings, saves and audio remain
      unverified. Hosted browser qualification is pending.
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
    type: url
    url: https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/otello-20.hqx
    expected_sha256: 116f491d310a10aacb1ca6eb91b88442f7c7e6f304cbe2e93ef201b5918265d5
    expected_size: 60244
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
      unmodified form together with that document. Distribution for profit
      requires written permission. Preserve the complete unchanged archive and
      its notices, separately from emulator software.
    notes: >-
      Historical Info-Mac submission includes the original application and
      About Otello document. Independent extraction identified three CODE
      resources and no cfrg in the resource-only application. A later SIT
      package has different resource-fork bytes and is not this archive.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/otello/board.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5394
    permission: >-
      Original gameplay capture from the permitted unchanged archive, cropped
      to the game board and score display.
    notes: >-
      Native legal move and computer reply; excludes the menu bar, window frame,
      desktop, emulator framing and host interface.
references:
- https://github.com/benletchford/systemless/issues/5394
- https://www.ffothello.org/informatique/telecharger-des-programmes/
---

## Flip the discs

![Otello board after a move and computer reply](incoming/otello/board.png)

Place a disc to surround an opposing line and turn those discs to your colour.
Play against the Macintosh or another player; the side with more discs at the
end wins.
