---
id: loony-labyrinth-demo
kind: game
title: Loony Labyrinth Demo
summary: >-
  Play LittleWing's ornate pinball table in its original 90-second Macintosh
  demonstration.
developer: LittleWing Co. Ltd.
publisher: StarPlay Productions
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
runtime:
  runtime_pacing:
    cpu_mhz: 10
compatibility:
  status: playable
  verified:
  - date: "2026-09-30"
    tester: Catalogue maintainer
    systemless_version: "952774ad + local 10 MHz catalogue preview"
    architecture: 68k
    environment: >-
      Release-mode Chrome browser preview of the unchanged BinHex demo. Operation >
      Insert Coin and New Game opened the full table after How to Play. Z and /
      raised their respective flippers and release returned them. The second
      ball remained in the launch lane during a Shift hold, then moved to the
      top of the table within 500 ms of release. Two active-table samples ran at
      60.1/60.2 host FPS and 59.5/58.2 guest ticks per second at 10 MHz, with
      maximum measured frames of 15.7/15.9 ms. The demo's 90-second limit applies.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3184
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: 73f7a32ef9adc30eb2e49872852e9d33a2fa387a
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged BinHex demo. The notice and
      instructions opened, followed by the full-color pinball table. Ball play, native-Mac
      comparison, and browser launch have not yet been verified. The promoted
      archive and screenshot were fetched back and matched their recorded
      SHA-256 hashes.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/3092
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 2eb0c8aa52edd453ad5080c6c37fa1c2fd6adca07248ee72daab76ab395c00cf
    size_bytes: 3300692
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/loony-labyrinth
    - https://download.classicmacdemos.com/Loony%20Labyrinth.hqx
    rights_holder: LittleWing Co. Ltd. and StarPlay Productions
    permission: >-
      The application's original notice calls this a demo version, marks it not for
      sale, disables some functions, limits playing time to 90 seconds, and provides
      ordering details for the full game. The unchanged promotional download is
      distributed by Classic Macintosh Game Demos; no full-game files are used.
    notes: >-
      Original 3,300,692-byte BinHex archive, SHA-256
      2eb0c8aa52edd453ad5080c6c37fa1c2fd6adca07248ee72daab76ab395c00cf. It contains a 68K Macintosh demo application
      with a resource fork.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 4330e7975a35b7a7f078f377df10a4df984e16790db1e5c51536f5352b867289
    size_bytes: 385409
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3092
    permission: >-
      Fresh pinball-table capture made for this catalogue entry from the unchanged
      promotional demo. Underlying game artwork remains its owners' property.
    notes: >-
      Exact 620-by-536 content crop at (88,32) of a deterministic 800-by-600
      Systemless framebuffer. Surrounding desktop was excluded without changing game pixels.
      PNG SHA-256 4330e7975a35b7a7f078f377df10a4df984e16790db1e5c51536f5352b867289,
      385,409 bytes.
references:
- https://classicmacdemos.com/loony-labyrinth
---

## A pinball maze

![Loony Labyrinth demo pinball table](https://assets.systemless.org/catalogue/media/sha256/43/4330e7975a35b7a7f078f377df10a4df984e16790db1e5c51536f5352b867289.png)

LittleWing's colorful table winds through looping ramps, bumpers, and a
labyrinth motif. The original notice identifies this as a promotional demo:
some functions are disabled and play is limited to 90 seconds. It also includes
the contemporary instructions and ordering information.

Systemless plays the unchanged 68K demo in the browser. To start, click through
the opening screens, choose **Operation > Insert Coin**, then **Operation > New
Game**, and dismiss **How to Play**. Press **Z** and **/** for the flippers; hold
and release **Shift** to launch a ball. The original demo ends play after 90
seconds.
