---
id: sam-and-max-hit-the-road
kind: game
title: "Sam & Max: Hit the Road"
summary: >-
  Join the Freelance Police in LucasArts' original Macintosh demonstration of
  their anarchic comic adventure.
developer: LucasArts Entertainment Company LLC
publisher: LucasArts Entertainment Company LLC
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: boots
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.50.0"
    architecture: 68k
    environment: >-
      Deterministic Systemless run from the exact official Macintosh demo archive
      through its LucasArts logo and animated opening sequence, cross-checked with
      the same archive under BasiliskII
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2432
runtime:
  show_menu_bar: true
  application_partition_size: 8388608
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: incoming
    path: catalogue/incoming/sam-and-max-hit-the-road/samnmax-mac-demo-en.zip
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.scummvm.org/demos/
    - https://downloads.scummvm.org/frs/demos/scumm/samnmax-mac-demo-en.zip
    license: LucasArts Sam & Max promotional demo distribution
    rights_holder: Lucasfilm Games / LucasArts and their successors
    permission: >-
      LucasArts deliberately released this self-contained package as the Macintosh
      demonstration of Sam & Max: Hit the Road. Its included read-me thanks the player
      for trying the demo, describes its first-puzzle challenge and supplies ordering
      details for the full game. ScummVM's official demo library continues to distribute
      the unchanged package. This entry preserves only the demo files and does not
      include or claim permission for the retail game.
    notes: >-
      Unchanged 9,651,922-byte ZIP with SHA-256
      c39aceb453093aea0c0564b36439636f13d46df480ccf1ec85304638e00dd6b5. The
      package retains the MacBinary application, read-me and icon alongside the original
      Sam & Max Demo Data file.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/sam-and-max-hit-the-road/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2432
    permission: >-
      Original in-game screenshot captured for this catalogue at the maintainer's
      request. Underlying Sam & Max artwork remains the property of its rights holders.
    notes: >-
      Fresh deterministic Systemless 0.50.0 capture made from the staged demo archive
      on 2026-09-23 during its animated opening. The 800x600 guest framebuffer was
      cropped to the 640x400 game surface, excluding the Classic Mac menu bar and
      surrounding desktop. PNG SHA-256
      42ea66f59bc5e50410e04c12041a73ad54592231c6ad199a71481010df9e64f2,
      26,306 bytes.
references:
- https://www.scummvm.org/demos/
---

## Freelance police, reporting for duty

![Sam and Max on the road](incoming/sam-and-max-hit-the-road/gameplay.png)

Sam is a six-foot canine detective. Max is a hyperkinetic rabbity thing. When
the commissioner sends them after a missing carnival attraction, the case turns
into a road trip through LucasArts' most gleefully unruly corner of America.
The SCUMM interface lets players investigate, improvise and inflict their own
peculiar brand of justice with verbs, dialogue and inventory objects.

This Macintosh demonstration opens with the game's animated road sequence and
then challenges the player with the first puzzle from the adventure. It is a
compact showcase for the hand-drawn animation, comic timing and irreverent
writing that made Sam and Max enduring adventure-game characters.

## LucasArts' Macintosh demonstration

This is the original promotional demo, not the commercial game. The included
LucasArts read-me explicitly calls it the Sam & Max demo, invites players to
solve its opening challenge and gives ordering details for the complete game.
ScummVM's official demo library identifies the download specifically as the
Macintosh demo and continues to provide the original ZIP.

The catalogue stages that archive byte-for-byte. Systemless decodes and launches
the original 68K MacBinary application directly, preserving its accompanying
data file and read-me. A deterministic run verifies the animated opening using
the exact staged archive, with BasiliskII providing an independent cross-check.
