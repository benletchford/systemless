---
id: oxyd
launch_enabled: true
kind: game
title: Oxyd
summary: >-
  Roll a marble through obstacle-filled landscapes and uncover matching pairs of
  stones in Dongleware's original mouse-controlled puzzle game.
developer: Meinolf Schneider
publisher: Dongleware Verlags GmbH
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: e8846aed private production-equivalent browser; runtime e8e979b3
    architecture: 68k
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5900
    environment: >-
      Original Systemless-hosted 3.8 archive; private frontend only, not released
      public-route qualification. Release profile with fat LTO, one codegen unit,
      SIMD and 4 MiB stack. Actual page and current dedicated-worker Wasm HTTP
      bodies match 15887649 bytes / SHA-256
      eafd085df42eb4872e55e592a693406969123a793272efd2ae42b41a5eb08c1a;
      original archive HTTP body matches 979758 bytes / SHA-256
      485cb9aca4ac03d75dd47730c9330db71ad83d0cff88271d3bf2a0fcd9c064ef.
      Worker/WebGL, isolation, SharedArrayBuffer and original 68K 25 MHz, display,
      partition, sound and code-book limits retained. Each case measures twenty
      seconds starting before Command-3, including first-landscape initialization,
      120-tick settle, two mouse movements or equal-clock omissions. A populated
      marble component moves 120.09 native pixels from its own baseline, while
      omission retains position with zero displacement. Native final coordinates
      and stone-opening are not transferred to browser expectations. Separate
      twenty-second sustained samples stay in the running first landscape;
      ordinary active life loss and respawn remain intact. All four unchanged
      pacing gates pass. Active initialization/sustained final steady guest rates
      are 60.007/61.315 ticks/sec and host rates 60.007/60.293 FPS; control rates
      are 59.983/58.649 and 59.983/59.660. Maximum worker frame is 17.9 ms and
      minimum audio queue 130.1 ms. These are final steady scheduling snapshots,
      not whole-run averages or game-rendered FPS. All observed targets resumed;
      zero lifecycle or observer errors and no concurrent local build. Actual
      input/control and all sustained captures inspected. An earlier case passed
      timing but stopped on exact native-coordinate/stone expectations; it has
      no sustained or qualification claim. Full landscapes, code-book stones,
      two-player links, saves, independent visual parity, audio fidelity and
      released public delivery remain unverified.
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: 9903e995 supplementary native source; browser qualification pending
    architecture: 68k
    environment: >-
      Original Macintosh 3.8 distribution. Actual game has thirteen CODE resources
      and no PPC fragment. Original default CPU, display, application partition, sound
      and code-book restrictions retained. Command-3 starts the first landscape. Two
      ordinary mouse movements move the marble and reveal a stone; the equal-clock
      omitted-input control retains the starting marble and covered stone. Pre-input
      screenshots are identical. Fresh repeats each pass ten checks (six measured pixels and four
      strict own screenshot references), 782 frontend ticks / 1382 guest ticks, zero
      exhausted frames. Own references check consistency, not independent hardware
      parity. Actual captures inspected. Production expansion matches independent extraction
      for all eight files, sixteen forks and Finder records. Repeated signed-reference
      -6 serial read errors remain in the host log; this does not resolve the older
      3.7 issue. Browser input and pacing, full landscapes, code-book play, two-player
      links, persistence, independent visual parity and audio fidelity remain unverified.
      No game bytes, preferences or restrictions are modified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5900
runtime:
  executable_path: Oxyd™ 3.8 Installer/Oxyd™ 3.8/Oxyd™
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 485cb9aca4ac03d75dd47730c9330db71ad83d0cff88271d3bf2a0fcd9c064ef
    size_bytes: 979758
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.lip6.fr/pub/mac/info-mac/_Game/arc/oxyd-38.hqx
    - https://github.com/benletchford/systemless/issues/5900
    license: Original in-game Dongleware distribution permission
    rights_holder: Dongleware Verlags GmbH
    permission: >-
      The original Command-9 Dongleware information screen expressly permits and
      encourages copying the disk and passing it to others. Preserve the complete unchanged
      distribution, all three language documents, original data and code-book
      restrictions. Permission concerns this original game distribution; no modern source-port
      or separate code-book licence is claimed.
    notes: >-
      Original 979758-byte Info-Mac BinHex/StuffIt wrapper containing the Compact Pro
      AutoExtractor and complete Macintosh 3.8 game. All eight expanded paths, sixteen
      forks and eight Finder records match independent unar extraction; strict pack
      EOF 971766, pack SHA-256
      8ec70abd41be6aff18033a91d0ceef0c916efe23f12dd959c7829c57e4b0f283. All three bundled Read Me documents were read. The permission screen was
      captured and inspected through the original UI. The archive is neither stripped
      nor repacked; its complete original contents remain intact.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: a78baa92cd1de62e343340ad4ea00bac28503dcec607b4a783caffbf992b95f7
    size_bytes: 27000
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5900
    permission: >-
      Actual native gameplay capture from the permitted unchanged original
      distribution. Underlying artwork remains Dongleware's property.
    notes: >-
      First landscape after ordinary mouse input, losslessly cropped from native
      framebuffer coordinates (80,100)-(720,500) to the 640x400 game surface and HUD.
      Excludes host desktop and emulator framing. PNG SHA-256
      a78baa92cd1de62e343340ad4ea00bac28503dcec607b4a783caffbf992b95f7, 27000 bytes. No generated or repainted
      artwork.
references:
- https://github.com/benletchford/systemless/issues/5900
---

## Roll, remember and match

Guide the marble with the mouse and touch stones to uncover their patterns.
Open a matching pair consecutively to keep both revealed, while avoiding the
hazards scattered around each landscape.

Choose **Command-3** from the main menu to begin the first landscape.
**Command-8** opens the guide and **Command-9** explains Dongleware. The original
code-book requirements for later landscapes remain in place.

![Oxyd first landscape after mouse input](https://assets.systemless.org/catalogue/media/sha256/a7/a78baa92cd1de62e343340ad4ea00bac28503dcec607b4a783caffbf992b95f7.png)
