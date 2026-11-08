---
id: brick-attack
kind: game
title: Brick Attack
summary: Keep the ball in play, break bricks and catch falling prizes in Dan Zink's original
  Macintosh arcade game.
developer: Dan Zink
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Arcade
compatibility:
  status: playable
  verified:
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: d6633529 native candidate
    architecture: ppc
    environment: Complete unchanged original Brick Attack 1.0 archive; exact Brick Attack/Brick
      Attack executable. PPC, default 25 MHz and original partition. Original Faster graphics,
      Stereo, eight sound channels and VM warning enabled retained. DrawSprocket requests the
      original 640x480 16-bit display/back buffer. Accept defaults and shareware notice, allow
      New Game hover processing before clicking, wait through Level 1 introduction, click to
      release the ball, then hold right arrow. Equal-clock active/control omit only the final
      arrow edges in control. Pre-arrow images match exactly; the final paddle regions differ
      by 1166 pixels. Actual captures show active paddle move right while control stays left.
      Each repeat passes three causal pixels and ten exact prior captures at 898 frontend/guest
      ticks, zero exhausted frames. Own references establish consistency, not hardware accuracy.
      Native binary SHA-256 9e096a66479765d6092dd7cba5dd2a889aaec6e19473f2a40cc1d38f038327da;
      d6633529 differs from its compiled patch only in an SDK-version comment. Runtime fix tracked
      in PR5855. Score remains zero in this pair. Full levels, persistence, audio fidelity,
      browser pacing and released public play remain unverified. Hosted-original browser evidence
      follows below.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5856
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: 4ef07e3a private browser preview on d6633529 runtime
    architecture: ppc
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5856
    environment: Integrity-pinned hosted original in a private source preview, with actual PPC
      worker, default 25 MHz, original partition and 640x480 game mode verified. Original Faster
      graphics, Stereo, eight channels and VM warning remain unchanged. Each 20-second measurement
      includes New Game, level introduction, normal mouse/launch input and gameplay; no hidden
      gameplay warmup. Active holds right arrow; control omits only those final arrow edges.
      The visible paddle moves 264.5 pixels in active and zero in control. Actual before/after
      captures inspected; ball, brick and score progression also visible. Both unchanged pacing
      gates pass. Final steady guest/host rates are 61.2573/60.2364 active and 59.9606/59.9606
      control per second; maximum sampled worker frames 21.9/23.5 ms, minimum audio queues 117/116.8
      ms. These final steady snapshots are not whole-game averages. No captured lifecycle/network
      errors. Actual original archive HTTP200 body matches 1582077 bytes and SHA-256 d509855343315fb7b4efc3f250137af1f7688156e6745af616edc6f7694dd84c.
      Both main/worker Wasm HTTP200 bodies match 17972049 bytes and SHA-256 b5389902f51cd804ecff6834d0238e4f3ef0dd57ba27550b26d1c57b8e3aaabb.
      Private source 4ef07e3a adds only the promoted catalogue entry to runtime d6633529. Initial
      preview setup stopped before building because promotion omits the default-false launch
      field; the corrected private copy explicitly enables launch. A separate command-line archive
      request without site Origin/Referer received403; allowed site-origin and actual browser
      requests return200 with exact body pins. Full levels, persistence, audio fidelity and
      released public play remain unverified.
runtime:
  executable_path: Brick Attack/Brick Attack
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: d509855343315fb7b4efc3f250137af1f7688156e6745af616edc6f7694dd84c
    size_bytes: 1582077
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/brick-attack-10-ppc.hqx
    - https://github.com/benletchford/systemless/issues/5856
    license: Original unchanged nonprofit-distribution grant
    rights_holder: Dan Zink and his licensors
    permission: The full original readme permits nonprofit distribution without prior notice
      as long as the software is not modified in any way. It does not grant permission for for-profit
      distribution and prohibits object-code disassembly/reverse engineering. Retain these restrictions
      and all original files/notices with the unchanged free online archive; no paid bundle,
      registration bypass or game modification.
    notes: Version 1.0, copyright 1998. Full 8200-byte readme reviewed. Complete five-file package
      retains application, readme, level, Register and bundled DrawSprocketLib. No runtime added
      or installed. Original unregistered first-five-level limit retained. Independent verification
      passes all five paths, ten forks and five Finder records. Strict pack 2337298 bytes, SHA-256
      d397fb9c7fec613070d286031afc4f0b0b756708b265e459bebb1dbe4fe89820. Shareware notice and
      original in-game help viewed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 22a314cbef3daf8da0c0ad6d700f4459a573a78f317fd8771f6250faa0ffcd9f
    size_bytes: 353873
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5856
    permission: Original gameplay capture of the unchanged freely distributed game.
    notes: Complete native 640x480 guest game frame without cropping or resampling; excludes
      host interface. Level 1 ball in play after right-arrow paddle movement, score zero.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/arc/brick-attack-10-ppc.hqx
- https://github.com/benletchford/systemless/issues/5856
launch_enabled: true
---

## Keep the ball in play

![Brick Attack Level 1 gameplay](https://assets.systemless.org/catalogue/media/sha256/22/22a314cbef3daf8da0c0ad6d700f4459a573a78f317fd8771f6250faa0ffcd9f.png)

Accept the original startup settings and continue through the shareware notice.
Choose New Game and click to release the ball after the level introduction.
Move the mouse or use the arrow keys to move the paddle. Control also releases
the ball; Tab pauses and resumes play. The original unregistered game includes
its first five levels, with documentation and registration utility preserved.
