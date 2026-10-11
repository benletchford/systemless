---
id: nethack
kind: game
title: NetHack
summary: Explore a changing dungeon with a companion and recover the Amulet of Yendor.
developer: The NetHack DevTeam
year: 2003
architectures: [ppc]
default_architecture: ppc
category: Role-Playing
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: c1a98415f47e04887b7afb0c4d8b302cfd5bfea4
    architecture: ppc
    environment: >-
      Unchanged publisher 3.4.3 Carbon archive, NetHack executable, PPC/depth 8.
      Ordinary Player character creation, Space and Return reach the dungeon.
      Keypad 4 moves the player and displaces the dog. Eight measured pixel
      assertions pass at 1532 frontend/guest ticks with zero exhaustion; captures
      inspected. Production packing matches eight original files, sixteen forks
      and supported Finder metadata. Browser, broader gameplay, saving and audio
      remain unverified. The earlier 3.3.1 68K edition remains unqualified.
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
    type: url
    url: https://www.nethack.org/download/3.4.3/nethack-343-mac-carbon.sit
    expected_sha256: 37b103f7468539c92bcf86521df91a656618a3d9c3359a0e91c979323d08efd1
    expected_size: 1083366
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
      corresponding source. Preserve the entire original archive, licence,
      notices and bundled src-diff.txt port changes; supply the unchanged
      corresponding upstream source download alongside the game.
    notes: >-
      Publisher MD5 7125425d3644ad962a330db45fa88473 matches. This original
      classic Mac OS 8.5/9 Carbon port is genuinely PPC, with no CODE resources.
      Eight original files and sixteen forks plus Finder metadata independently
      match production packing. Original Kevin Hugo port differences are included
      as src-diff.txt, SHA-256
      2e1540a7e6c8222a7f904cb0181b834b4894e1c21969be735eb03100ce5b7c9b.
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

First-dungeon movement and dog displacement are verified natively. Browser
qualification, broader gameplay, saving and audio remain pending.
