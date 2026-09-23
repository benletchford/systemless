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
compatibility:
  status: boots
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.51.0-dev"
    architecture: ppc
    environment: >-
      Deterministic PowerPC launch of the unchanged demo in a local 0.51.0
      development build at 800 by 600. The 256-color and memory-partition
      prompts render, but dismissing the latter halts execution before the
      flight interface. Current published-runtime behavior is not yet verified.
    status: boots
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
references:
- https://classicmacdemos.com/flight-unlimited
- https://github.com/benletchford/systemless/issues/2573
---

## A sample flight

Looking Glass Technologies' official Macintosh demo provides one plane and one
location from its full flight simulator. It retains the original readme and
control chart in the same archive as the application and flight data.

The PowerPC demo reaches its display and memory startup prompts in Systemless.
Selecting "Switch to 256 Colors" and then "No, Thanks" halts before the flight
interface. Browser launch remains disabled while the startup failure is
investigated.
