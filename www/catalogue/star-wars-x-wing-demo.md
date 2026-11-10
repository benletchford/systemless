---
id: star-wars-x-wing-demo
kind: game
title: "Star Wars: X-Wing Demo"
summary: >-
  Fly an X-Wing through a training maze in LucasArts’ original Macintosh demo,
  which also includes a troop-convoy mission.
developer: Totally Games
publisher: LucasArts
year: 1996
launch_enabled: true
architectures:
- 68k
- ppc
default_architecture: ppc
category: Simulation
compatibility:
  status: playable
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: "0.114.0"
    architecture: ppc
    environment: >-
      Released public route on human-published v0.114.0 (472a68a5). Fresh independent
      default-PPC steering and omitted-key runs verify the original hosted archive,
      both main/worker Wasm response bodies and executing current PPC worker wire
      bytes (SHA-256 83ee6b0ca0fd4bc12bfbe133852008b6846b29943d70508506d5d55510035cfb).
      Debugger disabled before timing; original CPU, display, partition, sound and
      preferences retained. Normal left-door click enters training; recentered mouse
      and Right held for 60 guest ticks shift upper-view maze geometry 47.838 CSS
      pixels left, versus 0.352 left in the omitted-key control. Each uses its own
      preceding frame and the same mask/thresholds as private approval. All four
      unchanged twenty-second entry/input and separate sustained-flight windows pass:
      endpoint guest 60.115-61.818 ticks/sec, host 61.569-63.335 FPS, maximum worker
      frame 30.1 ms, minimum audio queue 403 ms, zero observer/lifecycle errors.
      Before/after/final and all eight intermediate sustained captures inspected;
      control passes one gate and scores 100, while active steers away and remains
      in flight. Scheduling metrics do not establish game render FPS or audio fidelity.
      Combat, full missions, save/load, audio fidelity and independent hardware
      equivalence remain unqualified. This establishes one newly verified distinct
      live game without a release action.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5953
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: fb4b5071 runtime / ac28c51c browser source
    architecture: ppc
    environment: >-
      Private production-equivalent Chrome Worker browser with only launch enabled;
      original hosted archive and both actual main/worker Wasm response bodies match
      their pins. Default 25 MHz CPU, display, partition, sound and preferences remain
      intact. Normal left-door click enters training; recentered mouse and Right held
      for 60 guest ticks shift upper-view maze geometry 35.47 CSS pixels left, versus
      0.35 right in a fresh omitted-input control, each from its own preceding frame.
      Four unchanged twenty-second entry/input and separate sustained-flight windows
      pass: endpoint guest 57.527-60.004 ticks/sec, host 58.518-61.724 FPS, maximum
      worker frame 28.9 ms, minimum audio queue 404.9 ms, zero lifecycle or observer
      errors. These are scheduling metrics, not game render FPS or audio fidelity.
      Actual before/after and intermediate sustained captures inspected. Control
      naturally passes one gate and scores 100; active remains in flight after steering
      away. All targets resumed and debugger disabled before measurement. Combat,
      mission completion, saves, independent hardware fidelity and released public-route
      replay remain unverified. No public/live qualification or release action claimed.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5953
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: e0b55663 runtime code
    architecture: ppc
    environment: >-
      Unchanged original demo, default PowerPC CPU, initial display, partition, sound
      and preferences. Hovering the left concourse door identifies Fly X-Wing Training
      Mission; clicking enters actual flight. Recenter the mouse, hold Right for 60
      ticks and release for 12. Fresh active and omitted-key control each pass three
      measured pixel assertions at 936 frontend/guest ticks with zero exhausted frames.
      Pre-input PNGs are byte-identical; the training-world rectangle (200,100)-(600,280)
      differs by 6161 pixels across active/control. Actual captures inspected. This
      verifies bounded steering, not combat, mission completion, persistence, audio
      fidelity, independent hardware rendering or browser input/pacing. Native timing is not
      browser performance evidence. Frozen binary SHA-256
      22f46071915eb224a3475359bbf5fd64118fc901fffbc62bf758ec151a10d4b5.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5953
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: e0b55663 runtime code
    architecture: 68k
    environment: >-
      Original default 68K run reaches only a partial cockpit after the same
      concourse interaction, with missing instruments and world rendering. Actual capture
      inspected at 804 frontend/1404 guest ticks, zero exhausted frames. Script completion
      is not gameplay qualification for this slice.
    status: broken
    evidence: https://github.com/benletchford/systemless/issues/5953
