---
id: alone-in-the-dark
kind: game
title: Alone in the Dark
summary: Explore Derceto in the original Macintosh promotional demo.
developer: Infogrames
publisher: MacPlay
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: boots
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.51.0 + local runtime"
    architecture: 68k
    environment: >-
      Deterministic 800-by-600, 256-colour run of the unchanged Macintosh demo.
      Selected the 640-by-400 game window, used Escape to skip the opening credits
      and travel sequence, and entered the mansion interior. Right turned the
      character; Up moved her across the room. Browser launch remains disabled
      pending manual verification of the promoted archive.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2663
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Alone%20Demo.sit
    expected_sha256: 095b238bcc793411f14278f4db900f2a245e397a5a9ba7aca66f3544625c8e6e
    expected_size: 4985529
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/alone-in-the-dark
    - https://www.apajalista.net/id?id=10098
    rights_holder: Infogrames, MacPlay/Interplay, and successors
    permission: >-
      This is the unchanged, deliberately limited Macintosh promotional demo,
      not a copy of the commercial game. The archive contains an application
      named Alone In The Dark Demo and a reduced Alone Data folder. A 1996
      Macintosh file index independently records an Alone in the Dark Mac game
      demo. No express redistribution licence was found for this package.
    notes: >-
      Original 4,985,529-byte StuffIt 5 archive, SHA-256
      095b238bcc793411f14278f4db900f2a245e397a5a9ba7aca66f3544625c8e6e.
      The contained application runs as a 68K Mac program. The contemporary
      requirements quoted by Classic Mac Demos recommend a 68040 or PowerPC Mac.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/alone-in-the-dark/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2663
    permission: >-
      Fresh gameplay capture from the unchanged Macintosh demo. The underlying
      artwork remains the property of its rights holders.
    notes: >-
      Cropped the 640-by-400 game content at (80,110) from an 800-by-600
      Systemless framebuffer after turning and moving the character inside
      Derceto. The crop excludes the Mac menu bar, title bar and desktop
      without changing game pixels.
references:
- https://classicmacdemos.com/alone-in-the-dark
- https://www.apajalista.net/id?id=10098
---

## Inside Derceto

![The character inside Derceto in the Alone in the Dark Macintosh demo](incoming/alone-in-the-dark/gameplay.png)

Enter the mansion and explore its rooms in this original Macintosh demo. The
archive contains the promotional edition and its reduced data folder, not the
retail CD or a DOS release.

Choose a screen size at startup. Escape skips the long opening sequences. In
the first room, Left and Right turn the character, and Up moves forward.
Browser launch remains disabled until the promoted archive is tested on the
site.
