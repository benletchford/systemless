---
id: taskmaker
kind: game
title: TaskMaker
summary: >-
  Explore Storm Impact's classic Macintosh fantasy adventure in its original
  unregistered shareware release.
developer: David Cook and Tom Zehner
publisher: Storm Impact, Inc.
year: 1989
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.57.0 + PR #2639"
    architecture: 68k
    environment: >-
      Deterministic run of the unchanged creator-hosted 2.2.4 ZIP through the
      shareware Distribution notice, New/Open dialog, Create Character dialog, and opening
      tutorial board. Return from character creation reaches the board. An early
      deterministic snapshot of New/Open lacked its background art; later browser
      verification showed the artwork after the dialog settled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2555
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.57.0 + PR #2639"
    architecture: 68k
    environment: >-
      Release-mode local Chromium route using the promoted immutable ZIP. The
      Distribution notice, illustrated New/Open dialog, character creation,
      first-save dialog, and tutorial board all rendered. A click on an on-screen
      movement control changed the map pixels outside the cursor. The local
      preview disabled cross-origin enforcement because the asset host allows
      the production site origin, not localhost.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/2641
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: sha256
    sha256: 2fc4b81e56559cd30560d8138acf3df8ae7d34a4ffe2dc8f364e6d2df4bf6d72
    size_bytes: 1593622
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.robotroom.com/StormImpact.html
    - https://www.robotroom.com/StormImpact/TaskMaker_v224.zip
    license: Storm Impact TaskMaker shareware Distribution notice
    rights_holder: Storm Impact, Inc. / David Cook
    permission: >-
      The creator directly hosts this final 2.2.4 shareware ZIP. The game's
      Distribution notice invites free copies and says that copies must be complete and
      unaltered, including the sound and color files, and that copies or distribution must not
      be charged for. This is the exact unregistered creator-hosted package, with no
      registration code or edits.
    notes: >-
      The ZIP is 1,593,622 bytes, SHA-256
      2fc4b81e56559cd30560d8138acf3df8ae7d34a4ffe2dc8f364e6d2df4bf6d72. A fresh fetch from the creator's site matched these bytes.
      The original 1989 game was later expanded into this color shareware version.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: da8c937e58aeb93dbc4d47bb0f04bc098a97382763ca5b7d109202931c4df6ee
    size_bytes: 43278
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2640
    permission: >-
      Fresh Systemless capture of the unregistered shareware tutorial board.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Cropped the 800-by-600 deterministic guest framebuffer to the 510-by-322 game
      content surface, excluding the Mac desktop, menu bar, and window chrome without
      altering game pixels. PNG SHA-256
      da8c937e58aeb93dbc4d47bb0f04bc098a97382763ca5b7d109202931c4df6ee; 43,278 bytes.
references:
- https://www.robotroom.com/StormImpact.html
- https://github.com/benletchford/systemless/issues/2640
- https://github.com/benletchford/systemless/issues/2555
---

## A task awaits

![TaskMaker's opening tutorial board](https://assets.systemless.org/catalogue/media/sha256/da/da8c937e58aeb93dbc4d47bb0f04bc098a97382763ca5b7d109202931c4df6ee.png)

Storm Impact's Macintosh role-playing adventure sends a new hero through
villages, dungeons, monsters and quests. This entry uses the complete,
unregistered 2.2.4 shareware package still offered by its creator, including
the original sounds and color artwork. It is not an unlocked or modified copy.

The game's Distribution notice encourages sharing complete, unchanged copies
without charging for them. Registration rights and the game's copyright remain
with its owner. In the browser, dismiss Distribution, choose New, create a
character, and save to reach the tutorial board. The illustrated New/Open
dialog appears after a short startup delay, and the on-screen movement controls
respond in the opening area.
