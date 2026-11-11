---
id: you-dont-know-jack-demo
kind: game
title: You Don't Know Jack Demo
summary: >-
  Buzz in for irreverent multiplayer trivia in the original five-question demo.
developer: Berkeley Systems and Jellyvision
publisher: Berkeley Systems, Inc.
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: 97bb8282 native candidate
    architecture: 68k
    environment: "Complete unchanged original HQX, original guest defaults and genuine shipped 68k slice. Category 2 selects Zoiks! Forsooth! and opens a multiple-choice question. Q highlights Angie, and answer 2 advances to the next category. Active and omitted-buzzer/answer control each finish 3306 frontend ticks with zero exhausted frames. RGB-normalized pre-answer images match exactly; the active-only buzzer highlight changes 4880 pixels, and post-answer frames differ at 306765 pixels. The control checks five observed pixels; active scenario completion alone is not the gameplay evidence. Named captures were inspected and compared. This verifies bounded native input and advancement, not the resulting score value, full-demo completion, browser pacing/audio or independent hardware accuracy."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6079
runtime:
  executable_path: Double-Click-Me To Install Jack/Jack Demo
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://download.classicmacdemos.com/YDKJ.hqx
    download_page: https://classicmacdemos.com/you-dont-know-jack
    expected_sha256: d8a4b6b8abdf1ccf852f33aebee14b7d5112e1b6ea772e30d0bb65bc1c182e88
    expected_size: 16166552
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/you-dont-know-jack
    - https://archive.org/details/macaddict-cd-022
    - https://github.com/benletchford/systemless/issues/6079
    license: Maintainer-accepted promotional demo
    rights_holder: Berkeley Systems, Inc., Jellyvision and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance,
      without claiming an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed notices. Preserve the
      complete original HQX, installer stub, all components and both QuickTime
      alternatives, original credits, notices and five-question demo limit.
    notes: >-
      MacAddict 22, June 1998, includes the five-question demo and original readme.
      All question files, executable and other data forks match. The installed period
      copy has initialized YDKJ._HD data of 48 rather than 50 bytes, two extra state
      files and lacks the installer payload's SIZE 0/1 resources. All other executable
      resource payloads match. No full-core or period-wrapper byte identity is claimed.
      Full production packing preserves all fifteen entries, thirty forks and fifteen
      Finder records, matching independently verified extraction. Duplicate QuickTime
      components remain under separate parents without guessing installer conditions.
      Notice review screened 423 selected installer/payload UI/text/version resources
      and 228 records in the separately recovered first QuickTime/PowerPlug extraction;
      these scopes overlap. Included TEXT and notice hits were read. Thirty-four custom
      ADCR installer UI records remain unread and disclosed; no complete artwork or
      installer UI text extraction is claimed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/you-dont-know-jack-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6079
    rights_holder: Berkeley Systems, Inc., Jellyvision and original contributors
    permission: Fresh content-only Systemless capture of the project-policy-accepted demo.
    notes: >-
      Unscaled 640x480 crop of the actual native multiple-choice question. Excludes
      desktop, Classic Mac menu bar and emulator framing. Original artwork retains
      its copyright.
references:
- https://classicmacdemos.com/you-dont-know-jack
- https://coverdiscs.com/disc/macaddict-issue-022/
- https://www.jackboxgames.com/blog/our-earliest-memories-of-you-dont-know-jack
- https://github.com/benletchford/systemless/issues/6079
- https://github.com/benletchford/systemless/pull/6077
- https://github.com/benletchford/systemless/issues/6078
---

![A multiple-choice question in the original demo](incoming/you-dont-know-jack-demo/gameplay.png)

The complete original five-question demo preserves its questions, sound, graphics,
installer and supplied QuickTime and Sound Manager components.

Choose a category with 1, 2 or 3. The players' buzzer keys are Q, B and P; buzz in
and use a number key to answer. The native 68k run verifies category selection,
buzzing in and answer-driven advancement.

Browser launch stays disabled pending qualification. The original PowerPC slice
also has a separate [PEF loading issue](https://github.com/benletchford/systemless/issues/6078).
