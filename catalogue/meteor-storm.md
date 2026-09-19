---
id: meteor-storm
kind: game
title: Meteor Storm
summary: >-
  Dodge a sky full of tumbling rock, collect power-ups and fight for room on a
  single crowded screen—alone or beside a second pilot.
developer: Zachary Black
publisher: Z Sculpt Entertainment
year: 1996
architectures:
- 68k
- ppc
default_architecture: ppc
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-16
    tester: Catalogue maintainer
    systemless_version: 0.41.5
    architecture: ppc
    environment: Deterministic headless run from the untouched author-submitted archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/1993
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 60e5a8c365fd18f99d72800a6b0858a3947b6d6b8d8ef54f30a2bfeeabc9716a
    size_bytes: 3872023
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?p=12519
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/_Arcade/meteor-storm-14.hqx
    license: Z Sculpt Entertainment shareware
    rights_holder: Z Sculpt Entertainment
    permission: >-
      The bundled MS Read Me 1.4 permits shareware distribution when the software
      package remains complete and unmodified.
    notes: >-
      Untouched Meteor Storm 1.4 BinHex archive retrieved from the Info-Mac mirror on
      2026-09-16. The 3,872,023-byte file has SHA-256
      60e5a8c365fd18f99d72800a6b0858a3947b6d6b8d8ef54f30a2bfeeabc9716a. Z Sculpt remains the current rights holder and
      released official modern editions in 2021; those commercial and mobile builds are
      neither hosted nor used here.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8825abb3522a58520d39a92c11da52d76e940a80840d0b8320fab35b3b7bc321
    size_bytes: 19558
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/157
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged author-submitted archive on
      2026-09-16, cropped exactly to the game's 640-by-480 content. It excludes the Classic
      Mac menu bar, browser, website, host desktop and emulator chrome.
references:
- https://www.zsculpt.com/website/games/meteorstorm/
- https://store.steampowered.com/app/1690500/Meteor_Storm/
- https://apps.apple.com/us/app/meteor-storm-classic/id364910927
- https://info-mac.org/viewtopic.php?p=12519
---

## Two pilots, one storm

![Meteor Storm gameplay](https://assets.systemless.org/catalogue/media/sha256/88/8825abb3522a58520d39a92c11da52d76e940a80840d0b8320fab35b3b7bc321.png)

Meteor Storm puts both pilots in the same field and then steadily takes away
their breathing room. The ships drift between spinning meteors, carve openings
with rapid fire and scramble for power-ups while each new wave adds more rock,
enemy craft and the occasional mothership. Played alone it is a brisk score
chase; with a second pilot it becomes a noisy argument about who owns the last
safe patch of sky.

The shared screen is the point. There is no camera to escape into and no quiet
corner for long, just the wave counter, two shield readouts and a dark starfield
that becomes harder to read as debris spreads. Clearing a wave brings a short
pause before the next collision course begins.

## The complete 1.4 package

Zachary Black's October 2000 release is a fat Macintosh application containing
both 68K and PowerPC code. Version 1.4 brought revised music and mixing, with
InputSprocket support for PowerPC Macs, and was submitted to Info-Mac as one
complete BinHex shareware package. That exact package is preserved here:
Systemless opens its BinHex and StuffIt layers and finds the application without
altering the download.

Z Sculpt later returned to Meteor Storm with official mobile and commercial
desktop editions in 2021. Those newer releases establish the game's present-day
home, but they are not substitutes for this catalogue artifact. Only the
original 1.4 shareware archive—whose own readme permits complete, unmodified
distribution—is hosted here.
