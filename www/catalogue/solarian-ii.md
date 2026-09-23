---
id: solarian-ii
kind: game
title: Solarian II
summary: >-
  Defend the Solar System in Ben Haller's colourful classic Macintosh shareware
  shooter.
developer: Ben Haller
publisher: Stick Software
year: 1989
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.54.0"
    architecture: 68k
    environment: >-
      Deterministic headless run of the unchanged, developer-hosted Solarian II 1.0.4
      archive. The first-run dialog, shareware title, main menu, and live first-level
      gameplay render. Browser interaction awaits manual approval.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2510
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 677042e0c8a993b3bb2c4ef4095fbf719ef5832213a7a307ca2563ec2e620ef1
    size_bytes: 464042
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.sticksoftware.com/archive.html
    - https://www.sticksoftware.com/archive/Solarian.html
    rights_holder: Ben Haller / Stick Software
    permission: >-
      The developer still hosts this original, unregistered Classic Mac 1.0.4
      archive. Its shareware title screen explicitly says to distribute the game freely, while
      its first-run dialog warns not to distribute registered copies. This is the
      unchanged developer-hosted download, not a registered copy or the later Mac OS X port.
    notes: >-
      Original 464,042-byte StuffIt archive, SHA-256
      677042e0c8a993b3bb2c4ef4095fbf719ef5832213a7a307ca2563ec2e620ef1.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 69083a7fbac1544e909389bf0c27cf2b4bc495169e323e0cb213beffce69fcf7
    size_bytes: 9581
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2510
    permission: >-
      Fresh Systemless capture of live gameplay in the unregistered shareware game.
      The underlying game artwork remains the property of its rights holder.
    notes: >-
      Cropped from the live 800-by-600 framebuffer to the 640-by-480 game content.
      PNG SHA-256 69083a7fbac1544e909389bf0c27cf2b4bc495169e323e0cb213beffce69fcf7, 9,581
      bytes.
references:
- https://www.sticksoftware.com/archive.html
- https://www.sticksoftware.com/archive/Solarian.html
---

## Classic colour arcade action

![Solarian II first-level gameplay](https://assets.systemless.org/catalogue/media/sha256/69/69083a7fbac1544e909389bf0c27cf2b4bc495169e323e0cb213beffce69fcf7.png)

Solarian II is Ben Haller's early colour Macintosh shooter. This entry uses the
original 1.0.4 shareware archive still supplied by its developer, not the later
Mac OS X port. The unregistered game invites free distribution and asks players
who keep playing to pay its shareware fee.

Systemless reaches live first-level gameplay. Browser launch remains disabled
until its controls have been manually verified there.
