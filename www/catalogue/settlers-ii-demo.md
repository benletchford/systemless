---
id: settlers-ii-demo
kind: game
title: The Settlers II Demo
summary: >-
  Build a Roman settlement after a shipwreck in the original Macintosh demo of
  Blue Byte's settlement and logistics game.
developer: Blue Byte and Similis
publisher: Blue Byte
year: 1997
architectures:
- ppc
default_architecture: ppc
launch_enabled: true
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: d36bbda2 with local AGL candidate
    architecture: ppc
    environment: >-
      Complete original English Macintosh demo. At default settings, select Roman
      Campaign, Play chapter and Continue in the opening diary. The first settlement
      renders at 2,242 frontend and guest ticks with zero exhausted frames. A matched
      input and no-input pair confirms that clicking headquarters opens its inventory.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6026
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: 5eb8fdef private browser preview
    architecture: ppc
    status: playable
    environment: >-
      Chromium worker and WebGL with the original archive, default PowerPC
      settings and both Wasm response bodies verified. Enter Roman Campaign,
      play the first chapter and dismiss the diary when it appears. Clicking
      headquarters opens the inventory panel and leaves it visible throughout
      the 20-second settlement sample. The matched no-input run shows the
      settlement with no panel. All four 20-second campaign and settlement
      windows pass pacing, with maximum worker frames of 28.6, 20.7, 20.8 and
      21.0 ms and minimum audio queues of 119.4, 122.1, 132.7 and 124.9 ms.
      Building, later chapters, persistence and released browser play remain
      unverified.
    evidence: https://github.com/benletchford/systemless/issues/6026
runtime:
  executable_path: Settlers II Demo/Settlers II Demo
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: f529f9a0e2c8ca7ddaf0cf2b999ce00b53a951cc9d3ef061592a16362b8a783f
    size_bytes: 6359808
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://bluebyte.settlers2.net/demos/settlers2mac/
    - https://archive.org/details/FTP.BLUEBYTE.COM.2004.03
    - https://github.com/benletchford/systemless/issues/6026
    license: Maintainer-accepted promotional demo
    rights_holder: Blue Byte and original contributors
    permission: >-
      Accepted under the maintainer's project policy for complete, unchanged
      period-distributed promotional demos. No explicit redistribution restriction was
      identified in the reviewed notices. The permission field records project-policy
      acceptance. Preserve the complete archive, original credits, and demo limits.
    notes: >-
      The original English MacBinary distribution is preserved in the Blue Byte FTP
      mirror. Independent unar extraction matches the mirror's BinHex distribution and
      the runtime's eight files, sixteen forks and eight Finder type, creator and flags
      records. The original menu credits Blue Byte and Similis and reports an October
      31, 1997 build date. The application version resource reports 0.9.9b5.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 34807055f91fe0289903dec728cff1ae7e30e680cb79087e6f967e94da0f4ac3
    size_bytes: 513150
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6026
    permission: >-
      Original Systemless gameplay capture. Underlying artwork belongs to its
      original rights holders.
    notes: The Roman Campaign's first settlement in the unchanged English demo.
references:
- https://bluebyte.settlers2.net/demos/settlers2mac/
- https://github.com/benletchford/systemless/issues/6026
---

![The Roman settlement in the first mission](https://assets.systemless.org/catalogue/media/sha256/34/34807055f91fe0289903dec728cff1ae7e30e680cb79087e6f967e94da0f4ac3.png)

Choose Roman Campaign and play the first chapter. The diary asks you to
build accommodations for a woodcutter, carpenter and stonemason. The original
demo limits remain in place.
