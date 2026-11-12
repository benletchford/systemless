---
id: sky-shadow-demo
kind: game
title: Sky Shadow Sample
summary: Fly above an alien landscape in the original first-zone Macintosh sample.
developer: Patrick Buckland
publisher: Casady & Greene
year: 1990
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: 0a0c6a31 native
    architecture: 68k
    environment: >-
      Complete unchanged original sample at default machine settings. Normal Return
      opens instructions and Space starts the first zone. Mouse movement to 450,330
      followed by eight ticks moves the cyan ship down 28 pixels against a matched
      omitted-input control. Starting RGB frames are identical; both runs finish 864 frontend
      and 1464 guest ticks with zero exhausted frames. External image assertions verify
      the ship masks and movement; scripts contain zero assertions. Actual captures
      inspected. Instructions are incompletely rendered and the short active capture omits
      the HUD; these visual limitations remain unresolved. A larger movement causes a
      crash while the control remains airborne. No game-file, CPU, memory or display
      override. Browser input/pacing, later gameplay, audio fidelity and independent
      hardware accuracy remain unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/6132
runtime:
  executable_path: Sky Shadow/Sky Shadow™ Sample
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 172c940a31a4c3a2dde8f72be15d80b491843cd7459ccfabaf1a9c1935d8b8c6
    size_bytes: 525578
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/sky-shadow
    - https://archive.org/details/mac-demo-games
    - https://archive.org/download/mac-demo-games/MacDemoGames.img
    - https://github.com/benletchford/systemless/issues/6132
    license: Maintainer-accepted promotional demo
    rights_holder: Casady & Greene, Patrick Buckland and original contributors
    permission: >-
      Accepted under the existing project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance, not an
      affirmative publisher licence grant. No applicable explicit redistribution restriction
      was identified in the reviewed notices. Preserve all four files, credits, notices
      and original first-zone sample restrictions.
    notes: >-
      The preserved Apple Macintosh Demo Games CD, 1992, contains this sample under
      More Information/Sky Shadow. All four files match in data content and every
      resource payload, name, ID and attribute; raw resource maps differ. Whole archive
      wrapper identity is not claimed. The complete 64-byte ReadMe and 31 readable selected
      standard text/UI/version resources were reviewed, with zero compressed unread
      records. Original title, incomplete instructions and first-zone screens were
      inspected. PICT artwork was not decoded for text. Production packing matches all four
      files, eight forks and four Finder records against independent unar. No opaque game
      data, scripts or guest instructions were decoded.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 38456d2ca120ec1c692a6908d6530db2d2a9288b735820674b7e5e1aa2d278e7
    size_bytes: 14980
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6132
    rights_holder: Casady & Greene, Patrick Buckland and original contributors
    permission: Fresh content-only capture of the project-policy-accepted original demo.
    notes: >-
      Unscaled 528x358 crop of actual native first-zone gameplay and HUD from the
      complete unchanged archive. Excludes the outer black desktop and host framing. No
      original pixels were retouched; no broader artwork licence is claimed.
references:
- https://classicmacdemos.com/sky-shadow
- https://archive.org/details/mac-demo-games
- https://github.com/benletchford/systemless/issues/6132
---

Fly across the first zone in this original **Sky Shadow** sample, version **1.1.6s**.
The complete package retains its original sounds and first-zone data.

Press **Space** at the title to start. The mouse steers the craft. Native first-zone
startup and a bounded mouse response are verified; instruction and HUD rendering
have known limitations. Browser play remains pending.

![Sky Shadow first-zone gameplay](https://assets.systemless.org/catalogue/media/sha256/38/38456d2ca120ec1c692a6908d6530db2d2a9288b735820674b7e5e1aa2d278e7.png)
