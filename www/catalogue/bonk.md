---
id: bonk
kind: game
title: Bonk
summary: Click the popping targets before they escape in Mike Darweesh's Macintosh arcade game.
developer: Mike Darweesh
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: d6633529 native candidate; browser qualification pending
    architecture: 68k
    environment: Complete unchanged Bonk 1.4.3 original package; exact Bonk 1.4.3.cpt/Bonk 1.4.3 Folder/Bonk
      1.4.3 (256 Colors) executable. 68K/default 25 MHz/default 800x600 8-bit/original partition. Default
      Level 1 and Freeze Level off retained, sound unchanged. Click Begin, wait for targets, then click
      two visible targets. Active score advances from zero to one to two; equal-clock control omits only
      target button edges and stays zero. Actual captures inspected. Both repeats pass two causal score
      pixels and five exact prior captures at 288 frontend ticks and guest TickCount 888, zero exhausted
      frames. Own captures establish consistency, not hardware accuracy. Native binary SHA-256 9e096a66479765d6092dd7cba5dd2a889aaec6e19473f2a40cc1d38f038327da.
      No dependency on the separate CDM change is claimed. The earlier baseline runtime1858f837 also passes
      all seven checks with all five captures identical, proving CDM is not required by this native scenario.
      All levels, long play, bombs, audio fidelity, persistence and released public play remain unverified.
      Hosted-original browser evidence follows below.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5862
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: f85543d9 private browser preview on merged38dd032b runtime
    architecture: 68k
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5862
    environment: 'Integrity-pinned hosted original in a private preview; actual source/68K worker/default25MHz/800x6008-bit/original
      partition verified. Original Level1, Freeze Level off and sound settings retained. Begin and two
      visible target mouse inputs occur inside each20-second measurement, no hidden gameplay warmup. Active
      score advances0→1→2; control uses two target pointer moves while omitting target button edges and
      stays0. Actual before/after PNGs inspected; score digits read from rendered pixels against native0/1/2
      masks. Both unchanged pacing gates pass: final steady guest/host rates61.0860/64.3585 active and60.0070/65.5632
      control per second, maximum sampled worker frames48.4/32.8ms, minimum audio queues115.4/121.5ms.
      Final steady snapshots are not whole-game averages. No captured lifecycle/network errors. Actual
      archive HTTP200 body matches240896 bytes and SHA-2560747cf3b0e48da1f21c8a10f6890e76295388fed68cbbb6a026359ab8373b0a5.
      Both main/worker Wasm HTTP200 bodies match17976177 bytes and SHA-25686373a8b8d84ba71a9f0b8443a6e775f109810d826984f14fd02d6287669a1f4.
      Private sourcef85543d9 adds only the promoted entry with launch enabled to runtime38dd032b. Initial
      root-directory trunk invocation failed before compilation; corrected www build passed. Initial urllib
      hosted request returned403, separate site-origin curl and both actual browser requests returned200
      with exact pins. All levels, bombs, long play, persistence, audio fidelity and released public play
      remain unverified.'
runtime:
  executable_path: Bonk 1.4.3.cpt/Bonk 1.4.3 Folder/Bonk 1.4.3 (256 Colors)
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 0747cf3b0e48da1f21c8a10f6890e76295388fed68cbbb6a026359ab8373b0a5
    size_bytes: 240896
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/bonk-143.hqx
    - https://github.com/benletchford/systemless/issues/5862
    license: Original free-distribution grant
    rights_holder: Mike Darweesh
    permission: The original documentation permits free distribution of the program and accompanying documentation
      provided no distribution fee is charged. Its separate request that user groups contact the author
      before archiving or distributing is retained; this free online game entry is not a user-group archive.
      Preserve the original shareware request and all notices with the complete unchanged package. No
      paid bundle or registration bypass.
    notes: Version 1.4.3, copyright 1996 per original vers resource. Full original Word 6 document text
      read, including distribution paragraph and version history. Complete package retains both colour
      and monochrome executables plus original Word documentation. Independent verification matches all
      three paths, six forks and Finder records. Production KPK2 retains the outer BinHex Compact Pro
      container prefix; comparison maps exactly that prefix to lsar's recursively extracted children,
      then checks strict path sets, original forks, sizes, Finder metadata and EOF. Pack 248724 bytes,
      SHA-256 9f152a7bbc74655059c7b1b38a3a8ffcaed807fcabf4380a1ffeecb836ee0032. No runtime added or installed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: a43578aea39c092da86b933fa9ffd68dd80ae0d57ad6b42062a78e9a687e4536
    size_bytes: 2311
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5862
    permission: Original gameplay capture of the unchanged freely distributed game.
    notes: Exact native crop (267,102)-(533,431), 266x329 without resampling; retains complete game board
      and scores, excludes desktop and menu bar. Second successful target hit, score two.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/arc/bonk-143.hqx
- https://github.com/benletchford/systemless/issues/5862
launch_enabled: true
---

## Catch the targets

![Bonk after two successful hits](https://assets.systemless.org/catalogue/media/sha256/a4/a43578aea39c092da86b933fa9ffd68dd80ae0d57ad6b42062a78e9a687e4536.png)

Click Begin to start at the original level-one difficulty, then click the targets
before they escape. Space uses a bomb against the targets currently showing.
Ten escapes end the game. The complete original shareware package includes the
colour and monochrome editions and Mike Darweesh's documentation.
