---
id: fokker-triplane-demo
kind: game
title: Fokker Triplane Demo
summary: Fly a First World War triplane in Donald Hill's flight simulator.
developer: Donald A. Hill Jr.
publisher: Alliance Interactive Software USA
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Simulation
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 23601d26 + local catalogue preview
    architecture: 68k
    environment: >-
      Release-mode Chrome preview of the unchanged StuffIt demo. Accepted the
      advisory, default pilot name, and flight instructions, then reached the cockpit. Browser
      mouse movement banked the horizon; clicking fired and advanced the score from
      100 to 110. One exact archive fetch. A five-second active sample measured 60.2 host
      FPS and 60.2 guest ticks per second, maximum measured frame 17.4 ms, and minimum
      audio queue 130 ms.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3622
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo, accepted startup dialogs, and reached the
      cockpit. Mouse input and a shot changed the cockpit scene and advanced the score
      from 100 to 150.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3622
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 5ce2086af1b3a7865830b78548ec8afe5b725e65f50ba7d0a4eec514736e7378
    size_bytes: 325203
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/fokker-triplane
    rights_holder: Donald A. Hill Jr.
    permission: >-
      This unchanged archive contains the purpose-built Fokker Triplane Demo v2.89
      and its sound data, not a retail game. The application's own title identifies it as
      a demo and credits Donald A. Hill Jr.; the in-game instructions credit Alliance
      Interactive Software USA. No additional redistribution restriction was found in
      the bundled material.
    notes: >-
      Unchanged 325,203-byte StuffIt archive, SHA-256
      5ce2086af1b3a7865830b78548ec8afe5b725e65f50ba7d0a4eec514736e7378.
      The promoted public asset was fetched back and matched this hash.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 21bc328d294888ca79772b257453a23d792f697d3528eacc56218524cacb9e53
    size_bytes: 11983
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3622
    permission: >-
      Fresh gameplay capture made from the original demonstration archive for this
      catalogue entry. The underlying game artwork remains its owners' property.
    notes: >-
      511-by-301 direct Chrome capture of the active cockpit after mouse input and
      firing, without website framing or Mac desktop. PNG SHA-256
      21bc328d294888ca79772b257453a23d792f697d3528eacc56218524cacb9e53, 11,983 bytes.
      The promoted public asset was fetched back and matched this hash.
references:
- https://classicmacdemos.com/fokker-triplane
---

## Fly the triplane

![Fokker Triplane cockpit](https://assets.systemless.org/catalogue/media/sha256/21/21bc328d294888ca79772b257453a23d792f697d3528eacc56218524cacb9e53.png)

Accept the startup dialogs to enter flight. Move the mouse to steer and click
to fire. The cockpit instruments and score respond as you play. The original
demo limits the flight to about five minutes of fuel.
