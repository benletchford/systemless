---
id: pajama-sam-demo
kind: game
title: "Pajama Sam: No Need to Hide When It's Dark Outside Demo"
summary: Visit the Land of Darkness in Humongous Entertainment's original Macintosh demo.
developer: Humongous Entertainment
publisher: Humongous Entertainment
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: 0a0c6a31 native
    architecture: 68k
    environment: >-
      Complete unchanged publisher MacBinary using the original 68k version at
      default machine settings. Dismiss the visible startup OK dialog and use the
      publisher-documented Escape shortcut for the intro and arrival cutscenes. Click
      the visible left path at 95,300: Sam reaches the mine-cart room. A matched
      omitted-click control stays beside the tree. Starting RGB frames are identical;
      both runs finish 2080 frontend and 2680 guest ticks with zero exhausted frames.
      External image checks find 14766 orange floor pixels in the observed lower-room
      region versus zero in the control; scripts contain zero assertions. Actual room
      and control crops inspected. No modified game files, preferences or CPU, memory
      or display overrides. This verifies bounded navigation, not later puzzles,
      persistence, audio fidelity, other architectures, browser input/pacing or
      independent hardware accuracy.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6140

runtime:
  executable_path: Pajama Sam Demo
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.cavedog.com/humongous/PajamaSamDemo.sit.bin
    expected_sha256: 8f365bf609b6d3d1cb036ec791ff979bbc7565e034074481310c20c03644a444
    expected_size: 17833088
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.cavedog.com/humongous/
    - https://ftp.zx.net.nz/pub/archive/ftp.cavedog.com/humongous/PajamaSamDemo.sit.bin
    - https://discmaster.textfiles.com/file/11876/ftp.cavedog.com.tar/ftp.cavedog.com/humongous/PajamaSamDemo.sit.bin
    - https://github.com/benletchford/systemless/issues/6140
    license: Maintainer-accepted promotional demo
    rights_holder: Humongous Entertainment, Apple Computer and original contributors
    permission: >-
      Accepted under the existing project policy for complete, unchanged original
      promotional demos preserved from period publisher distribution. This records
      project-policy acceptance, not an affirmative publisher or third-party licence
      grant. No applicable explicit redistribution restriction was identified in the
      reviewed notices. Preserve all original files, credits, notices, bundled
      components and demo limits.
    notes: >-
      The complete original publisher FTP download independently matches the
      preserved ftp.cavedog.com.tar file byte-for-byte, including its MacBinary and
      StuffIt wrappers. All nine files are retained, including the 11510-byte publisher
      Readme and bundled Apple system software. The complete Readme and 396 readable
      selected standard UI/text/version resources were reviewed, with zero compressed
      unread records. Original startup and room screens were inspected; PICT artwork
      was not decoded for text. Production packing independently matches all nine
      files, eighteen forks and nine Finder records against unar. Opaque game data,
      scripts and guest instructions were not decoded.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/pajama-sam-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6140
    rights_holder: Humongous Entertainment and original contributors
    permission: Fresh content-only capture of the project-policy-accepted original demo.
    notes: >-
      Unscaled 640x480 crop of the original mine-cart room from the complete
      unchanged publisher download. Excludes desktop, Classic menu bar and outer
      framing. Original pixels are unretouched; no broader artwork licence is claimed.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.cavedog.com/humongous/
- https://github.com/benletchford/systemless/issues/6140
---

Meet **Pajama Sam** in the Land of Darkness in this original Macintosh demo.
The complete publisher package includes its original Readme and bundled components.

The publisher describes point-and-click exploration, inventory items and **Escape**
to fast-forward the current cutscene. Native navigation along the left path to the mine-cart room is verified.
Browser play remains pending.

![Pajama Sam in the mine-cart room](incoming/pajama-sam-demo/gameplay.png)
