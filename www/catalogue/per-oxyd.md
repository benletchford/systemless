---
id: per-oxyd
kind: game
title: Per.Oxyd
summary: >-
  Guide a marble around obstacles and uncover matching stones in Dongleware's
  original follow-up to Oxyd.
developer: Meinolf Schneider
publisher: Dongleware Verlags GmbH
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: 2de32158 private browser; candidate WWW runtime 046859c9
    architecture: 68k
    environment: >-
      Production-equivalent private browser using the complete original archive and
      candidate function-key mapper, with core runtime 0d1c10f7. Actual archive and both
      page/current-worker Wasm response bodies match their recorded pins. F1 reaches
      the original single-player prompt; two ordinary mouse confirmations enter
      landscape 1. No landscape code is entered. Each sixty-tick mouse interval moves
      the marble from its preceding checkpoint (13.03 and 6.47 pixels); the equal-clock
      omitted-input control moves zero pixels. Actual original prompts, gameplay and
      control captures inspected. All four unchanged twenty-second initialization/input
      and separate sustained-first-landscape windows pass: final steady guest and host
      rates 59.741-60.063, maximum worker frame 16.7 ms, minimum audio queue 131.7 ms,
      zero lifecycle/observer errors. Rates are final steady snapshots, not averages
      or rendered game FPS. Measurement starts before F1 and world initialization;
      bounded visible-screen readiness replaces arbitrary caption waits. Earlier
      capture and scene-assertion failures are retained, including a second input
      returning near its initial position. Each movement is measured against its
      preceding frame without lowering the five-pixel threshold. No game bytes,
      original settings, shareware restrictions or pacing gates change. Exact native
      trajectories, stone pairing, completion, later landscapes, links, saves,
      independent visual parity and audio fidelity remain unverified. Serial-reference
      errors persist. Released public-route qualification remains pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5902
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: 9903e995 supplementary native source; browser qualification pending
    architecture: 68k
    environment: >-
      Complete original Macintosh 4.6 distribution. The actual application has
      thirteen CODE resources, no cfrg and an empty data fork; 68K only. Original default
      CPU, display, preferred partition, sound and shareware/landscape-code restrictions
      retained. F1 opens the single-player prompt; ordinary mouse confirmations continue
      the initial landscape and its title. No code is entered. Two mouse movements
      move the marble on landscape 1, while the equal-clock omitted-input control retains
      its position. All four pre-input PNGs are identical. Fresh repeats each pass
      twelve checks (six measured pixels and six strict own screenshot references), 1326
      frontend / 1926 guest ticks, zero exhausted frames. Actual captures inspected. Own
      references prove consistency, not independent hardware parity. Production
      expansion matches independent extraction for all four files, eight forks and Finder
      records. Repeated host signed-reference -6 serial-read errors remain; this does not
      resolve the older Oxyd issue. Browser input/pacing, stone pairing, level
      completion, later landscapes, links, saves, independent visual parity and audio fidelity
      remain unverified. No game bytes, preferences, code stones or registration state
      are changed.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5902
runtime:
  executable_path: Per.Oxyd™ 4.6 Installer/Per.Oxyd™/Per.Oxyd™
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 35310ece5ecea2591905e507dae0ec7e8f2189a304cd79b8b8cf40c79594f358
    size_bytes: 1842404
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.lip6.fr/pub/mac/info-mac/_Game/arc/per-oxyd-46.hqx
    - https://github.com/benletchford/systemless/issues/5902
    license: Original Per.Oxyd shareware copying permission
    rights_holder: Dongleware Verlags GmbH
    permission: >-
      Per.Oxyd's own title/menu explicitly identifies this software as shareware and
      expressly permits and encourages copying, in English, German and French. Preserve
      the complete unchanged distribution and its original shareware, registration and
      landscape-code restrictions. No code-book or standalone component licence is
      claimed. This is its own captured permission, not a grant transferred from the
      earlier Oxyd game.
    notes: >-
      Original 1842404-byte Info-Mac BinHex/StuffIt distribution, SHA-256
      35310ece5ecea2591905e507dae0ec7e8f2189a304cd79b8b8cf40c79594f358, containing the Compact Pro
      AutoExtractor. All four expanded paths (game, Data A, Data B and folder icon),
      eight forks and four Finder records match independent unar extraction. Strict pack
      EOF 1497271, SHA-256
      c317fc2f878f131b3921d0391533fb274ea14655f16105beb65b95ae5fdec434. No standalone Read Me is bundled. Application text resources and the
      actual title/menu copying permission were inspected; the old Oxyd Command-9
      information sequence does not apply to this UI. The original archive is neither stripped
      nor repacked.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b059ac9bfb7e8f24c9ee28f18d82ac3c9af3d1c9967cedee83dc7746228d8cf5
    size_bytes: 55976
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5902
    permission: >-
      Actual native gameplay capture from the permitted unchanged original
      distribution. Underlying artwork remains Dongleware's property.
    notes: >-
      Landscape 1 after ordinary mouse input, losslessly cropped from native
      framebuffer coordinates (80,100)-(720,500) to the 640x400 game surface and HUD. Excludes
      host desktop and emulator framing. PNG SHA-256
      b059ac9bfb7e8f24c9ee28f18d82ac3c9af3d1c9967cedee83dc7746228d8cf5, 55976 bytes. No generated or repainted artwork.
references:
- https://github.com/benletchford/systemless/issues/5902
- https://www.mobygames.com/game/65770/oxyd-2/releases/
---

## Another world of marbles and stones

Roll the marble around each landscape, avoiding obstacles while uncovering
pairs of matching Oxyd stones. This is the original Macintosh follow-up to
Oxyd, with its own landscapes and presentation.

Press **F1** for single-player mode, then use the normal mouse confirmations to
begin the first landscape. Move the mouse to guide the marble. The original
shareware and landscape-code requirements remain intact.

![Per.Oxyd landscape 1 after mouse input](https://assets.systemless.org/catalogue/media/sha256/b0/b059ac9bfb7e8f24c9ee28f18d82ac3c9af3d1c9967cedee83dc7746228d8cf5.png)
