---
id: damage-incorporated
kind: game
title: Damage Incorporated
summary: >-
  Lead a fire team through close-quarters missions where every order competes
  with the next burst of gunfire.
developer: Paranoid Productions
publisher: MacSoft
year: 1997
architectures:
- 68k
- ppc
default_architecture: 68k
category: FPS
compatibility:
  status: playable
  verified:
  - date: 2026-09-15
    tester: Catalogue maintainer
    systemless_version: 0.41.2
    architecture: 68k
    environment: Deterministic headless framebuffer run from the original BinHex archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/1981
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: f899ad2b6cce7489975e9499e5a3faa1cbb74f786cf862b733b266a06013c4a3
    size_bytes: 18006011
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=4329
    - https://ftp.lip6.fr/pub/mac/info-mac/_Game/arc/damage-inc-demo.hqx
    - https://www.paranoidproductions.com/damage/
    license: Damage Incorporated demonstration redistribution grant
    rights_holder: MacSoft and WizardWorks Group, Inc.
    permission: >-
      The Info-Mac submission and bundled Read This document permit the demo to be
      distributed freely by any means and to anyone, provided it remains unaltered and
      the Read This file stays with it.
    notes: >-
      This unchanged 18,006,011-byte Info-Mac BinHex submission was retrieved
      independently on 2026-09-15. It has SHA-256
      f899ad2b6cce7489975e9499e5a3faa1cbb74f786cf862b733b266a06013c4a3 and retains its distribution notice, original StuffIt
      archive, application, Read This document, maps, shapes, images, sounds and voices.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 9d28a16580fcc3ed236dc280fdd8cb2a9ba57a30f2c3cb5b512282c48dab5114
    size_bytes: 105020
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/142
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holders.
    notes: >-
      Fresh gameplay capture made from the unchanged archive on 2026-09-15. The
      800x600 framebuffer was cropped to the 640x448 game surface, removing only unrelated
      black margins; no game pixels were altered.
references:
- https://info-mac.org/viewtopic.php?t=4329
- https://www.paranoidproductions.com/games.html
- https://www.paranoidproductions.com/damage/
---

## Operation: White Night

![Damage Incorporated gameplay](https://assets.systemless.org/catalogue/media/sha256/9d/9d28a16580fcc3ed236dc280fdd8cb2a9ba57a30f2c3cb5b512282c48dab5114.png)

The rifle points down a narrow approach, but the lower half of the screen tells
the larger story. Duke and Carnage are waiting on orders. A number key can call
the whole fire team, send them to a navigation point, change formation or tell
them to hold while you scout ahead. Damage Incorporated borrows the immediacy of
a Marathon firefight, then makes responsibility for several other people part of
the pressure.

Its first mission begins under a hard blue night sky with a fenced compound in
the distance. The route looks simple until movement, ammunition and squad
position all demand attention at once. The command console never lets the game
become a solitary corridor shoot; even a quiet stretch is a chance to bring the
team back together before the next door.

## Four demo missions

MacSoft's demonstration contains three single-player operations and one network
map. Its Info-Mac submission is unusually plain about sharing: distribute it by
any means, to anyone, so long as the package stays unaltered and its Read This
document remains inside. The catalogue therefore keeps the complete BinHex file
exactly as submitted, including the nested StuffIt archive that Systemless opens
at launch.
