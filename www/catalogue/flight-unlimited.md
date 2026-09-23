---
id: flight-unlimited
kind: game
title: Flight Unlimited
summary: >-
  Take a one-plane, one-location test flight in Looking Glass Technologies' Power
  Macintosh flight-simulator demo.
developer: Looking Glass Technologies
publisher: Looking Glass Technologies
year: 1995
architectures:
- ppc
default_architecture: ppc
category: Simulation
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: 0.55.0-dev
    architecture: ppc
    environment: >-
      Deterministic PowerPC play of the unchanged demo in a local 0.55.0 development
      build at 800 by 600. After the display and memory prompts, the intro tour, pilot
      log, FBO, and Pitts Special selection, Start on Taxiway reaches the live cockpit
      and landscape at tick 7,505 without unsupported imports. The same route and
      throttle input were also tested in a local browser preview against the unchanged
      archive.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2573
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 3f6ba4e4073b6ad32fa8b65a7414bf5d40765546a12875b85cc3161e7b691cb5
    size_bytes: 8330584
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/flight-unlimited
    - https://static.classicmacdemos.com/demos/flight-unlimited/README.txt
    rights_holder: Looking Glass Technologies and successors
    permission: >-
      This is the unchanged, intentionally limited promotional demo issued by Looking
      Glass Technologies. Its bundled April 2, 1996 readme identifies it as a demo,
      limits it to one plane and one location, and invites purchase of the retail
      version. No retail CD or registration data is included; the package does not state a
      broader redistribution licence.
    notes: >-
      Original 8,330,584-byte StuffIt archive, SHA-256
      3f6ba4e4073b6ad32fa8b65a7414bf5d40765546a12875b85cc3161e7b691cb5. The archive contains the PowerPC application,
      demo data, key chart, and the publisher's readme.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b30df3449efe0f4d551815b6299fe938922b404065173e7778917946a699eaf0
    size_bytes: 110022
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2573
    permission: >-
      Original gameplay capture made for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Deterministic capture from the exact unchanged demo archive after choosing
      Start on Taxiway, cropped from its 800-by-600 framebuffer to the 640-by-480 game
      content surface without altering game pixels. PNG SHA-256
      b30df3449efe0f4d551815b6299fe938922b404065173e7778917946a699eaf0, 110,022 bytes.
references:
- https://classicmacdemos.com/flight-unlimited
- https://github.com/benletchford/systemless/issues/2573
---

## A sample flight

![Flight Unlimited Pitts Special cockpit](https://assets.systemless.org/catalogue/media/sha256/b3/b30df3449efe0f4d551815b6299fe938922b404065173e7778917946a699eaf0.png)

Looking Glass Technologies' official Macintosh demo provides one plane and one
location from its full flight simulator. It retains the original readme and
control chart in the same archive as the application and flight data.

Choose "Switch to 256 Colors" and then "No, Thanks" at startup. Leave the
introductory tour with Space, enter a pilot name and details, and navigate the
FBO with Tab to "Fly the Pitts." Dismiss the plane-selection help, then choose
"Start on Taxiway" to enter the cockpit. The demo deliberately disables the hoop
course and lesson options; neither requires a retail copy to take the permitted
flight.
