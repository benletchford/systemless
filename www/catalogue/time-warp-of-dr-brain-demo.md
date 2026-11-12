---
id: time-warp-of-dr-brain-demo
kind: game
title: The Time Warp of Dr. Brain Demo
summary: Steer Beaver Brain through the river in the original Macintosh Time Warp demo.
developer: Sierra On-Line
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-13"
    tester: Catalogue maintainer
    systemless_version: b45f1fcb native
    architecture: 68k
    environment: >-
      Complete unchanged original demo and original defaults, with generic
      RestoreEntries repair in PR6214. Choose English, then move the pointer through Beaver Dam.
      Active and omission runs both complete 1924 frontend ticks / 2524 guest ticks
      with zero exhausted frames. Pre-input RGB images match exactly. Only active moves
      pointer to h500/v450; its beaver swims across the lower river while omission
      remains near the original point. After-input 2156 changed pixels; actual PNGs
      inspected. Natural language-screen and puzzle colours visually match the published
      reference after the generic repair. Bounded startup, colour repair and primary steering
      only; stick pickup, dam construction, other puzzles, complete adventure, audio
      fidelity, hardware accuracy and browser pacing remain unverified. Frozen native
      SHA256 8e8ccf50d35447130c002d51b012699616546c69e19b46441633768b6b4d9429.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6215
  - date: "2026-11-13"
    tester: Catalogue maintainer
    systemless_version: 3fb622c8 private candidate
    architecture: 68k
    environment: >-
      Complete original archive and defaults. The browser reaches the correctly coloured
      language screen, but the premeasurement 1200-guest-tick readiness wait exceeds 60 seconds.
      All first 60 seconds retained: three raw 20-second startup windows advance guest ticks
      at 20.9630, 19.5670 and 18.2161/s, with host rates above 60 FPS. These are startup diagnostics,
      not formal world-input qualification. Archive and both main/worker Wasm HTTP body hashes
      match; Wasm 16331939 bytes, SHA256
      3d133873583e3389aa8254eea4d30ac2f4c0498d157f1d84567daf91b091ee3f.
      The executing-worker wire-byte check was not reached. No browser English-selection,
      steering/omission or sustained-world samples ran. Actual failure image inspected.
      No unchanged retry or longer-wait qualification. Browser launch stays disabled.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/6217
runtime:
  executable_path: Time Warp of Dr. Brain/Dr. Brain
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: eca4e5042bc678da6f8aefb1fe6a017e92f67dc108d12e543e91be193d5e8fa1
    size_bytes: 96607595
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/the-time-warp-of-dr-brain
    - https://archive.org/details/macaddict-cd-008
    - https://github.com/benletchford/systemless/issues/6215
    license: Maintainer-accepted promotional demo
    rights_holder: Sierra On-Line and original contributors
    permission: >-
      Accepted under the user-authorized existing project policy for complete
      unchanged period promotional demos. This records project-policy acceptance, not an
      affirmative publisher or artwork licence grant. No explicit restriction on sharing the
      intact game demo was identified in the reviewed notices. Retain every original
      file, notice and demo limit.
    notes: "Preserved complete StuffIt archive; wrapper not claimed publisher-authored. All 759 file paths, 1518 raw forks and 759 Finder records exactly match the full demo folder on MacAddict 008 April 1997. Period ISO size and published SHA1 verified. Production packing independently matches every file, raw fork and Finder record against unar/lsar with strict EOF. Six standard Mac UI/string/version resources reviewed without errors; all three Microsoft Write ReadMes contain complete 13-byte version text. VER.TXT, full DirectX license, both Japanese driver notices decoded as cp932 and full MacAddict root ReadMe were read. DirectX prohibits standalone redistributable-code distribution; its original Windows components remain inside the complete received demo, without component stripping or repackaging. Original classic and PowerPC executable code is preserved. Only classic play is currently offered: the default native PowerPC path stops at the original 256-colour monitor requirement, with no display override used. No opaque PGF/ScriptO or game instructions decoded. No files modified, replaced or reconstructed."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 862d2d6c91fb553e2c8834ef6c565abe4f559c971404764e973927baac5270a1
    size_bytes: 302108
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6215
    rights_holder: Sierra On-Line and original contributors
    permission: >-
      Actual content-only capture of the project-policy-accepted promotional demo; no
      separate artwork licence grant claimed.
    notes: >-
      Unscaled unretouched 640x480 lossless pixel crop from actual native Beaver Dam
      gameplay after steering the beaver across the river. No desktop or surrounding
      menu bar.
references:
- https://classicmacdemos.com/the-time-warp-of-dr-brain
- https://archive.org/details/macaddict-cd-008
- https://github.com/benletchford/systemless/issues/6215
- https://github.com/benletchford/systemless/issues/6213
- https://github.com/benletchford/systemless/pull/6214
- https://github.com/benletchford/systemless/issues/6217
- https://github.com/benletchford/systemless/releases/tag/v0.120.0
---

Play the original Macintosh **The Time Warp of Dr. Brain** demo. Choose **English**, then move the pointer through the river to steer Beaver Brain.

The original demo is available to download.

![Beaver Brain swimming across the river](https://assets.systemless.org/catalogue/media/sha256/86/862d2d6c91fb553e2c8834ef6c565abe4f559c971404764e973927baac5270a1.png)
