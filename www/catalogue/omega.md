---
id: omega
kind: game
title: Omega
summary: >-
  Explore procedural wilderness, navigate the sprawling City of Rampart, and
  climb guild ranks in Laurence Brothers and Sheldon Simms's graphical roguelike.
developer: Laurence R. Brothers & Sheldon Simms
publisher: Sheldon Simms
year: 1999
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-19
    tester: Catalogue maintainer
    systemless_version: 0.41.13
    architecture: 68k
    environment: Deterministic headless run from the Info-Mac mirror distribution archive
    status: playable
    evidence: https://github.com/benletchford/systemless.org/issues/313
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 4283c82520fda5479f9ee392889adde24aaef6dc6575f54ee972ce72164198ca
    size_bytes: 433089
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/omega-0802.hqx
    license: Freeware
    rights_holder: Laurence R. Brothers & Sheldon Simms
    permission: "Omega is distributed under the Omega General Public License: \"Anyone may make a copy of omega, and distribute the copy, so long as this license remains accessible (via the 'P' command). Distribution must be free of charge (except, possibly, for the physical medium).\" The author specifically waives all rights to compensation for non-commercial distribution, and the port was submitted to Info-Mac without commercial or CD-ROM exclusions."
    notes: >-
      Untouched Info-Mac BinHex distribution archive retrieved from
      mirrors.nic.funet.fi. The 433,089-byte file has SHA-256
      4283c82520fda5479f9ee392889adde24aaef6dc6575f54ee972ce72164198ca. Original game created by Laurence R. Brothers in
      1987–1989; enhanced for Macintosh with color graphical tiles and native windowing by
      Sheldon Simms in 1999.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 6ac3c6e48babafc4c330a97b262b80101e3ba0e663f9adc03bd86c84df6a10f7
    size_bytes: 16288
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/313
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged distribution archive on
      2026-09-19, cropped exactly to the game's 640-by-432 content surface. It excludes the
      Classic Mac menu bar, browser, website, host desktop and emulator chrome.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/omega-0802.hqx
---

## Procedural open-world exploration and city life

![Omega](https://assets.systemless.org/catalogue/media/sha256/6a/6ac3c6e48babafc4c330a97b262b80101e3ba0e663f9adc03bd86c84df6a10f7.png)

Created by Laurence R. Brothers in the late 1980s and enhanced for the Macintosh by Sheldon Simms in 1999, *Omega* is a landmark roguelike that radically broadened the scope of traditional dungeon crawlers. While contemporaries confined gameplay to vertical underground labyrinths, *Omega* introduced a vast overland wilderness, dynamic town ecologies, and complex factional allegiances.

Players begin their journey outside the fortified walls of Rampart, a bustling metropolis featuring cobblestone streets, guarded watchtowers, local taverns, banks, temples, and competing trade guilds. Surviving and thriving requires more than combat prowess: adventurers must manage funds across banking accounts, tithe at patron shrines, acquire specialized permits, and navigate political rivalries between lawful authorities and underground syndicates.

## Graphical tile display and Macintosh enhancements

Simms's Macintosh release elevates the classic terminal experience by replacing raw ASCII glyphs with a complete set of 8-by-12 graphical color tiles. City gates, stonework walls, arched doorways, tranquil water pools, and character avatars are rendered in crisp 256-color detail within a dedicated Mac OS document window.

Key features include:
- **Open-Ended Role-Playing:** Join the Mercenaries Guild, Thieves Guild, Collegium Magii, or religious orders, taking on unique promotion rites and guild-specific quests.
- **Deep Combat and Magic:** Tactical turn-based encounters allow granular aiming, melee maneuvers, spellcasting, item consumption, and environmental tactics.
- **Procedural Overworld:** Venture beyond Rampart into enchanted forests, volcanic peaks, sunken temples, and forgotten ruins scattered across a procedurally populated kingdom.

## Emulation and execution in Systemless

Systemless executes *Omega* via Motorola 68k emulation directly in web browsers. The runtime faithfully provides the required Classic Mac OS System 7 QuickDraw environments, Color QuickDraw window rendering, resource fork decompression, and low-latency keyboard input handling, delivering an authentic 1990s roguelike experience with modern accessibility.
