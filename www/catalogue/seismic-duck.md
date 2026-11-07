---
id: seismic-duck
kind: game
title: Seismic Duck
summary: >-
  Survey underground rock with sound waves, drill for oil and keep your
  prospecting business solvent.
developer: Arch D. Robison
year: 2001
architectures:
- ppc
default_architecture: ppc
category: Simulation
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 9dde754438eddce4d35f0bfc0a02a1d92b088483 native; runtime merged in b912638f
    architecture: ppc
    environment: >-
      Complete unchanged original, exact executable, PowerPC/default 25 MHz,
      800x600/default 16-bit color as recommended by the original manual. Original 15fps
      simulation throttle unchanged. File Begin Game hides the underground views and starts
      the cash/time meters. Hold Right for 270 ticks to move the platform from384 to564,
      then Down for300 ticks to drill into the anticline visible before Begin Game.
      Cash200 to173 after drilling to206 after180 further ticks; green oil meter active.
      Equal-clock control omits only Down and retains200 with no bore or oil. Both
      repeats pass seven measured causal pixels and five exact own-run capture comparisons,
      frontend1116/guest1094 ticks, zero exhausted frames. Pre-drill captures match;
      actual final captures inspected. Own-run consistency is not hardware accuracy.
      Native binary SHA-256
      b2aada24f38cf1f555f6b76939bfee827d3a1656e3c6915ca1af92e8a408b8ff. Both active/control also pass12 checks each with exact reference images on
      dffc7668070fdef10e088a2a3387f53baa4e701f PEF candidate, now merged as
      79d256e11c088ab671aeb487b1b5b2ab294d5084 with identical tested tree.
      Complete games, persistence, audio fidelity and released public play remain
      unverified; browser movement/drilling evidence follows below.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5808
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: 646598393f4dc5d5b8f9213e9b82e2625b0e6c96 private browser on 55657c5e runtime
    architecture: ppc
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5808
    environment: >-
      Fresh Chromium isolated worker/WebGL profiles and SharedArrayBuffer;
      default PowerPC/25MHz/800x600/default16-bit color, original15fps simulation
      throttle unchanged. Audio gesture outside the application window, then
      normal Begin Game before each unchanged20-second sample. Ordinary held
      Right moves the platform; held Down produces a visible bore and spends
      cash200 to181 in pair01 and200 to160 in pair02. Equal-clock controls retain
      cash200 and no bore, omitting only Down. Actual initial/final captures
      inspected. Native oil extraction is verified separately; browser oil
      revenue, seismic surveying, complete games, persistence, audio fidelity
      and public released play remain unverified. Initial browser holds reached
      less far than native; pair02 adjusts only held-input duration from measured
      displacement, without changing any game/profile/pacing setting.
      Pair01 active/control steady guest57.6232/62.1665 per second, host60.7102/61.1304FPS,
      maximum worker35.5/32.9ms and minimum audio114.5/129.6ms.
      Pair02 active/control steady guest62.2675/60.1145 per second, host61.2297/60.1145FPS,
      maximum worker35.8/31.6ms and minimum audio121.0/121.3ms.
      All unchanged pacing gates pass; no captured lifecycle/network errors.
      These are steady snapshots, not whole-game averages. Independent game
      starts; no same-terrain or browser determinism claim. Actual source stamp,
      game ID, worker, architecture,25MHz, isolation and shared memory checked.
      Original archive and both main/worker Wasm HTTP200 response bodies match
      independent integrity pins. Wasm17889166 bytes, SHA-256
      bd762f1a5af513f818c399651b274a9d3888ac5677ef8087928f6e747d07aff6.
runtime:
  executable_path: Seismic Duck 1.6 ƒ/Seismic Duck 1.6
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 178d439c4a791c78eaac0d85b09d36142353095798de902db799f1700693e842
    size_bytes: 129713
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/seismic-duck-16.hqx
    - https://github.com/benletchford/systemless/issues/5808
    license: Original unchanged distribution grant with accompanying README
    rights_holder: Arch D. Robison
    permission: >-
      Complete 29960-byte README, revised February21,2001, explicitly permits free
      distribution unchanged with the README. Complete1330-byte New Features also
      reviewed. Preserve all three files, forks, Finder metadata and notices.
    notes: "Independent extraction matches production packing: three files, six forks, three Finder records; strict KPK300618 bytes, SHA-256 11f0ad2403692b3a2c62f8201b554d1388d0de8adc5bd325f9fb6b8aa5e9f7b5. Original PowerPC application; no external runtime supplied."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 38c6ea0db855a4917b01d919b234737dff38ed51c56096f75e19b68b90ea9a0e
    size_bytes: 5269
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5808
    rights_holder: Arch D. Robison
    permission: Fresh content-only capture from the complete unchanged distributable original.
    notes: Exact crop (0,20)-(642,482), no resampling; scored game after oil extraction.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/seismic-duck-16.hqx
- https://github.com/benletchford/systemless/issues/5808
---

![Drilling and oil extraction in Seismic Duck](https://assets.systemless.org/catalogue/media/sha256/38/38c6ea0db855a4917b01d919b234737dff38ed51c56096f75e19b68b90ea9a0e.png)

Choose Begin Game from File to start prospecting. Left and Right move the drilling
platform, Down drills and Up retracts the drill. The platform cannot move while
the drill is in rock. Move the duck with < and >, and press Space to fire its air
gun. Read the returning seismic waves to choose a drilling site. Drilling costs
money; extracted gas and oil earn it back. The original Duckumentation explains
how to interpret the waves and explore geology outside scored game mode.
