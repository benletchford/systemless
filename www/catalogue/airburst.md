---
id: airburst
kind: game
title: Airburst
summary: >-
  Rotate your bat to deflect razor balls, burst rival balloons and survive the
  arena.
developer: Aaron and Adam Fothergill
publisher: Strange Flavour
year: 2001
architectures:
- ppc
default_architecture: ppc
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-13"
    tester: Catalogue maintainer
    systemless_version: a8fe92e087f4ce5838cf9e8da9988fa6818d5925
    architecture: ppc
    environment: "Merged-master optimized native build, untouched original 1.0 shareware archive, exact selector below and depth 16. Normal menu/Play starts the four-player arena. Held default left/right arrows rotate BEM's bat in both directions; a live ball and CPU score progression pass 25 measured pixel checks at 722 frontend/guest ticks with zero exhaustion. The matched no-key run passes nine stationary-bat checks. Captures inspected. All 32 original files/64 forks and supported Finder metadata match independent extraction and production packing. Actual game is PPC-only; helper CODE does not establish a native 68K slice. Fullscreen, Carbon math and genuine cooperative movie callbacks are merged in #4742, #4744 and #4746. Options background artifacts remain tracked in #4745. Browser, promotion, launch approval, full games, human-player scoring, saving and audio fidelity remain unverified. Hosted verification below supersedes the initial launch hold."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4698
  - date: "2026-10-13"
    tester: Catalogue maintainer
    systemless_version: a8fe92e087f4ce5838cf9e8da9988fa6818d5925
    architecture: ppc
    environment: >-
      Browser launch approved after two optimized Chrome 151 Worker/Canvas2D
      staging replays at default 25 MHz and implicit PPC 16-bit depth. The
      unreleased frontend is bridged under production origin; the actual managed
      archive is untouched, with no local archive fixture or security override.
      Fresh archive/screenshot downloads match pinned sizes and SHA-256 values.
      Normal menu/Play enters the original arena. Six measured blue-bat region
      checks pass twice for left/right arrow rotation; inspected captures show a
      live ball and CPU score progression. Steady pacing is 61.017 and 62.172
      guest ticks/sec, above the unchanged minimum 50, with no console errors.
      Page compositor captures show the complete arena; later raw-canvas exports
      can capture a different intermediate image and are not exact frame oracles.
      Original Options background artifacts remain in #4745. Full games,
      human-player scoring, saves and audio are unverified. This is hosted-asset
      staging approval; human release and direct public qualification are pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/4748
runtime:
  executable_path: Airburst 1.0 Shareware/Airburst/Airburst Carbon
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 4e36cd7b1f2559f5bd0330f6dac169b9d95ec245adf1c37dac6e3ad03f17f8b5
    size_bytes: 7760231
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/a/
    - https://github.com/benletchford/systemless/issues/4698
    license: Original complete unchanged Airburst shareware distribution grant
    rights_holder: Aaron and Adam Fothergill
    permission: >-
      The original Airburst Manual.html explicitly permits free distribution of the
      game with this manual and every associated file unchanged. Preserve the authors'
      credits, copyright and complete package. Personalised registration keys must not
      be distributed. No separate reuse of its art, music, characters or manual is
      granted.
    notes: "Temporary checksum-verified HTTPS transfer supplies the unchanged original bytes because publisher access returns HTTP 403 to CI; publisher provenance is retained above. Normal CI has rewritten this source to immutable managed storage; the temporary transfer is retired. Original 1.0 outer StuffIt archive contains an ST65 self-extractor. Keep the original bytes: the production packer preserves all 32 bundled files, 64 forks, supported Finder metadata, manual pictures, music, backgrounds, RezLib, StrangeIntro and skin template. The 164250-byte game data fork is genuine Joy!/peff/pwpc with cfrg/carb and no CODE. Levels and DeathMatch remain available without registration; the other nine modes and balloon editor stay locked. No key or availability bypass is supplied."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 1efb53cb6f57752dc82a4cff78d3c7173d813f18cff533fbe28da423be520904
    size_bytes: 703170
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4698
    permission: >-
      Actual Systemless gameplay of the intact original package, captured as
      catalogue evidence at the maintainer's request; no standalone game-art extraction or
      reuse is supplied.
    notes: >-
      Original four-player arena after default right-arrow input, with live ball and
      CPU scores. The game owns the entire 800x600 guest display; no desktop, classic
      menu bar, emulator framing or host UI is included.
references:
- https://www.vintageapplemac.com/software/games/a/
- https://github.com/benletchford/systemless/issues/4698
- https://github.com/benletchford/systemless/issues/4745
---

Click **Play** to enter the arena with the current players and game settings.
BEM's default controls are the left/right arrows to rotate the bat and the up
arrow for the action button when a sticky bat or ammunition is available.
The original Options page lets you change keys or select mouse control.

Deflect balls toward opponents' shield balloons and collect power-ups. This
complete original shareware package includes Levels and DeathMatch; the other
nine game types and balloon editor retain their registration restrictions.
No registration key or game patch is included.

Native bat rotation, ball movement and CPU score progression are verified.
Hosted-archive browser controls and pacing are approved. The Options background has a known rendering issue;
full games, human-player scoring, saving and audio remain unverified.

![Airburst original four-player arena](https://assets.systemless.org/catalogue/media/sha256/1e/1efb53cb6f57752dc82a4cff78d3c7173d813f18cff533fbe28da423be520904.png)
