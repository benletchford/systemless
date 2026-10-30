---
id: macpaint
kind: application
title: MacPaint 1.3
summary: Draw and edit monochrome pictures with Apple's classic painting application.
developer: Bill Atkinson
publisher: Apple Computer
year: 1984
architectures:
- 68k
default_architecture: 68k
category: Art
launch_enabled: true
compatibility:
  status: boots
  verified:
  - date: "2026-10-28"
    tester: Issue reporter
    systemless_version: 16f24e9310cab886ce1088dc7c02d0831a829c82
    architecture: 68k
    environment: >-
      Native headless runner, original Gryphel gry2 disk image, 1-bit screen and
      24-bit addressing. Drawing and text work but brush, bucket, selection, dialogs,
      FatBits and saving have defects; see the linked report.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/5211
runtime:
  addressing_24_bit: true
  screen_depth: 1
  screen_size:
  - 512
  - 342
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: sha256
    sha256: 1ce6185ce816c42aa8a4d15dff87335c5e09491ce333dd34a5acd07c57950926
    size_bytes: 117158
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/c/sw/general/macpaint/
    - https://github.com/computerhistory/Historical-Source-Code-MacPaint-Repository
    rights_holder: Apple Inc.
    permission: >-
      Gryphel describes the build and source as available only for non-commercial
      use. Preserve the unchanged archive and its accompanying terms.
    notes: >-
      Gryphel's 2014 gry2 build adapts the donated source to MPW Pascal and fixes HFS
      clipboard operations. The ZIP contains an 800K HFS image and checksum.
- id: screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c7b39d03b3f306e4c243c8c49f0bbfccfcbb3025af007afc4b8c5a0aec3c2cfd
    size_bytes: 11413
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://www.gryphel.com/c/sw/general/macpaint/
    permission: >-
      Captured from the non-commercial-use build; the application artwork remains the
      property of its rights holder.
    notes: >-
      Systemless capture of drawing and Copy/Paste using the verified gry2 disk
      image, cropped to remove the Classic Mac menu bar.
references:
- https://www.gryphel.com/c/sw/general/macpaint/
- https://github.com/benletchford/systemless/issues/5211
- https://github.com/benletchford/systemless/issues/5212
---

MacPaint is Bill Atkinson's monochrome painting program for the Macintosh. This
modified 1.3 build was compiled by the Gryphel Project from the source Apple
donated to the Computer History Museum. The download is available for
non-commercial use only.

![MacPaint drawing and copied selection](https://assets.systemless.org/catalogue/media/sha256/c7/c7b39d03b3f306e4c243c8c49f0bbfccfcbb3025af007afc4b8c5a0aec3c2cfd.png)

MacPaint requires 24-bit addressing and a compact monochrome display.
