---
id: bad-mojo
kind: game
title: Bad Mojo
summary: >-
  Explore the underside of Eddie's bar as a cockroach in the original Macintosh
  interactive demo.
developer: Pulse Entertainment
publisher: Acclaim Entertainment
year: 1996
architectures:
- 68k
- ppc
default_architecture: 68k
category: Puzzle
compatibility:
  status: boots
  verified:
  - date: "2026-09-25"
    tester: Catalogue maintainer
    systemless_version: "0.60.0"
    architecture: 68k
    environment: >-
      Deterministic 800-by-600, 256-colour run of the unchanged Macintosh promotional
      archive. The introduction matches BasiliskII's interactive-demo menu; selecting
      Play Demo enters the first illustrated roach scene. Movement and browser launch
      have not yet been verified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2814
  - date: "2026-09-25"
    tester: Catalogue maintainer
    systemless_version: 0.60.0 + local release-mode site build
    architecture: 68k
    environment: >-
      Chromium loaded the exact original demo archive through a local response
      intercept and started the Mac runtime without console errors. A 30-second pacing sample
      averaged 56 host FPS but only 25.6 guest ticks per second, below the 50-tick
      browser-launch gate. Direct launch remains disabled.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2814
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: bedfb8b0638f6368843c56f23886afbd5bcfd13e12848a72a969fd13366ffbae
    size_bytes: 3114242
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/bad-mojo
    - https://static.classicmacdemos.com/demos/bad-mojo/README.txt
    rights_holder: Pulse Entertainment, Acclaim Entertainment, and successors
    permission: >-
      This is the unchanged, deliberately limited Macintosh promotional demo, not the
      commercial CD. Its included Read Me describes playing a small portion of the
      game, and Classic Macintosh Game Demos records distribution on six contemporary
      cover discs. The package has no express redistribution licence; this evidence
      supports preserving the original promotional copy only, not any retail edition or a
      modified package.
    notes: >-
      Original 3,114,242-byte StuffIt 5 archive, SHA-256
      bedfb8b0638f6368843c56f23886afbd5bcfd13e12848a72a969fd13366ffbae. The bundled Read Me requires a 68040
      Macintosh or Power Macintosh, and the application has both 68K CODE resources and a
      PowerPC fragment. Its MOJODEMO.EXE launch instruction appears copied from another
      platform; the archive itself contains a native Macintosh application.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8356cd588a6ea4b9300d85e7b390e1ea92fc4679870a034ab52df3c010fd648e
    size_bytes: 428431
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2814
    permission: >-
      Fresh game-content capture from the unchanged Macintosh promotional demo. The
      underlying artwork remains the property of its rights holders.
    notes: >-
      Cropped the 640-by-400 game surface at (80,90) from a Systemless 800-by-600
      framebuffer after selecting Play Demo. The crop removes only the surrounding black
      emulator display; game pixels are unchanged. PNG SHA-256
      8356cd588a6ea4b9300d85e7b390e1ea92fc4679870a034ab52df3c010fd648e; 428,431 bytes.
references:
- https://classicmacdemos.com/bad-mojo
- https://github.com/benletchford/systemless/issues/2814
---

## Under Eddie's bar

![The first roach scene in the Bad Mojo Macintosh interactive demo](https://assets.systemless.org/catalogue/media/sha256/83/8356cd588a6ea4b9300d85e7b390e1ea92fc4679870a034ab52df3c010fd648e.png)

Explore a small part of *Bad Mojo* from a cockroach's point of view. The original
Macintosh demo introduces the setting, then offers **Play Demo** alongside its
plot and trailer options. It is a limited promotional release, not the retail CD.

The 68K run reaches the first scene in Systemless. Browser performance is still
below the launch threshold, so direct launch remains disabled for now.
