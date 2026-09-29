---
id: star-wars-droidworks
kind: game
title: Star Wars DroidWorks Demo
summary: >-
  Build and test droids in Lucas Learning's original Power Macintosh
  demonstration.
developer: Lucas Learning Ltd.
publisher: Lucas Learning Ltd.
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Puzzle
launch_enabled: false
compatibility:
  status: boots
  verified:
  - date: "2026-09-29"
    tester: Catalogue maintainer
    systemless_version: 583723e6c2ad8529594edc3caa8306144ea637cf
    architecture: ppc
    environment: >-
      Deterministic native replay of the unchanged Macintosh demo reaches the
      full-screen introductory sequence and demo-information dialog. Gameplay and browser
      launch have not been verified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/3249
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 14715ed438a20efab76fd8c3f0b776d771f072c1bb97939577b8124168fba2c5
    size_bytes: 26406088
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/star-wars-droidworks
    - https://download.classicmacdemos.com/Star%20Wars%20DroidWorks.sit
    license: Lucas Learning DroidWorks demo license
    rights_holder: Lucas Learning Ltd.
    permission: >-
      The bundled License.txt permits electronic sharing of the demo in its original
      form without charging or receiving consideration. This archive is unchanged and
      includes the original license and game files.
    notes: >-
      Original 26,406,088-byte StuffIt archive, SHA-256
      14715ed438a20efab76fd8c3f0b776d771f072c1bb97939577b8124168fba2c5. The September 30, 1998 Read Me identifies
      Demo Version 1.0. Its game application has PowerPC PEF code and no 68K CODE game
      resources.
references:
- https://classicmacdemos.com/star-wars-droidworks
- https://github.com/benletchford/systemless/issues/3249
---

## Build a droid

Lucas Learning's original demo lets players assemble a droid from parts in the
Jawa workshop and try it in a mission. The bundled Read Me describes wheeled,
legged and tread designs, along with painting and testing a creation. This is
the complete, unchanged Power Macintosh demo package.
