---
id: theme-park-demo
kind: game
title: Theme Park Demo
summary: >-
  Build rides, shops and paths, then keep the crowds happy and the money flowing
  in Bullfrog's theme park management game.
developer: Bullfrog Productions
publisher: Electronic Arts
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged demo archive ran natively on the 68K slice. Pressing 1 started
      the demo version in the park editor. Selecting the path tool and clicking near the
      entrance advanced the tutorial and laid path. A matched idle control was
      identical until the input and then differed by 4,822 pixels. An exact replay matched both
      checkpoints, with zero exhausted frames. Rides, shops, the park economy, and
      audio remain unverified. Three browser samples in the park passed at about 60 host
      fps and 60 guest ticks/s at default pacing.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5506
runtime:
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: fdce4467cee061c01913854f111304c2f07d2b0648609de6bb3f6374fc8e0c3b
    size_bytes: 756462
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/theme-park
    - https://download.classicmacdemos.com/MacThemePark%20Demo.sit
    - https://github.com/benletchford/systemless/issues/5506
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Electronic Arts
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: 68K demo of Bullfrog's theme park management game.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c75b4ccad000be6912d6c51be0ae4d2eb990dc3ef69374e7e823ccd879afd3a3
    size_bytes: 12030
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5506
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native 68K park editor with the path tutorial, cropped to the game window.
references:
- https://classicmacdemos.com/theme-park
- https://github.com/benletchford/systemless/issues/5506
---

## Open the gates

![Theme Park Demo](https://assets.systemless.org/catalogue/media/sha256/c7/c75b4ccad000be6912d6c51be0ae4d2eb990dc3ef69374e7e823ccd879afd3a3.png)

Start with an empty field and a budget. Lay paths, build rides and stalls, hire staff and watch the visitors decide whether your park is worth the ticket price.

Press 1 at the title menu to start. Pick tools from the icon bar and click on the park to build.
