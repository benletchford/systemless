---
id: trophy-bass-demo
kind: game
title: Trophy Bass Demo
summary: Cast a line at Lake Murray in Sierra's original fishing simulation demo.
developer: Dynamix, Inc.
publisher: Sierra On-Line, Inc.
year: 1995
architectures:
- 68k
- ppc
default_architecture: ppc
category: Simulation
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: c5f7760f native-equivalent runtime
    architecture: ppc
    environment: "Complete unchanged original SIT and original guest defaults, using the genuine shipped PPC slice. Go To > Quick Fish reaches Lake Murray. Clicking Cast animates the rod and displays line and depth counters; the button label remains Cast. An omitted-button control starts from an RGB-identical frame and remains uncast. Both runs finish 920 frontend and 899 guest ticks with zero exhausted frames; the control passes eight observed pixel assertions. The active run has zero script assertions: inspected captures and the external RGB comparison establish input response, with 6291 pixels different while held, 13020 after casting and 12921 at the final frame. This verifies bounded casting, not catching a fish, a complete round, browser pacing/audio or independent hardware accuracy. Frozen binary b3d7252a uses source 97bb8282, matching c5f7760f runtime source and preceding later 6363b91e changes."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6085
runtime:
  executable_path: Trophy Bass Demo/Trophy Bass Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 3740fb96db951965d41079cea082e38623f9732f391589b8b2c83b56d0383122
    size_bytes: 5554204
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/trophy-bass
    - https://archive.org/details/IMG33Dec1995
    - https://github.com/benletchford/systemless/issues/6085
    license: Maintainer-accepted promotional demo
    rights_holder: Sierra On-Line, Inc., Dynamix and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance,
      without claiming an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed notices. Preserve the
      complete original archive, all five game/data/info/music/sound files, original
      credits, notices and demo limits.
    notes: >-
      All five files, ten raw forks, all 353 resource payloads, names, IDs and
      attributes, and all five Finder type/creator/flag records match Inside Mac Games disc
      33, December 1995, exactly. No period-wrapper or whole-disc byte identity is
      claimed. Full production packing also matches independent unar extraction on all five
      files, ten forks and five Finder records. Notice screening covered nineteen
      selected included standard UI/text/version resources, without standard compression. No
      selected TEXT resources or notice-term hits were present. The original visible
      Sierra 1995 title copyright was inspected. The custom IFhs Info file and opaque
      game Data were not decoded; no artwork-wide text extraction is claimed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 3ba5ef092d75805c7ffe5e4c144c4dbaac5f98b1403a603ec752b478bf770e9a
    size_bytes: 112109
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6085
    rights_holder: Sierra On-Line, Inc., Dynamix and original contributors
    permission: Fresh content-only Systemless capture of the project-policy-accepted demo.
    notes: >-
      Unscaled 640x480 crop of actual native gameplay after casting, with the rod and
      line/depth counters visible. Excludes desktop, Classic Mac menu bar and title
      framing. Original artwork retains its copyright.
references:
- https://classicmacdemos.com/trophy-bass
- https://coverdiscs.com/disc/inside-mac-games-vol-3-issue-12/
- https://www.mobygames.com/game/4831/trophy-bass/releases/
- https://github.com/benletchford/systemless/issues/6085
- https://github.com/benletchford/systemless/issues/6087
---

![Casting at Lake Murray in the original demo](https://assets.systemless.org/catalogue/media/sha256/3b/3ba5ef092d75805c7ffe5e4c144c4dbaac5f98b1403a603ec752b478bf770e9a.png)

Choose Go To > Quick Fish from the original menu, then click Cast to throw the
line. The complete demo preserves its game, data, info, music and sound files.

Native testing verifies a cast against an idle control. Catching a fish, a complete
round and browser gameplay remain unverified. The first private browser title
sample renders at 57.44 host FPS but advances only 29.20 guest ticks per second,
below the required 50, with a 54.3 ms maximum frame against a 50 ms limit. Audio
queueing passes that sample. Browser launch stays disabled while
[the pacing issue](https://github.com/benletchford/systemless/issues/6087) is open.
