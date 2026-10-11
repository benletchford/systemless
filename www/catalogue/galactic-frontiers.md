---
id: galactic-frontiers
kind: game
title: Galactic Frontiers
summary: Build a planetary empire in a turn-based contest for control of the galaxy.
developer: Human Media
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: fbd12e1e33941e1da82b9ceb61ed28966571ed81
    architecture: 68k
    environment: "Unchanged original 1.0.2 archive, Galactic Frontiers executable, 68K/depth 8. Ordinary title click, Game Setup Start and Command-E reach the star map and advance Turn 1 to Turn 2. Eleven assertions pass at 2006 frontend / 2605 guest ticks with zero exhaustion: three ROM metadata checks and eight measured turn-title pixel checks. Captures inspected. Production packing matches all ten original files, twenty forks and supported Finder metadata. Ship movement, combat, saving, audio and browser qualification remain pending."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4594
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 92afd9bf
    architecture: 68k
    environment: >-
      Optimized Chrome preview with current merged emulator source and this
      promoted original catalogue record, local launch enabled. Controlled
      frontend responses under production origin; actual managed archive
      response untouched, HTTP 200, 333691 bytes and original SHA-256 verified.
      Cross-origin isolation and 800x600 canvas verified. Ordinary title click,
      Game Setup Start and browser Command-E start a game and advance Turn 1
      to Turn 2. Eight measured before/after title pixel checks pass; captures
      inspected. Browser launch approved. Ship movement, combat, saving,
      audio and post-release public replay remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4594
runtime:
  executable_path: Galactic Frontiers/Galactic Frontiers
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 46728fdb8c1676c386586ee38c0de9d54c0993f208e781ba189bd2cc6bfccf50
    size_bytes: 333691
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/c/sw/games/galfront/index.html
    - https://github.com/benletchford/systemless/issues/4594
    license: Original Galactic Frontiers no-profit distribution grant
    rights_holder: Human Media Pty Ltd
    permission: >-
      The original ReadMe permits free distribution only where the distributor has no
      potential direct or indirect profit. The work must not be sold or purchased.
      CD-ROM distribution requires written author permission except for the named original
      exceptions. All Mac OS users may use it free. Preserve the complete unchanged
      package, credits and conditions; this permission applies only to no-profit
      distribution complying with those terms.
    notes: >-
      Original MD5 56817e6f71e69bf1713cc7e0f47040bb matches Gryphel's receipt;
      signature not independently verified. Ten original files include six game presets,
      ReadMe, Finder icon, game and Color Frontiers COLR asset. Actual game is
      resource-only with twelve CODE resources and no cfrg. Color Frontiers is artwork, not a PPC
      executable. All twenty forks and Finder metadata independently match production
      packing. The developer archive page is no longer available; permission is evidenced
      in the original package.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 18ac2bafb988be654c628c950d8822034d017849a7f8af5476f865c5a07ff3a5
    size_bytes: 3943
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4594
    permission: >-
      Original game content under the same no-profit distribution conditions,
      captured for the catalogue at the maintainer's request.
    notes: >-
      Actual Turn 2 star map, cropped to game content without title bar, menus or
      desktop.
references:
- https://www.gryphel.com/c/sw/games/galfront/index.html
- https://github.com/benletchford/systemless/issues/4594
---

Start a new game using the original Game Setup dialog. Each player begins with
one home planet. Command-E ends the current turn; all players' actions resolve
between turns. Click planets to inspect them and plan your empire.

This complete original 1.0.2 package includes its presets, artwork, instructions
and distribution conditions. It may only be distributed without potential direct
or indirect profit and must not be sold. CD-ROM distribution has separate written
permission requirements in the original ReadMe.

New-game startup and one turn transition are verified natively and in the
browser. Ship movement, combat, saving and audio remain unverified.

![Galactic Frontiers Turn 2 star map](https://assets.systemless.org/catalogue/media/sha256/18/18ac2bafb988be654c628c950d8822034d017849a7f8af5476f865c5a07ff3a5.png)
