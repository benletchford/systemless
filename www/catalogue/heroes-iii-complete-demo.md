---
id: heroes-iii-complete-demo
kind: game
title: Heroes of Might and Magic III Complete Demo
summary: Explore a fantasy map and lead heroes and armies in this turn-based strategy demo.
developer: New World Computing / Evolution Interactive
publisher: The 3DO Company
year: 2000
architectures:
- ppc
default_architecture: ppc
category: Strategy
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: 9f4a4207 native candidate
    architecture: ppc
    environment: >-
      Original complete demo, native PowerPC, default CPU, partition, sound and
      initial display. Ordinary New Game, welcome dismissal and two chest clicks
      reach the reward dialog. Taking gold raises 30000 to 31500; an equal-duration
      omitted-confirmation control retains the dialog and 30000 gold. Pre-choice
      captures match exactly. Each repeat passes three measured pixel assertions
      at 2144 frontend/guest ticks with zero exhaustion. This is bounded native
      input/reward evidence, not independent hardware accuracy. Sustained play,
      battles, save/load, audio fidelity and browser pacing remain unqualified.
      Launch stays disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5921
runtime:
  executable_path: Heroes III Demo/Heroes III Demo
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/heroes-of-might-and-magic-iii/
    expected_sha256: 3e5f77588f00ca681717aeaed8060d1e400a1774fb6c9787232209d64c007680
    expected_size: 110469948
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/heroes-of-might-and-magic-iii
    - https://www.insidemacgames.com/historical/news/index.php%3Fdate%3D2001-01-12.html
    - https://www.insidemacgames.com/historical/news/index.php%3Fdate%3D2001-01-15.html
    - https://github.com/benletchford/systemless/issues/5921
    rights_holder: The 3DO Company and its licensors (original 2000 notice)
    license: Maintainer-accepted freely distributed promotional demo
    permission: >-
      Accepted under the project's promotional-demo policy with explicit
      maintainer authorization on 2026-11-10. Contemporary release and cover-disc
      reporting corroborates period promotional distribution. The complete
      original package is preserved, and no explicit redistribution restriction
      was found in inspected applicable notices. This is project-policy acceptance,
      not an affirmative publisher licence grant or current ownership assertion.
      Preserve original copyright and bundled component notices.
    notes: >-
      Original 110469948-byte SIT, SHA-256
      3e5f77588f00ca681717aeaed8060d1e400a1774fb6c9787232209d64c007680,
      matching the earlier tested archive in #4294. All 20 files, 40 forks and
      20 Finder records match independent unar extraction; strict web-pack end
      117816218, SHA-256
      047216b2635b66e7a7f5daed003a64ea17e4ccbfe4158030aec24f65fac88ee2.
      The complete 6007-byte Read Me identifies Mac demo v4.0, December 2000;
      all 25 selected UI/text/version resources were inspected. Data, maps and
      music are unchanged. Required external Apple Sprockets/QuickTime installers
      are not bundled in this archive. No publisher-CD byte parity or modern-port
      licence is substituted.
- id: treasure-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/heroes-iii-complete-demo/treasure.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5921
    rights_holder: The 3DO Company and its licensors (original artwork)
    permission: >-
      Fresh Systemless capture from the project-policy-accepted promotional demo;
      original artwork retains its copyright.
    notes: "Exact 800x600 native game surface after accepting the treasure's gold reward; original game UI, no host UI, Classic Mac menu bar, cropping, resampling or colour changes. SHA-256 4ff133dfce5d693ed32d68bf13a3f18b2104b90d72cb40d47a086e9dcc98115f. Native bounded evidence only."
references:
- https://github.com/benletchford/systemless/issues/5921
- https://github.com/benletchford/systemless/pull/4294
---

![Heroes III native adventure map after collecting gold](incoming/heroes-iii-complete-demo/treasure.png)

Explore a fantasy map, collect resources and lead armies in turn-based battles.
This preserves the original December 2000 Macintosh Complete demo.

A bounded native treasure interaction is observed: gold rises from 30000 to
31500 after taking the reward. An equal-duration omitted-confirmation control
retains the reward dialog and 30000 gold. This does not establish sustained play,
battles, saving/loading, audio fidelity or browser pacing. Launch stays disabled.

A private browser build reaches the demo menu, but ordinary New Game remains on
its loading screen through a 30-second welcome-dialog readiness bound. Scheduling
metrics pass during the measured loading window; that does not qualify browser
gameplay or sustained play. The cause and eventual completion remain unproven.
See the [browser checkpoint](https://github.com/benletchford/systemless/issues/5921).
