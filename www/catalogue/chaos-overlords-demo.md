---
id: chaos-overlords-demo
kind: game
title: Chaos Overlords Demo
summary: Recruit gangs and vie for control of the city in the original turn-based strategy demo.
developer: Stick Man Games
publisher: New World Computing, Inc.
year: 1996
architectures:
- 68k
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: 4d5e4c00 (e230afdc source-tree equivalent)
    architecture: ppc
    environment: >-
      Complete unchanged original SIT and default machine settings, genuine PPC
      executable. File > New Game and Begin reach the city. A normal drag from
      the five-cost recruitment portrait to the owned red sector queues a hire.
      Done and OK on the original idle-gang warning advance to 2050.02. A matched
      omitted-button control uses identical pointer moves, waits and turn input.
      Both finish 1892 frontend and 1871 guest ticks with zero exhausted frames.
      The starting captures are RGB-identical; queued and next-turn frames differ
      by 1246 and 3615 pixels. The active run marks the recruit HIRED, replaces its
      portrait next turn and leaves cash at 13; control retains the recruit and
      cash is 21. Both scripts have zero assertions; inspected captures and an
      external RGB comparison establish this bounded interaction. Full scenarios,
      combat, audio, multiplayer and browser compatibility remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6095
runtime:
  executable_path: Chaos Overlords Demo/Chaos Overlords DEMO
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/chaos-overlords/
    download_page: https://classicmacdemos.com/chaos-overlords
    expected_sha256: 802ae6493504db443f1be2c9fb97364dbd27345c50543cff9e243b17fb351687
    expected_size: 3936070
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/chaos-overlords
    - https://archive.org/details/IMG38July1996
    - https://github.com/benletchford/systemless/issues/6095
    license: Maintainer-accepted promotional demo
    rights_holder: New World Computing, Inc., Stick Man Games and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance,
      without claiming an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed notices. Preserve all
      218 files, both documentation parts, copyright notices and original demo limits:
      Kill 'Em All only, 52 turns, disabled saving and disabled background music.
    notes: >-
      All 218 files, 436 forks and 218 Finder type/creator/flag records match the
      July 1996 Inside Mac Games demo folder. All 177 resource payloads, names,
      IDs and attributes match. Two documentation filenames use equivalent classic
      Mac slash representations: period slash, packed U+F02F and independent unar
      metadata colon; unar exports those colons as underscores. No wrapper or raw
      resource-container byte identity is claimed. Full production packing matches
      independent unar extraction on all files, forks and Finder records. Notice
      review covered the 722-byte readme, both 28636- and 26173-byte documentation
      parts, and 60 standard UI/text/version resources, without compressed resources
      in that subset. Opaque game data, guest code and scripts were not decoded.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/chaos-overlords-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6095
    rights_holder: New World Computing, Inc., Stick Man Games and original contributors
    permission: Fresh content-only Systemless capture of the project-policy-accepted demo.
    notes: >-
      Unscaled 640x480 crop of actual city gameplay after the verified hire and
      first turn, showing week 2050.02, cash 13 and the replacement recruit.
      Excludes desktop, Classic Mac menu bar and outer framing. Original artwork
      retains its copyright.
references:
- https://classicmacdemos.com/chaos-overlords
- https://coverdiscs.com/disc/inside-mac-games-vol-4-issue-7/
- https://archive.org/details/IMG38July1996
- https://github.com/benletchford/systemless/issues/6095
---

Choose File > New Game, then Begin to use the original setup. Drag a recruitment
portrait to a sector you control to queue a hire, then choose Done to end the turn.
The original demo allows only Kill 'Em All for 52 turns and disables saving and
background music. The complete archive includes both documentation parts.

Native testing verifies one recruitment and turn against an omitted-drag control.
Browser controls, pacing, audio, complete scenarios and multiplayer remain
unverified, so browser launch stays disabled.
