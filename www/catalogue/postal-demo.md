---
id: postal-demo
kind: game
launch_enabled: false
title: Postal Demo
summary: Play the Construction Site in the original Macintosh Postal demo.
developer: Running With Scissors
year: 1997
architectures:
- ppc
default_architecture: ppc
category: Arcade
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: dcf21abe native
    architecture: ppc
    environment: >-
      Original unchanged demo at default machine, preferred partition, display,
      sound, difficulty and preferences. Continue the original notices, then choose
      Start, Single Player and New. On the Construction Site, hold Up for 60 ticks.
      Matched active/omission runs start with identical RGB frames; actual captures
      show the active player moving away from the building while the control stays
      at its starting position. Both complete 2316 frontend and guest ticks with
      zero exhausted frames and zero script assertions. Bounded level entry and
      movement only; not level completion, every weapon, multiplayer, saves, audio
      fidelity, independent hardware or browser pacing.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6186
runtime:
  executable_path: Postal™ Demo Folder/Postal™
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Postal%20Demo.sit
    expected_sha256: 6b60e5a29df6c8766acc7b86cfb4c9249d67a9f676a96938c622b069588293a8
    expected_size: 15862823
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/postal
    - https://archive.org/details/macaddict-cd-017
    - https://coverdiscs.com/disc/macaddict-issue-017
    - https://github.com/benletchford/systemless/issues/6186
    license: Maintainer-accepted promotional demo
    rights_holder: Running With Scissors and original contributors
    permission: >-
      Accepted under the existing project policy for complete unchanged original
      promotional demos preserved from period cover-disc distribution. This records
      project-policy acceptance, not an affirmative publisher or artwork licence
      grant. No applicable explicit redistribution restriction was identified in
      the reviewed notices. Preserve every original file, notice and demo limit.
    notes: >-
      Unchanged preserved StuffIt archive. Independent extraction and production
      packing match all 52 files, 104 forks and 52 Finder records with strict EOF.
      The MacAddict 17 January 1998 installer contains the same 52 unique filenames:
      every data/resource fork and type/creator matches, including Postal.ini.
      Three Finder flags differ. The installer parser misplaces sibling directory
      paths, so independent period directory-layout or whole-installer parity is
      not claimed. No file was reconstructed, replaced or removed from this archive.
      The full 18720-byte release notes match installer TEXT 1000 and the preserved
      public Readme. All eight app dialog records, 33 installer dialog/string
      records and both installer TEXT records were reviewed. Proprietary game
      bundles, AI logic and level scripts were not decoded. Modern source-release
      terms are not substituted for this original package's notices.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/postal-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6186
    rights_holder: Running With Scissors and original contributors
    permission: Real content-only capture of the project-policy-accepted promotional demo.
    notes: >-
      Unscaled, unretouched 640x480 lossless crop of actual native Construction
      Site gameplay after the forward input; excludes desktop. No separate artwork
      licence grant claimed.
references:
- https://classicmacdemos.com/postal
- https://archive.org/details/macaddict-cd-017
- https://github.com/benletchford/systemless/issues/6186
---

Play the Construction Site in the original Macintosh **Postal** promotional demo.
The package includes one single-player level with three difficulty layouts, one
multiplayer level and one challenge level.

Press **Return** to continue the opening screens, then choose **Start → Single
Player → New**. Use the arrow keys to move and turn. The bundled release notes
describe the other controls and game modes.

Native level entry and movement are verified. Browser qualification remains pending.

![The player moving across the Construction Site](incoming/postal-demo/gameplay.png)
