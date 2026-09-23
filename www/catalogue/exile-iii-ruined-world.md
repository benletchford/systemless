---
id: exile-iii-ruined-world
kind: game
title: "Exile III: Ruined World"
summary: >-
  Return to the surface in Spiderweb Software's original Macintosh fantasy
  role-playing demo, where a mysterious disaster threatens the world.
developer: Jeff Vogel
publisher: Spiderweb Software
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.55.0"
    architecture: 68k
    environment: >-
      Release-mode browser run of the promoted unregistered demo through the
      welcome dialog, new-game and party creation, and Guest Quarters gameplay;
      welcome text rendering briefly stalls before recovering
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/2547
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 4c8666f28854ffa917bc3a529f835b8cba8781c0511a276fdb8ae023d4b95054
    size_bytes: 3197312
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.spiderwebsoftware.com/exile3/macexile3.html
    - https://www.spiderwebsoftware.com/ftp/mac/exile3.v103.sit
    license: Spiderweb Software License included in the demo archive
    rights_holder: Spiderweb Software, Inc.
    permission: >-
      The bundled Software License permits non-profit distribution without prior
      written notice if the complete software is unmodified. This is the unchanged
      owner-hosted demo with its documentation and license intact, not the separately offered
      fully registered installer or an unlocked copy.
    notes: >-
      Original Exile III StuffIt demo, 3,197,312 bytes, SHA-256
      4c8666f28854ffa917bc3a529f835b8cba8781c0511a276fdb8ae023d4b95054. The archive folder is named v1.0.3
      while its game screen displays v1.0.3b. Spiderweb's current page says the game is
      free to play and offers free unlocking keys through its support team; no key is
      distributed here.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 3899ac002dd9654d2b2fdd02b1c2d9b25fbf1bde5198ba7e7680d05a1564d4b7
    size_bytes: 405188
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2542
    permission: >-
      Fresh Systemless capture from the unregistered demo. The underlying game
      artwork remains Spiderweb Software's property.
    notes: >-
      An 800-by-600 guest-framebuffer capture of the opening Guest Quarters game
      scene, showing the party, map, inventory, and controls without host desktop, browser,
      or emulator controls. PNG SHA-256
      3899ac002dd9654d2b2fdd02b1c2d9b25fbf1bde5198ba7e7680d05a1564d4b7; 405,188 bytes.
references:
- https://www.spiderwebsoftware.com/exile3/macexile3.html
- https://github.com/benletchford/systemless/issues/2542
- https://github.com/benletchford/systemless/issues/2566
---

## Back to the surface

![The Exile III demo in the Guest Quarters](https://assets.systemless.org/catalogue/media/sha256/38/3899ac002dd9654d2b2fdd02b1c2d9b25fbf1bde5198ba7e7680d05a1564d4b7.png)

After surviving beneath the Empire for years, the people of Exile can finally
seek a return to the surface. But the land above is being devastated by an
unknown force. This entry preserves Spiderweb Software's original Macintosh
demo, not an unlocked or modified edition.

The bundled license allows non-profit sharing of the complete, unchanged demo.
Spiderweb now describes Exile III as free to play and offers free unlocking keys
through its support team; this archive contains no key. The original license
still accompanies the download and sets the terms for unregistered use.

Systemless reaches party creation and the opening Guest Quarters game scene in
a deterministic run. The same path and scene were verified in the release-mode
browser build using this promoted archive. The welcome dialog can briefly slow
the browser while its text appears, then normal speed resumes; this is tracked
in [the runtime performance issue](https://github.com/benletchford/systemless/issues/2566).
