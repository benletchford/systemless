---
id: ipuzzle-10-68k
kind: game
title: iPuzzle 1.0
summary: Slide the tiles of a scrambled iMac picture back into place.
developer: Alesh Slovak
publisher: Alesh Slovak
year: 1999
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: 97ec1e637d085b447685cc055e4f0e9e8a862b65
    architecture: 68k
    environment: >-
      Release-mode local Chrome preview, normal DOM keyboard and stationary pointer
      input, regular WebAssembly worker, 8 MHz pacing. Command-T scrambled the board;
      two legal adjacent-tile clicks moved tiles, and a nonadjacent click left the
      content unchanged pixel-for-pixel. Preview archive requests used independently
      hash-verified original bytes. Separately, the deployed v0.84.1 browser benchmark
      fetched and hash-checked the exact Systemless-hosted archive without interception,
      then rendered a scramble and tile move. Full solution and audible sound unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3944
  - date: "2026-10-03"
    tester: Catalogue maintainer
    systemless_version: 1cd9de170746430e9395b69811aac592db1b4c64
    architecture: 68k
    environment: >-
      Native public-API harness only, with an 800-by-600 8-bit display and 32-bit
      addressing. The unchanged 68K archive drew the solved board; Command-T scrambled it.
      An adjacent tile click and its reverse swapped the expected cells exactly, a
      nonadjacent click left the board unchanged, and Command-S toggled the Sound menu
      state. Two clean replays produced identical captures. No release-browser run,
      complete puzzle solution, or audio verification has been performed. This record covers native testing only;
      later browser verification is recorded separately.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/3944
runtime:
  runtime_pacing:
    cpu_mhz: 8
  screen_depth: 8
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: a6e049878770fe0c1531159c80ea4b45fd40e808ace8457a472690ef80dc2071
    size_bytes: 89881
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/ipuzzle-10-68k.hqx
    - https://archive.info-mac.org/game/
    rights_holder: Alesh Slovak
    permission: >-
      The bundled ReadMe permits redistribution of the unchanged freeware program
      when its original documentation is included. This is the complete original 68K
      distribution, retaining the application, ReadMe, and icon resource.
    notes: >-
      Original 89,881-byte Info-Mac BinHex/StuffIt archive. The Info-Mac index dates
      this archive to April 9, 1999. Application inspection finds 68K CODE resources
      and no PowerPC code fragment.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: d1a122e2aa6bd71d5dd52d33917549387a468eaa915d78a37513a20c78681cd6
    size_bytes: 7341
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3944
    permission: >-
      Fresh Systemless gameplay capture from the unchanged original freeware package.
      Underlying game artwork remains its owner's property.
    notes: >-
      Authentic native framebuffer after scrambling and moving an adjacent tile.
      Exact 128-by-128 content crop at (100,100) from the 800-by-600 framebuffer; no
      rescaling or retouching. Host framing and the Classic Mac menu and window title bars
      are excluded. This is not a release-browser capture.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/ipuzzle-10-68k.hqx
- https://archive.info-mac.org/game/
---

## Reassemble the iMac

![iPuzzle after a native scramble and tile move](https://assets.systemless.org/catalogue/media/sha256/d1/d1a122e2aa6bd71d5dd52d33917549387a468eaa915d78a37513a20c78681cd6.png)

Press **Command-T**, or choose **File → Scramble**, to mix up the picture. Click
a tile next to the blank square to slide it into place. **Command-S**, or
**File → Sound**, toggles the sound setting.

This is Alesh Slovak's original freeware 68K edition, with its ReadMe preserved
inside the unchanged archive.

## Verification status

Native checks covered scrambling, a valid move and its reverse, an invalid
nonadjacent move, and the sound menu toggle. The screenshot comes from that
native run. Release-mode browser checks now cover Command-T scrambling, two
stationary adjacent-tile clicks, and rejection of a nonadjacent click in the
regular 68K worker player. The exact hosted archive also loads in the deployed
browser benchmark. Full puzzle completion and audible sound remain unverified.
