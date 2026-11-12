---
id: dr-seuss-abc-demo
kind: game
title: Dr. Seuss's ABC Demo
summary: Explore animated alphabet pages with Aunt Annie, her alligator and other Dr. Seuss
  characters in the original Macintosh demo.
developer: Living Books
publisher: Living Books
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: '2026-11-12'
    tester: Catalogue maintainer
    systemless_version: dcf21abe native
    architecture: 68k
    environment: Original unchanged demo at default machine, preferred partition, display,
      sound and preferences. Enter Let Me Play and wait through the A-page narration, then
      click the visible alligator. Paired active/omission runs have identical pre-input RGB
      frames. Reaction frames differ by 4748 pixels and final frames by 20015 pixels, including
      15047 in the alligator/Annie region; actual captures show the clicked alligator and
      Aunt Annie moving differently from the control. Both runs complete 4161 frontend / 4761
      guest ticks with zero exhausted frames and zero script assertions. Actual images and
      external RGB comparisons inspected. Bounded interactive page behavior only; not every
      letter, the complete retail book, saves, audio fidelity, independent hardware or browser
      pacing.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6182
runtime:
  executable_path: Living Books/Dr. Seuss’s  ABC
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Dr%20Seuss%20ABC.sit
    expected_sha256: 2fb5cab02f6d8b26c474ec5f89af329f8b3c7807f3d69cf20abde42667be0814
    expected_size: 13398743
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/living-books-dr-seuss-abc
    - https://archive.org/details/cd-rom-today-15
    - https://coverdiscs.com/disc/cd-rom-today-15
    - https://github.com/benletchford/systemless/issues/6182
    license: Maintainer-accepted promotional demo
    rights_holder: Living Books, Random House, Brøderbund, Dr. Seuss rights holders and original
      contributors
    permission: Accepted under the existing project policy for complete unchanged original
      promotional demos preserved from period cover-disc distribution. This records project-policy
      acceptance, not an affirmative publisher or artwork licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed notices. Preserve all original
      files, credits, notices and demo limits.
    notes: Complete original Living Books demo folder from CD-ROM Today 15, September 1995.
      The preserved disc image matches its published SHA-1. Independent HFS comparison confirms
      all 16 files, every data/resource fork and every Finder record match this unchanged
      mirror archive exactly, including BookOutline and both Icon files. Independent production
      packing matches 16 files, 32 forks and 16 Finder records with strict EOF. All 28 standard
      version/string/dialog records were read; no standalone Readme is present in the folder.
      BookOutline and proprietary book-page files were not decoded. No original files or preferences
      were changed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/dr-seuss-abc-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6182
    rights_holder: Living Books, Random House, Brøderbund, Dr. Seuss rights holders and original
      contributors
    permission: Real content-only capture of the project-policy-accepted original promotional
      demo.
    notes: Unscaled, unretouched 512x384 lossless crop of the actual native A page after the
      alligator click; excludes desktop. No separate artwork licence claimed.
references:
- https://classicmacdemos.com/living-books-dr-seuss-abc
- https://archive.org/details/cd-rom-today-15
- https://github.com/benletchford/systemless/issues/6182
---

Explore a selection of alphabet pages in the original **Dr. Seuss’s ABC** Macintosh demo.

Choose **Let Me Play**, wait through the page narration, then click the characters and objects. On the A page, click **Aunt Annie’s alligator** to trigger its animation. This is a promotional selection of pages, not the complete retail book.

Native A-page interaction is verified. Browser qualification remains pending.

![Aunt Annie riding her alligator on the A page](incoming/dr-seuss-abc-demo/gameplay.png)
