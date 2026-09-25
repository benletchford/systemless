---
id: oids
kind: game
title: Oids
summary: Rescue the Oids in FTL Games' original interactive Macintosh demo.
developer: FTL Games
publisher: FTL Games
year: 1990
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: boots
  verified:
  - date: "2026-09-25"
    tester: Catalogue maintainer
    systemless_version: 0.60.0 with merged NewPixMap correction
    architecture: 68k
    environment: >-
      Deterministic headless run of the unchanged Apple demo-CD files in a
      MacBinary-preserved ZIP at 800 by 600 and 16 colours. The demo advanced for 3,600 frontend
      ticks to guest tick 4,200 and rendered its populated 640-by-480 playfield with
      ship, terrain, Oids, explosions, and HUD. This headless checkpoint did not test
      browser launch.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2509
  - date: "2026-09-25"
    tester: Catalogue maintainer
    systemless_version: 0.60.0 with merged NewPixMap correction and screen-depth preview
    architecture: 68k
    environment: >-
      Chromium loaded the exact demo archive through the preview route in the direct
      WebAssembly runtime at 16 colours. A 30-second sample painted game frames with no
      browser console errors. At the entry's 8 MHz pacing it averaged about 30 guest
      ticks per second on the test machine. The full two-to-three-minute built-in
      introduction and player handoff were not observed in this browser sample.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2509
runtime:
  runtime_pacing:
    cpu_mhz: 8
  screen_depth: 4
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: sha256
    sha256: 4ad7264fedb00777a58558269aad0cd6aaafe31ae011bdb2869074f6a9d2799d
    size_bytes: 180535
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
    rights_holder: FTL Games and successors
    permission: >-
      FTL supplied this promotional OIDS Demo, its OIDS.LIB support file, and Read Me
      for Apple's 1992 Macintosh Demo Games CD. The Read Me identifies a self-running
      demonstration that later lets the player try the game, while separately
      advertising the retail product. No express broader redistribution licence was found; this
      preservation basis covers only the original promotional files, not the commercial
      release.
    notes: >-
      This 180,535-byte ZIP (SHA-256
      4ad7264fedb00777a58558269aad0cd6aaafe31ae011bdb2869074f6a9d2799d) is a transport wrapper around unchanged MacBinary-preserved
      OIDS Demo and OIDS.LIB files plus the accompanying Read Me from the Apple CD. It is
      not an original publisher archive.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: ffc59c3ae55b63ec5cfd8205c8c210df9e9ee9bfc87301848c3926f1309b8edc
    size_bytes: 20597
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2509
    permission: >-
      Fresh Systemless capture from the unchanged promotional demonstration. The
      underlying artwork remains the property of its rights holders.
    notes: >-
      Direct 640-by-480 game-content crop of the 16-colour framebuffer at guest tick
      4,200, excluding surrounding black emulator framing without changing game pixels.
      PNG SHA-256 ffc59c3ae55b63ec5cfd8205c8c210df9e9ee9bfc87301848c3926f1309b8edc,
      20,597 bytes.
references:
- https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
- https://github.com/benletchford/systemless/issues/2509
---

## Rescue the Oids

![Oids demo ship flying over cavern terrain](https://assets.systemless.org/catalogue/media/sha256/ff/ffc59c3ae55b63ec5cfd8205c8c210df9e9ee9bfc87301848c3926f1309b8edc.png)

FTL Games' original Macintosh demo starts with a self-running introduction,
then offers a chance to fly a V-wing ship and rescue the Oids. Its Read Me says
the demonstration runs for two to three minutes before handing control to the
player. This is the promotional demo, not the complete retail game or its
galaxy editor.

The archive contains the 68K demo application, its OIDS.LIB support file, and
the Read Me from Apple's 1992 Macintosh Demo Games CD. It needs a 16-colour
display mode. The introduction runs before the game offers player control;
browser playback may take longer than the two to three minutes stated in the
original Read Me.

[Download the original demo files](https://assets.systemless.org/catalogue/objects/sha256/4a/4ad7264fedb00777a58558269aad0cd6aaafe31ae011bdb2869074f6a9d2799d.zip)
