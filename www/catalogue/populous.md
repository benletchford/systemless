---
id: populous
kind: game
title: Populous
summary: >-
  Shape the land and lead your followers in Bullfrog's original two-world
  Macintosh demo.
developer: Bullfrog Productions
publisher: Electronic Arts
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.57.0 + local site build"
    architecture: 68k
    environment: >-
      Release-mode browser launch from the immutable hosted two-world demo at
      localhost:8080; selected Run Slow at the colour prompt, Conquest and Start,
      then changed the first-world viewport through the overview map.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2648
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: 0.52.0-dev
    architecture: 68k
    environment: >-
      Deterministic headless play of the unchanged Macintosh demo archive in an
      800-by-600, 256-colour display. Selected Run Slow at the colour-depth prompt, then
      Conquest and Start; the first world's terrain, controls, and units rendered.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2474
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 845e53d0b506d0c2c930277ab5095bfd78ec72bc5a7ab9ddb5d330a10653d399
    size_bytes: 530490
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/populous
    - https://gardenmirror.oldapplestuff.com/manuals/Populous_ReadMe_Demo.pdf
    - https://www.ea.com/games/populous/populous
    rights_holder: Electronic Arts
    permission: >-
      This is the unchanged publisher-branded promotional demo, not the retail game.
      Its bundled 1993 readme distinguishes the demo's two worlds and conquest-only
      play from the full version and directs readers to Electronic Arts for further
      information. It contains no express redistribution clause; the basis for hosting is the
      original public demo distribution, not a grant for the full game.
    notes: >-
      Original 530,490-byte StuffIt archive, SHA-256
      845e53d0b506d0c2c930277ab5095bfd78ec72bc5a7ab9ddb5d330a10653d399. Its MD5, 7d75b5095cc08b8ee81146db9f60c080,
      matches Macintosh Garden's PopulousDemo.sit listing. Electronic Arts still sells
      Populous; no retail files are included.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 929e2085b6280045fc612889688cf9d35371a07bd6506949e4f53ebe0fb142c4
    size_bytes: 221443
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2474
    permission: >-
      Fresh gameplay capture made for this catalogue entry. Underlying Populous
      artwork remains the property of its rights holder.
    notes: >-
      Deterministic capture from the unchanged demo after starting its first conquest
      world. The 512-by-342 game surface was captured directly from the framebuffer at
      (144,139), excluding the Mac menu bar, desktop, and window chrome without
      altering game pixels. PNG SHA-256
      929e2085b6280045fc612889688cf9d35371a07bd6506949e4f53ebe0fb142c4, 221,443 bytes.
references:
- https://macintoshgarden.org/games/populous
- https://gardenmirror.oldapplestuff.com/manuals/Populous_ReadMe_Demo.pdf
- https://www.ea.com/games/populous/populous
---

## Shape a world

![Populous demo conquest map](https://assets.systemless.org/catalogue/media/sha256/92/929e2085b6280045fc612889688cf9d35371a07bd6506949e4f53ebe0fb142c4.png)

In *Populous*, the shape of the land decides where your followers can settle.
Flatten ground to help them build, expand their population, and gather the power
to reshape the world against a rival deity. The isometric map mixes a close-up
view with an overview and controls for terrain, units, and disasters.

## The Macintosh demo

This is the original two-world Macintosh demonstration, not the complete
commercial release. Its bundled readme says the demo offers conquest mode but
not the full game's custom mode. Electronic Arts [continues to sell the full
game](https://www.ea.com/games/populous/populous); it is not hosted here.
