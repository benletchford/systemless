---
id: nethack
kind: game
title: NetHack
summary: Explore a changing dungeon with a companion and recover the Amulet of Yendor.
developer: The NetHack DevTeam
year: 2003
architectures:
- ppc
default_architecture: ppc
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: c1a98415f47e04887b7afb0c4d8b302cfd5bfea4
    architecture: ppc
    environment: >-
      Unchanged publisher 3.4.3 Carbon archive, NetHack executable, PPC/depth 8.
      Ordinary Player character creation, Space and Return reach the dungeon. Keypad 4
      moves the player and displaces the dog. Eight measured pixel assertions pass at 1532
      frontend/guest ticks with zero exhaustion; captures inspected. Production packing
      matches eight original files, sixteen forks and supported Finder metadata.
      Browser, broader gameplay, saving and audio remain unverified. The earlier 3.3.1 68K
      edition remains unqualified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4568
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 92afd9bf
    architecture: ppc
    environment: >-
      Optimized Chrome preview, local frontend under production origin with
      launch enabled. Actual managed archive response untouched, HTTP 200,
      original 1083366-byte body and SHA-256 verified. Cross-origin isolation,
      actual runtime worker and 800x600 canvas verified. Ordinary Player typing,
      Play click, Space and Return reach the dungeon; Numpad4 moves west and
      displaces the dog. Eight measured before/after pixel checks pass; captures
      inspected. Corresponding original upstream source download exposed in the
      browser alongside the complete game's bundled port differences. Browser
      launch approved. Full progression, combat, saving, audio and post-release
      public replay remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4568
runtime:
  executable_path: NetHack/NetHack
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 37b103f7468539c92bcf86521df91a656618a3d9c3359a0e91c979323d08efd1
    size_bytes: 1083366
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.nethack.org/v343/ports/download-mac.html
    - https://github.com/benletchford/systemless/issues/4568
    license: NetHack General Public License
    rights_holder: The NetHack DevTeam and contributors
    permission: >-
      The original bundled License permits executable redistribution with
      corresponding source. Preserve the entire original archive, licence, notices and bundled
      src-diff.txt port changes; supply the unchanged corresponding upstream source
      download alongside the game.
    notes: >-
      Publisher MD5 7125425d3644ad962a330db45fa88473 matches. This original classic
      Mac OS 8.5/9 Carbon port is genuinely PPC, with no CODE resources. Eight original
      files and sixteen forks plus Finder metadata independently match production
      packing. Original Kevin Hugo port differences are included as src-diff.txt, SHA-256
      2e1540a7e6c8222a7f904cb0181b834b4894e1c21969be735eb03100ce5b7c9b.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: d8ab4557e3c548d0109656f1813daacb563f3be170738af766f19799e50bdd95
    size_bytes: 1234
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4568
    license: NetHack General Public License
    rights_holder: The NetHack DevTeam and contributors
    permission: Original game content covered by the bundled NetHack General Public License.
    notes: >-
      Actual first-dungeon map after keypad movement and dog displacement. Cropped to
      the map content, excluding window frame, clipped status line, menu bar and
      desktop. Native source c1a98415f47e04887b7afb0c4d8b302cfd5bfea4.
references:
- https://www.nethack.org/v343/ports/download-mac.html
- https://github.com/benletchford/systemless/issues/4568
---

This is the original NetHack 3.4.3 Carbon release for classic PowerPC Macs.
Create a character, then use Space and Return to acknowledge opening messages.
The bundled defaults enable the numeric keypad: keypad 4 moves west, 6 east,
8 north and 2 south.

The unchanged game download includes its licence, documentation, data and
original Macintosh port source differences. Download the corresponding upstream
source above and use it with the included src-diff.txt.

First-dungeon movement and dog displacement are verified natively and in the
browser. Broader gameplay, saving and audio remain unverified.

![NetHack first dungeon after moving west](https://assets.systemless.org/catalogue/media/sha256/d8/d8ab4557e3c548d0109656f1813daacb563f3be170738af766f19799e50bdd95.png)
