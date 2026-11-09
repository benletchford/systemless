---
id: pararena-demo
kind: game
title: Pararena Demo
summary: Skate around a curved arena in John Calhoun's Macintosh arcade sports game.
developer: John Calhoun
publisher: Casady & Greene
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: unknown
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: c670f53b native tree
    architecture: 68k
    environment: >-
      Complete unchanged 2.01 demo, original 68K executable, default CPU, partition,
      sound and initial display. Select 16 Colors in the original prompt, New Game and
      the available Simple George opponent. Ordinary mouse movement changes the player
      position against a matched omitted-input control. Both repeats pass nine
      assertions at 1026 frontend ticks and 1626 guest ticks with zero budget exhaustion.
      Own-run PNG references test consistency, not independent hardware accuracy. Scoring,
      match completion, audio fidelity, persistence, browser input and public pacing
      remain unqualified. Launch stays disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5917
runtime:
  executable_path: Pararena Demo 2.01 ƒ/Pararena Demo 2.01
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 18b7d3c2c2901677c64cef1b8402d2ff6d2ae48d6330e49bfe4509d30e6bdf4e
    size_bytes: 404577
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://oldapplestuff.com/download/Macintosh/Macintosh_Garden/games/PararenaDemo_v2.01.sit
    - https://www.macintoshrepository.org/3010-pararena-2-0
    - https://www.macintoshrepository.org/19424-the-macintosh-demo-games-cd
    - https://github.com/benletchford/systemless/issues/5917
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Casady & Greene / John Calhoun
    permission: >-
      Accepted under the project's promotional-demo policy with explicit maintainer
      authorization on 2026-11-10. Period publisher/cover-disc distribution is
      corroborated, the complete original package is preserved, and no explicit redistribution
      restriction was found in the inspected applicable notices. This records
      project-policy acceptance, not an affirmative publisher licence grant. Copyright notices
      remain intact.
    notes: >-
      Original 404577-byte SIT, SHA-256
      18b7d3c2c2901677c64cef1b8402d2ff6d2ae48d6330e49bfe4509d30e6bdf4e; SHA1 d4109e94e7eb64463b17d2fd94b2d83afefbe06a matches the
      archive listing. All 3 files, 6 forks and 3 Finder records match independent
      extraction. All 260 application resources match the preserved 1992 demo compilation in
      payload, name and attributes; raw resource forks and the disc image are not
      claimed byte-identical to original publisher media. The bundled Readme describes
      external ParaSounds as optional; it was absent from this received archive. No files
      were stripped or added. The modern source MIT licence is not substituted for
      historical binary or DOCMaker terms.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: a3b9b71c2005363d571ec41f16c9b38fb16e442be80320a57fd7915cea74b718
    size_bytes: 39489
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5917
    rights_holder: Casady & Greene / John Calhoun
    permission: >-
      Fresh Systemless capture from the project-policy-accepted promotional demo;
      underlying artwork retains its original copyright.
    notes: >-
      Native game window, cropped only to remove the top 20-pixel Classic Mac menu
      bar; 800x580, no resampling or palette changes. SHA-256
      a3b9b71c2005363d571ec41f16c9b38fb16e442be80320a57fd7915cea74b718. Native evidence only, not browser
      qualification.
references:
- https://github.com/benletchford/systemless/issues/5917
---

![Pararena native arena capture](https://assets.systemless.org/catalogue/media/sha256/a3/a3b9b71c2005363d571ec41f16c9b38fb16e442be80320a57fd7915cea74b718.png)

Skate around a bowl-shaped arena and try to send the ball past your opponent.
This original promotional demo offers a match against Simple George.

Native mouse steering is verified. A private browser build reaches the arena, but
input and omitted-input cases fail the unchanged guest pacing gate at 16.28 and
13.22 ticks/s (required 50). Host pacing and audio queue checks pass in those
initialization windows. The separate sustained gameplay window was not reached.
This is a private-build result, not public-release qualification. See
[browser pacing hold #5919](https://github.com/benletchford/systemless/issues/5919).
Launch stays disabled.
