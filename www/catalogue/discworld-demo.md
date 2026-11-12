---
id: discworld-demo
kind: game
title: Discworld Demo
summary: Explore Rincewind’s room in the original Macintosh Discworld adventure demo.
developer: TWG
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: '2026-11-13'
    tester: Catalogue maintainer
    systemless_version: f0e42699 native
    architecture: 68k
    environment: Complete unchanged original demo and original defaults. Return at title, then
      full intro without Escape or Space. Calibrated mouse h240/v200 visibly targets Wardrobe;
      active button edges move Rincewind from bed to wardrobe while paired omission remains
      by bed. Both complete 38072 frontend ticks / 38672 guest ticks with zero exhausted frames.
      Before-input RGB captures match exactly; after-input 9636 pixels differ. Actual images
      inspected. Bounded startup, targeting and walking only. A further paired double-click
      leaves wardrobe closed; object use is unverified. Full adventure, hardware accuracy, audio
      fidelity and browser pacing remain unverified. Frozen native binary SHA256 eb967b5de7e8941baa35608654303611d0123484b9b7a4c53f130bde9cced2fa.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6210
runtime:
  executable_path: Discworld/Discworld
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/discworld/
    download_page: https://classicmacdemos.com/discworld
    expected_sha256: e282ef9e4a7662df4216a1f556274bb452c2c3efd4ef88dc2c317fe33994109b
    expected_size: 45904252
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/discworld
    - https://archive.org/details/IMG37MayJune1996
    - https://github.com/benletchford/systemless/issues/6210
    license: Maintainer-accepted promotional demo
    rights_holder: Original game rightsholders and contributors
    permission: Accepted under the user-authorized existing project policy for complete unchanged
      period promotional demos. This records project-policy acceptance, not an affirmative publisher
      or artwork licence grant. No explicit game redistribution restriction identified in the
      reviewed notices. Preserve all original files, notices and demo limits.
    notes: Complete 46-file demo in preserved StuffIt container; wrapper not claimed publisher-authored.
      All 92 raw forks and complete file set match Inside Mac Games May/June 1996. ISO size
      and published SHA1 verified. All type/creator records and 45 Finder flags match; only
      AppleShare PDS flags differ (download 0x6000, period 0x7000). Production packing matches
      all files, raw forks and Finder metadata against independent unar/lsar. All 11 standard
      game UI/string/version resources reviewed without parse errors. Full 3663-byte IMG ReadMe
      explicitly permits distributing commercial game demos; its magazine-content restrictions
      are retained and no magazine content included. MacFormat 29 contains a different variant
      with an editorial startup advisory; no files are reconstructed from that variant. Opaque
      game data and scripts not interpreted. No original files stripped, replaced or modified.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/discworld-demo/discworld-gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6210
    rights_holder: Original game rightsholders and contributors
    permission: Real content-only capture of the project-policy-accepted promotional demo; no
      separate artwork licence grant claimed.
    notes: Unscaled unretouched 640x400 lossless pixel crop from actual native gameplay after
      walking to the wardrobe. No surrounding desktop or menu bar.
references:
- https://classicmacdemos.com/discworld
- https://archive.org/details/IMG37MayJune1996
- https://github.com/benletchford/systemless/issues/6210
---

Play the original Macintosh **Discworld** demo. Press **Return** at the title and let the opening sequence finish before exploring Rincewind’s room. Follow the game’s visible cursor when selecting a destination.

Native startup and walking response are verified. Browser launch remains disabled pending browser qualification.

![Rincewind at the wardrobe](incoming/discworld-demo/discworld-gameplay.png)
