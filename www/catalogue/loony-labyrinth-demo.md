---
id: loony-labyrinth-demo
kind: game
title: Loony Labyrinth Demo
summary: Play LittleWing's ornate pinball table in its original 90-second Macintosh demonstration.
developer: LittleWing Co. Ltd.
publisher: StarPlay Productions
year: 1994
architectures: [68k]
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: boots
  verified:
  - date: 2026-09-28
    tester: Catalogue maintainer
    systemless_version: 73f7a32ef9adc30eb2e49872852e9d33a2fa387a
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged BinHex demo. The notice and
      instructions opened, followed by the full-color pinball table. Ball play,
      native-Mac comparison, and browser launch have not yet been verified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/3092
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://download.classicmacdemos.com/Loony%20Labyrinth.hqx
    expected_sha256: 2eb0c8aa52edd453ad5080c6c37fa1c2fd6adca07248ee72daab76ab395c00cf
    expected_size: 3300692
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/loony-labyrinth
    - https://download.classicmacdemos.com/Loony%20Labyrinth.hqx
    rights_holder: LittleWing Co. Ltd. and StarPlay Productions
    permission: >-
      The application's original notice calls this a demo version, marks it not
      for sale, disables some functions, limits playing time to 90 seconds, and
      provides ordering details for the full game. The unchanged promotional
      download is distributed by Classic Macintosh Game Demos; no full-game
      files are used.
    notes: >-
      Original 3,300,692-byte BinHex archive, SHA-256
      2eb0c8aa52edd453ad5080c6c37fa1c2fd6adca07248ee72daab76ab395c00cf.
      It contains a 68K Macintosh demo application with a resource fork.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/loony-labyrinth-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3092
    permission: >-
      Fresh pinball-table capture made for this catalogue entry from the
      unchanged promotional demo. Underlying game artwork remains its owners'
      property.
    notes: >-
      Exact 620-by-536 content crop at (88,32) of a deterministic 800-by-600
      Systemless framebuffer. Surrounding desktop was excluded without changing
      game pixels. PNG SHA-256
      4330e7975a35b7a7f078f377df10a4df984e16790db1e5c51536f5352b867289,
      385,409 bytes.
references:
- https://classicmacdemos.com/loony-labyrinth
---

## A pinball maze

![Loony Labyrinth demo pinball table](incoming/loony-labyrinth-demo/gameplay.png)

LittleWing's colorful table winds through looping ramps, bumpers, and a
labyrinth motif. The original notice identifies this as a promotional demo:
some functions are disabled and play is limited to 90 seconds. It also includes
the contemporary instructions and ordering information.

Systemless reaches the table from the unchanged 68K demo archive. Browser
launch remains disabled until a manual check is complete.
