---
id: the-journeyman-project
kind: game
title: The Journeyman Project
summary: Watch Presto Studios' original 1992 Macintosh sneak preview.
developer: Presto Studios, Inc.
publisher: Presto Studios, Inc.
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: boots
  verified:
  - date: "2026-09-25"
    tester: Catalogue maintainer
    systemless_version: 0.59.0
    architecture: 68k
    environment: >-
      Deterministic 800-by-600, 256-colour run of the MacBinary-preserved
      Apple demo-CD package. The original application loaded its support files
      and rendered the aircraft trailer scene by frontend tick 4200.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2678
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: incoming
    path: catalogue/incoming/the-journeyman-project/journeyman-sneak-preview-1992-apple-cd.zip
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
    rights_holder: Presto Studios, Inc. and successors
    permission: >-
      The publisher's Read Me explicitly permits copying this demo freely,
      provided the complete folder is copied with the application, Support Files,
      and Read Me. This package preserves all of those original Macintosh files.
    notes: >-
      The 6,885,731-byte ZIP, SHA-256
      fb37f47cd34355f10bd693334759b1933349086ea92aea960956dec80d02a285,
      is a new transport wrapper around MacBinary-preserved files extracted from
      Apple's 1992 Macintosh Demo Games CD. The files themselves are unchanged.
      This is the original 68K digital trailer, not the retail CD or the later
      PowerPC-only Pegasus Prime demo.
- id: aircraft-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/the-journeyman-project/aircraft.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2678
    permission: >-
      Fresh Systemless capture from the unchanged Macintosh preview. The
      underlying artwork remains the property of its rights holders.
    notes: >-
      Direct 640-by-480 game-content capture, excluding the surrounding desktop
      and without altering the preview's pixels.
references:
- https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
- https://github.com/benletchford/systemless/issues/2678
---

## The original sneak preview

![An aircraft above the clouds in The Journeyman Project sneak preview](incoming/the-journeyman-project/aircraft.png)

Presto Studios called this 1992 release a digital trailer for *The Journeyman
Project*. It presents scenes from the time-travel adventure; it is not the
complete commercial game or a playable level demo.

The publisher's Read Me permits copying the complete preview folder. This
archive preserves the application, Read Me, and all 27 support files from
Apple's Macintosh Demo Games CD. A colour 68K Macintosh with at least 5 MB of
RAM was the stated requirement; the CD-ROM drive mentioned in the Read Me was
for the full game, not this preview.

Browser launch remains disabled until the packaged preview is checked in a
release-mode browser. The later *Pegasus Prime* demo is a separate PowerPC-only
release and is not substituted here.
