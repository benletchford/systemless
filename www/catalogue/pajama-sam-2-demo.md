---
id: pajama-sam-2-demo
kind: game
title: "Pajama Sam 2: Thunder and Lightning Aren't So Frightening Demo"
summary: Visit the weather factory in Humongous Entertainment's original Macintosh demo.
developer: Humongous Entertainment
publisher: Humongous Entertainment
year: 1998
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
      Complete unchanged publisher MacBinary download at default machine settings.
      Choose the original Set to 256 Colors button, then use the publisher-documented
      Escape shortcut to finish a cutscene. The title and first weather-factory room
      render. The original publisher run finishes 2056 frontend and 2656 guest ticks with
      zero exhausted frames. Actual title and room captures inspected. Floor and doorway
      clicks have not established character movement or a puzzle interaction; these
      runs are not puzzle-input qualification. The publisher-documented inventory hover
      does reveal Sam's bag at the bottom of the room. No modified files or CPU, memory
      or display override. The original PPC version halts after the color prompt and is
      not enabled. Browser input/pacing, later puzzles, persistence, audio fidelity and
      independent hardware accuracy remain unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/6136
runtime:
  executable_path: Pajama Sam 2 Demo/PJ2Demo
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: 559201dccfaa2901bcb1242168f82b5576f63f8087e858700ae7284cef12755e
    size_bytes: 6229632
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.cavedog.com/humongous/
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.cavedog.com/humongous/PajamaSam2Demo.sit.bin
    - https://coverdiscs.com/disc/machome-february-1999
    - https://archive.org/details/MacHome_CD_February_and_March_1999
    - https://github.com/benletchford/systemless/issues/6136
    license: Maintainer-accepted promotional demo
    rights_holder: Humongous Entertainment, Apple Computer and original contributors
    permission: >-
      Accepted under the existing project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance, not an
      affirmative publisher or third-party licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed notices. Preserve the complete
      original package, all notices, bundled components and demo limits.
    notes: >-
      The preserved publisher FTP package contains ten files, including its full
      16414-byte Demo Sampler ReadMe. The modern mirror omits that ReadMe and was not
      selected. Nine shared game and Apple-system files match the actual MacHome
      February/March 1999 CD in data content and all resource payloads, names, IDs and attributes.
      Raw resource maps and whole wrappers are not claimed identical. The publisher
      package additionally retains its ReadMe and an empty folder. The complete ReadMe
      and 400 readable selected standard UI/text/version records were reviewed, with zero
      compressed unread records. Original title and first-room screens were inspected;
      PICT artwork was not decoded for text. Production packing independently matches
      all ten files, twenty forks and ten Finder records against unar. Opaque game
      data, scripts and guest instructions were not decoded.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 2daaed1d9b4febd6e12c1c5772ef1f9250631c101ff1489a8e6b06b28ff12786
    size_bytes: 317668
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6136
    rights_holder: Humongous Entertainment and original contributors
    permission: Fresh content-only capture of the project-policy-accepted original demo.
    notes: >-
      Unscaled 640x480 crop of the original weather-factory room from the complete
      unchanged publisher download. Excludes desktop, Classic menu bar and outer framing.
      No pixels were retouched; no broader artwork licence is claimed.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.cavedog.com/humongous/
- https://coverdiscs.com/disc/machome-february-1999
- https://github.com/benletchford/systemless/issues/6136
---

Visit the weather factory with **Pajama Sam** in the original Macintosh demo.
The publisher's ReadMe describes a quest to find the missing Snowflake Inspector.
The complete package retains that ReadMe and its original bundled components.

The original controls use point-and-click interaction and inventory items.
**Escape** fast-forwards the current cutscene. Native startup reaches the first
room; interactive gameplay and browser play remain unverified.

![Pajama Sam 2 weather-factory room](https://assets.systemless.org/catalogue/media/sha256/2d/2daaed1d9b4febd6e12c1c5772ef1f9250631c101ff1489a8e6b06b28ff12786.png)
