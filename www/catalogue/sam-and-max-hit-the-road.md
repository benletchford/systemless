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
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.50.0"
    architecture: 68k
    environment: >-
      Deterministic Systemless run from the exact official Macintosh demo archive
      through its LucasArts logo and animated opening sequence, cross-checked with the
      same archive under BasiliskII
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2432
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: "0.65.1"
    architecture: 68k
    environment: >-
      Deterministic headless replay of the exact listed official ZIP reached the
      office after the full opening. Clicking the telephone moved Sam toward it. An
      independently obtained StuffIt demo produced the same framebuffer and has byte-identical
      application data, resource, and game data forks. Puzzle completion, PowerPC
      execution, and browser behavior were not reverified in this run. The
      promoted archive and screenshot were fetched back and matched their
      recorded SHA-256 hashes.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3101
runtime:
  application_partition_size: 8388608
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: sha256
    sha256: c39aceb453093aea0c0564b36439636f13d46df480ccf1ec85304638e00dd6b5
    size_bytes: 9651922
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.scummvm.org/demos/
    - https://downloads.scummvm.org/frs/demos/scumm/samnmax-mac-demo-en.zip
    license: LucasArts Sam & Max promotional demo distribution
    rights_holder: Lucasfilm Games / LucasArts and their successors
    permission: "LucasArts deliberately released this self-contained package as the Macintosh demonstration of Sam & Max: Hit the Road. Its included read-me thanks the player for trying the demo, describes its first-puzzle challenge and supplies ordering details for the full game. ScummVM's official demo library continues to distribute the unchanged package. This entry preserves only the demo files and does not include or claim permission for the retail game."
    notes: >-
      Unchanged 9,651,922-byte ZIP with SHA-256
      c39aceb453093aea0c0564b36439636f13d46df480ccf1ec85304638e00dd6b5. The package retains the MacBinary application,
      read-me and icon alongside the original Sam & Max Demo Data file.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 2554eec2fee03335304422a3a5de52754ea379940f65cdcc19f2c7c4a446d26d
    size_bytes: 60605
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3101
    permission: >-
      Fresh office-scene capture made for this catalogue entry from the unchanged
      promotional demo. Underlying Sam & Max artwork remains its owners' property.
    notes: >-
      Exact 640-by-400 office-scene crop at (80,100) from a deterministic 800-by-600
      Systemless framebuffer on 2026-09-28. Surrounding desktop was excluded without
      changing game pixels. PNG SHA-256
      2554eec2fee03335304422a3a5de52754ea379940f65cdcc19f2c7c4a446d26d, 60,605 bytes.
references:
- https://www.scummvm.org/demos/
---

## Freelance police, reporting for duty

![Sam and Max in their office beside the telephone](https://assets.systemless.org/catalogue/media/sha256/25/2554eec2fee03335304422a3a5de52754ea379940f65cdcc19f2c7c4a446d26d.png)

Sam is a six-foot canine detective. Max is a hyperkinetic rabbity thing. When
the commissioner sends them after a missing carnival attraction, the case turns
into a road trip through LucasArts' most gleefully unruly corner of America.
The SCUMM interface lets players investigate, improvise and inflict their own
peculiar brand of justice with verbs, dialogue and inventory objects.

This Macintosh demonstration opens with the game's animated road sequence and
then challenges the player with the first puzzle from the adventure. A later
deterministic replay reached the office and moved Sam toward the telephone in
response to a click. It is a
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