runtime:
  executable_path: X-Wing Demo/X-Wing
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 5d050133de79669941679e4868f6482b4c415432502a664ef94e961912fb9738
    size_bytes: 5376955
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/star-wars-x-wing
    - https://archive.org/details/macaddict-cd-001
    - https://archive.org/details/macformat-038
    - https://github.com/benletchford/systemless/issues/5953
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: LucasArts / Totally Games
    permission: >-
      Accepted under the existing promotional-demo project policy explicitly
      authorized by the maintainer on 2026-11-10. Period distribution, preservation of the
      complete unchanged downloadable package and absence of an explicit redistribution
      restriction in inspected applicable notices have been checked. This records
      project-policy acceptance, not an affirmative publisher licence grant. Original notices
      remain intact.
    notes: >-
      Original 5376955-byte SIT. Independent extraction matches all 66 files, 132
      forks and 66 Finder records to production preparation; strict KPK end 10617118,
      SHA-256 773b288f0a0219d468577a54cd4d055bf740687a52bc05055a1fd3b157ab4db5. All 66
      downloaded files match MacAddict 1 (September 1996) in both forks, recovered through
      validated HTTP 206 ranges from the original HFS image. MacFormat 38 (June 1996)
      independently corroborates the program tree at catalogue level. Both discs also
      contain BRAD.PLT, a 1706-byte pilot-save file with Finder type pilt and creator
      xwng; the unchanged download does not include that saved state. It is not borrowed,
      reset or added. Entire period-package or whole-media byte identity is not
      claimed. Full main Read Me (16122 bytes), Joystick Read Me (6952 bytes), 173 selected
      UI/text/version resources and retained notice matches were reviewed. No explicit
      redistribution restriction was found. Controller files, patches and all original
      game assets remain intact. No performance tips, settings rewrites,
      CPU/display/partition/sound overrides, cheats or unlocks were applied. The game has CODE 0-6 plus
      a pwpc PEF/cfrg; only the PPC slice has bounded steering qualification.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: a43970530f0c10e77152a18fcb997e176b2a8abfd0a2980523630d616d1c18e4
    size_bytes: 97329
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5953
    rights_holder: LucasArts / Totally Games
    permission: >-
      Fresh Systemless capture from the project-policy-accepted promotional demo;
      underlying artwork retains its original copyright.
    notes: >-
      Unscaled 640x480 training cockpit and world after ordinary Right steering,
      cropped from native frame at (80,60)-(720,540). No desktop, menu bar, emulator
      framing, colour changes or artwork edits. SHA-256
      a43970530f0c10e77152a18fcb997e176b2a8abfd0a2980523630d616d1c18e4, 97329 bytes. Native evidence only.
references:
- https://classicmacdemos.com/star-wars-x-wing
- https://archive.org/details/macaddict-cd-001
- https://archive.org/details/macformat-038
- https://github.com/benletchford/systemless/issues/5953
---

![X-Wing training cockpit after steering](https://assets.systemless.org/catalogue/media/sha256/a4/a43970530f0c10e77152a18fcb997e176b2a8abfd0a2980523630d616d1c18e4.png)

Enter the left concourse door, **Fly X-Wing Training Mission**, to practise
flying through the maze. The demo also offers the **Destroy Troop Convoy**
historical mission. The original Read Me explains both missions and controls.

The default flight control uses the mouse; the cursor keys also steer.
Click to fire, **F** toggles the X-Wing’s S-foils, and **+ / -** adjust throttle.

PowerPC testing verifies training entry, steering and sustained flight in the
browser. The 68K slice has incomplete cockpit/world rendering. Combat, mission
completion, persistence and audio fidelity remain unverified.
