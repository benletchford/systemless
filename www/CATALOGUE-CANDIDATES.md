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

## 68K or dual-architecture intake (25)

| Candidate | Repository state | Next evidence or work |
| --- | --- | --- |
| Munchies | [Catalogued](catalogue/munchies.md), 68K | Check any broader gameplay claim against the recorded 1.0.7 archive. |
| Koji the Frog | [Catalogued](catalogue/koji-the-frog.md), 68K | Check any broader gameplay claim against the recorded 2.0.1 archive. |
| Centaurian | [Catalogued](catalogue/centaurian.md), 68K and PPC | Check any broader gameplay claim against the recorded 1.2.1 archive. |
| Bonkheads Deluxe | Unqualified | Earlier [palette work](https://github.com/benletchford/systemless/issues/345) concerns Bonkheads; identify the exact **Deluxe** archive and its terms before treating that work as evidence for this candidate. |
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
| Mario’s Game Gallery | Unqualified | Exact archive and bundled redistribution terms needed. |
| Uninvited | Unqualified | Continue [original demo qualification](https://github.com/benletchford/systemless/issues/3355). |
| Dark Castle (original) | Unqualified | Identify the original archive and terms; [Color Dark Castle](catalogue/color-dark-castle.md) is a different entry. |
| Stunt Copter | [Catalogued](catalogue/stunt-copter.md), 68K | Check any broader gameplay claim against the recorded 1.2 archive. |
| Alley 19 Bowling | Unqualified | Exact archive and bundled redistribution terms needed. |
| Space Cab | Unqualified | [Original installer research](https://github.com/benletchford/systemless/issues/2642) records terms allowing installer distribution while forbidding separate installed components. Obtain the unchanged installer, validate installation and gameplay, then test a hosted browser route. |
| Meteor Storm | [Catalogued](catalogue/meteor-storm.md), **PPC** | Existing archive is PowerPC, despite this intake grouping; verify any proposed 68K version separately. |
| Pararena 2.0 | Unqualified | Exact archive and bundled redistribution terms needed. |
| Zork I (Infocom sampler) | Unqualified | Identify the exact sampler and its bundled redistribution terms. |

## PowerPC intake (25)

| Candidate | Repository state | Next evidence or work |
| --- | --- | --- |
| Gridz | Unqualified | [Demo issue #1979](https://github.com/benletchford/systemless/issues/1979) pins an original fat archive hash; 68K rendering is corrupt and PPC startup stops at a display prompt. Establish bundled redistribution rights and resolve those blockers before browser approval. |
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
| Caesar III demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Combat Mission: Beyond Overlord demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Tony Hawk’s Pro Skater 2 demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Oni demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Deus Ex demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| The Sims demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Cro-Mag Rally demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Otto Matic demo | Unqualified | Investigate [CarbonLib startup](https://github.com/benletchford/systemless/issues/3401) and [OpenGL startup](https://github.com/benletchford/systemless/issues/3405); exact archive terms still needed. |
| Worms Blast demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Aliens versus Predator demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Nanosaur | Unqualified | Exact archive and bundled redistribution terms needed. |
