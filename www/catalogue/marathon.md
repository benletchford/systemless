---
id: marathon
kind: game
title: Marathon
summary: >-
  Explore a besieged colony ship and fight alien forces in Bungie's first-person
  science-fiction adventure.
developer: Bungie
publisher: Bungie
year: 1994
architectures:
- 68k
- ppc
default_architecture: 68k
category: FPS
compatibility:
  status: unknown
controls:
  arrows_as_numpad: true
  mobile:
    buttons:
    - key: Space
      label: A
    - key: Tab
      label: B
    - key: Enter
      label: C
    enabled: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: eb6e3d4b791481367ed605062bf4a39d52c24bd23ba9824602b29c63f7c6bf10
    size_bytes: 4996142
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://trilogyrelease.bungie.org/
    - https://trilogyrelease.bungie.org/faq.html
    - https://github.com/benletchford/systemless/issues/5284
    rights_holder: Bungie
    permission: >-
      The Trilogy Release FAQ states that Bungie permits free distribution of the
      released files while retaining copyright and prohibiting sale. This is the unchanged
      Marathon archive linked from that release page.
    notes: >-
      Original Trilogy Release StuffIt archive, 4,996,142 bytes, SHA-256
      eb6e3d4b791481367ed605062bf4a39d52c24bd23ba9824602b29c63f7c6bf10. This replaces the
      previously hosted archive, whose active Music file was patched and whose additional
      components have not been separately cleared.
- id: screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: f585ef4e3e3df1203204336cc9e081e8a316874029b165b71e223e102225d5da
    size_bytes: 107139
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5284
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request.
    notes: >-
      Captured from the original Trilogy Release archive during verified native 68K
      movement, cropped to the game content surface.
references:
- https://trilogyrelease.bungie.org/
- https://github.com/benletchford/systemless/issues/5284
---

## Source and version

This entry tracks the original [Trilogy Release archive](https://assets.systemless.org/catalogue/objects/sha256/eb/eb6e3d4b791481367ed605062bf4a39d52c24bd23ba9824602b29c63f7c6bf10.sit). Bounded native tests reached the first level and verified forward movement on both 68K and PowerPC; browser launch remains pending validation.

## Aboard the Marathon

![The player approaches a terminal aboard the Marathon](https://assets.systemless.org/catalogue/media/sha256/f5/f585ef4e3e3df1203204336cc9e081e8a316874029b165b71e223e102225d5da.png)

The UESC Marathon hangs above Tau Ceti with its crew under attack and its three shipboard AIs pulling you in different directions. The corridors are spare, lonely places: exploration, switches, lifts and terminal messages matter as much as a quick trigger. Reading those terminals is the game; Durandal's voice slowly turns a rescue mission into something more unsettling.
