---
id: postal-demo
launch_enabled: true
kind: game
title: Postal Demo
summary: Play the Construction Site in the original Macintosh Postal demo.
developer: Running With Scissors
year: 1997
architectures:
- ppc
default_architecture: ppc
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: dcf21abe native
    architecture: ppc
    environment: >-
      Original unchanged demo at default machine, preferred partition, display,
      sound, difficulty and preferences. Continue the original notices, then choose Start,
      Single Player and New. On the Construction Site, hold Up for 60 ticks. Matched
      active/omission runs start with identical RGB frames; actual captures show the
      active player moving away from the building while the control stays at its starting
      position. Both complete 2316 frontend and guest ticks with zero exhausted frames
      and zero script assertions. Bounded level entry and movement only; not level
      completion, every weapon, multiplayer, saves, audio fidelity, independent hardware or
      browser pacing.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6186
  - date: "2026-11-13"
    tester: Catalogue maintainer
    systemless_version: fd6991fe release browser preview
    architecture: ppc
    environment: >-
      Private Chromium 156 release preview at original machine, preferred partition,
      sound, graphics, difficulty and preferences. Both active and omission cases
      continuously measure 80 seconds beginning before New, including the complete
      30-40 second loading sequence. Four consecutive 20-second sections per case
      pass the standard pacing gates and full-section checks; no startup work is
      excluded or hidden in a longer average. Across eight full sections, maximum
      runtime frame is 33.95 ms and minimum audio queue is 107.03 ms. Original
      archive, main/worker Wasm responses and actual executing-worker wire bytes
      match their pins; all targets are resumed and Debugger is disabled before
      measurement. The initial Up hold visibly moves the player while omission
      stays at the start; both cases then use the same ordinary walking and turning.
      Dynamic actors differ before input, so whole-frame pre-input identity is not
      claimed. Both remain in live play through the final 60-80 second section,
      ending with 54 and 36 health. Active recording exits with a post-measurement
      offline metadata error; its complete 4877-frame trace is separately revalidated
      against all live reports without replay. Omission recording and offline checks
      pass normally with 4869 frames. Bounded level entry and movement only; not
      complete-level, every-weapon, multiplayer, saves, audio-fidelity, independent
      hardware or public-deployment qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6188
runtime:
  executable_path: Postal™ Demo Folder/Postal™
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 6b60e5a29df6c8766acc7b86cfb4c9249d67a9f676a96938c622b069588293a8
    size_bytes: 15862823
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
      project-policy acceptance, not an affirmative publisher or artwork licence grant. No
      applicable explicit redistribution restriction was identified in the reviewed
      notices. Preserve every original file, notice and demo limit.
    notes: "Unchanged preserved StuffIt archive. Independent extraction and production packing match all 52 files, 104 forks and 52 Finder records with strict EOF. The MacAddict 17 January 1998 installer contains the same 52 unique filenames: every data/resource fork and type/creator matches, including Postal.ini. Three Finder flags differ. The installer parser misplaces sibling directory paths, so independent period directory-layout or whole-installer parity is not claimed. No file was reconstructed, replaced or removed from this archive. The full 18720-byte release notes match installer TEXT 1000 and the preserved public Readme. All eight app dialog records, 33 installer dialog/string records and both installer TEXT records were reviewed. Proprietary game bundles, AI logic and level scripts were not decoded. Modern source-release terms are not substituted for this original package's notices."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 74ff1d76d45220c9a69fbee20a9a691da6a0c260ef1d326cd83fe9b1dd1e4cfd
    size_bytes: 402651
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6186
    rights_holder: Running With Scissors and original contributors
    permission: Real content-only capture of the project-policy-accepted promotional demo.
    notes: >-
      Unscaled, unretouched 640x480 lossless crop of actual native Construction Site
      gameplay after the forward input; excludes desktop. No separate artwork licence
      grant claimed.
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

Level entry and movement have been checked in native and browser preview runs.

![The player moving across the Construction Site](https://assets.systemless.org/catalogue/media/sha256/74/74ff1d76d45220c9a69fbee20a9a691da6a0c260ef1d326cd83fe9b1dd1e4cfd.png)
