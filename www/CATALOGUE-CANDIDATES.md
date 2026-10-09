# Catalogue candidate queue

This is a research and development queue for 50 requested games, not a release
list. "Catalogued" means a record exists in `catalogue/`; its compatibility
claim has only the scope stated in that record. "Unqualified" means this
repository does not yet contain a catalogue record for the candidate. Neither
state grants distribution rights for another version or archive.

Before submitting an entry, identify the **exact archive** by URL, SHA-256, and
size; inspect its bundled terms and complete contents for permission to
redistribute that archive; validate its executable architecture; capture bounded
gameplay evidence; and test the browser route with the archive hosted by
Systemless. Follow [the catalogue contribution guide](README.md#contribute-a-catalogue-entry).
Keep launch disabled until browser testing is approved. Record compatibility
defects in focused issues. Do not offer a third-party download as a substitute
for a Systemless-hosted archive.

Public source pages, inspected documentation, and exact download receipts are recorded
in [the source inspection record](CATALOGUE-SOURCES.md). Availability is separate
from permission to distribute and compatibility.

## 68K or dual-architecture intake (25)

| Candidate | Repository state | Next evidence or work |
| --- | --- | --- |
| Munchies | [Catalogued](catalogue/munchies.md), 68K | Check any broader gameplay claim against the recorded 1.0.7 archive. |
| Koji the Frog | [Catalogued](catalogue/koji-the-frog.md), 68K | Check any broader gameplay claim against the recorded 2.0.1 archive. |
| Centaurian | [Catalogued](catalogue/centaurian.md), 68K and PPC | Check any broader gameplay claim against the recorded 1.2.1 archive. |
| Bonkheads Deluxe | Unqualified | The [exact demo archive](CATALOGUE-SOURCES.md#downloaded-archive-receipts) is sourced and hashed; its inspected README and TEXT resources contain no affirmative distribution grant. Earlier [palette work](https://github.com/benletchford/systemless/issues/345) concerns Bonkheads, so validate this Deluxe archive separately. |
| Civilization II | Unqualified | [Original 68K demo research](https://github.com/benletchford/systemless/issues/2856) identifies promotional media, but does not establish redistribution permission; resolve [dialog rendering](https://github.com/benletchford/systemless/issues/3107). |
| SimFarm | Unqualified | Recheck the exact archive and rights; prior [window-cycle performance work](https://github.com/benletchford/systemless/issues/1587) does not establish catalogue readiness. |
| The Playroom | Unqualified | A [public archive hash and native comparison](https://github.com/benletchford/systemless/issues/475) exist; cleanup, printer, and foreground rendering issues remain open. Establish exact-archive rights. |
| Day of the Tentacle | Unqualified | Prior [SCUMM intro work](https://github.com/benletchford/systemless/issues/117) does not establish current gameplay or rights for an exact archive. |
| Myst (original) | Unqualified | Exact archive and bundled redistribution terms needed. |
| Shufflepuck Café | Unqualified | Prior [startup work](https://github.com/benletchford/systemless/issues/1600) does not establish current gameplay or rights for an exact archive. |
| Sid Meier’s Colonization | Unqualified | Exact archive and bundled redistribution terms needed. |
| Populous II | Unqualified | Prior [launch-alert work](https://github.com/benletchford/systemless/issues/1633) does not establish current gameplay or rights for an exact archive. |
| King’s Bounty | Unqualified | Exact archive and bundled redistribution terms needed. |
| Afterlife | Unqualified | Exact archive and bundled redistribution terms needed. |
| A-Train | Unqualified | Exact archive and bundled redistribution terms needed. |
| Allied General | Unqualified | The [68K demo issue](https://github.com/benletchford/systemless/issues/4171) pins an archive hash and records missing battle status text; it does not attach the archive or establish redistribution rights. |
| Mario’s Game Gallery | Unqualified | Original demo sourced and hashed; bundled instructions contain no affirmative distribution grant. See [archive receipts](CATALOGUE-SOURCES.md#downloaded-archive-receipts). |
| Uninvited | Unqualified | Continue [original demo qualification](https://github.com/benletchford/systemless/issues/3355). |
| Dark Castle (original) | Unqualified | Original demo sourced and hashed; no standalone licence or TEXT grant found in inspected contents. See [archive receipts](CATALOGUE-SOURCES.md#downloaded-archive-receipts). [Color Dark Castle](catalogue/color-dark-castle.md) is a different entry. |
| Stunt Copter | [Catalogued](catalogue/stunt-copter.md), 68K | Check any broader gameplay claim against the recorded 1.2 archive. |
| Alley 19 Bowling | Unqualified | Exact archive and bundled redistribution terms needed. |
| Space Cab | Unqualified | Unchanged 1.2 installer sourced and hashed; its terms allow installer distribution only. Installation completes 23 files, but the installed application returns to the launcher. Continue [original installer research](https://github.com/benletchford/systemless/issues/2642); see [source inspection](CATALOGUE-SOURCES.md#qualified-installer-evidence). |
| Meteor Storm | [Catalogued](catalogue/meteor-storm.md), **PPC** | Existing archive is PowerPC, despite this intake grouping; verify any proposed 68K version separately. |
| Pararena 2.0 | Unqualified | Public source contains **Pararena Demo 2.01**; the bundled contact note is not a distribution grant. Resolve requested-version identity and rights; see [archive receipts](CATALOGUE-SOURCES.md#downloaded-archive-receipts). |
| Zork I (Infocom sampler) | Unqualified | Public sampler archive contains MaxZip-wrapped game data. Interpreter permissions preserve separate game-data restrictions; identify the exact requested sampler and establish its rights. See [archive receipts](CATALOGUE-SOURCES.md#downloaded-archive-receipts). |

## PowerPC intake (25)

| Candidate | Repository state | Next evidence or work |
| --- | --- | --- |
| Gridz | Unqualified | [Gridz 1.2 installer qualification](https://github.com/benletchford/systemless/issues/4227) establishes intact-installer distribution permission and bounded PPC board interaction. [Absolute resource lookup fix](https://github.com/benletchford/systemless/pull/4229) advances 68K to a corrupted title animation. Hosted promotion, sustained gameplay, and browser validation remain required; #1979 concerns a different 1.0 archive. |
| Tomb Raider Gold demo | Retired | The catalogue entry was removed in v0.82.1; do not restore without exact-archive rights and a new qualification review. |
| Ferazel’s Wand demo | [Catalogued](catalogue/ferazels-wand-demo.md), PPC | Check any broader gameplay claim against the recorded 1.0.3 archive. |
| King of Dragon Pass demo | Unqualified | Re-test the exact archive after the prior [PowerPC CFM startup investigation](https://github.com/benletchford/systemless/issues/4026); establish bundled redistribution rights. |
| Myth: The Fallen Lords demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Myth II: Soulblighter demo | Unqualified | Investigate [tutorial loading](https://github.com/benletchford/systemless/issues/4132) and [application memory](https://github.com/benletchford/systemless/issues/3215); exact archive terms still needed. |
| Quake II demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Unreal Tournament demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Bugdom demo | Unqualified | Investigate [menu artwork](https://github.com/benletchford/systemless/issues/2682) and [missing labels](https://github.com/benletchford/systemless/issues/2777); exact archive terms still needed. |
| Age of Empires demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Age of Empires II demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Total Annihilation demo | Unqualified | Investigate [DrawSprocket resolution](https://github.com/benletchford/systemless/issues/3222); exact archive terms still needed. |
| Sid Meier’s Alpha Centauri demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Heroes of Might and Magic III demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Caesar III demo | Unqualified | [Linked original README](https://static.classicmacdemos.com/demos/caesar-iii/README.txt) includes reproduction and transfer restrictions requiring written consent; no exact-archive hosting grant established. |
| Combat Mission: Beyond Overlord demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Tony Hawk’s Pro Skater 2 demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Oni demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Deus Ex demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| The Sims demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Cro-Mag Rally demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Otto Matic demo | Unqualified | Public documentation located; PDF and exact bundled terms need inspection. Continue [CarbonLib qualification](https://github.com/benletchford/systemless/issues/3401) and [OpenGL startup work](https://github.com/benletchford/systemless/issues/3405). |
| Worms Blast demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Aliens versus Predator demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Nanosaur | Unqualified | Exact archive and bundled redistribution terms needed. |
