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
| Civilization II | Unqualified | Civilization II PPC Demo 1.0 (25 July 1997), with a PowerPC PEF application. README inspected without an affirmative archive grant; this does not supply the [68K package](https://github.com/benletchford/systemless/issues/2856); [dialog rendering](https://github.com/benletchford/systemless/issues/3107) remains separate. See [exact archive receipt](CATALOGUE-SOURCES.md#five-classic-demo-archive-identities). |
| SimFarm | Unqualified | Recheck the exact archive and rights; prior [window-cycle performance work](https://github.com/benletchford/systemless/issues/1587) does not establish catalogue readiness. |
| The Playroom | Unqualified | A [public archive hash and native comparison](https://github.com/benletchford/systemless/issues/475) exist; cleanup, printer, and foreground rendering issues remain open. Establish exact-archive rights. |
| Day of the Tentacle | Unqualified | Bundled README identifies version 1.0 (16 October 1995) as a non-interactive demonstration. PowerPC PEF slice and two CODE resources are present; no whole-archive grant found. Do not claim interactive gameplay; prior [intro work](https://github.com/benletchford/systemless/issues/117) remains separate. See [exact archive receipt](CATALOGUE-SOURCES.md#five-classic-demo-archive-identities). |
| Myst (original) | Unqualified | Myst Preview has 47 CODE resources and accompanying preview media. No standalone licence or affirmative distribution grant found in inspected contents; it is not the full original game. See [exact archive receipt](CATALOGUE-SOURCES.md#five-classic-demo-archive-identities). |
| Shufflepuck Café | Unqualified | Prior [startup work](https://github.com/benletchford/systemless/issues/1600) does not establish current gameplay or rights for an exact archive. |
| Sid Meier’s Colonization | Unqualified | Exact archive and bundled redistribution terms needed. |
| Populous II | Unqualified | MacPopulousII Demo has seven CODE resources. Bundled READ ME expressly requires prior written consent for copying/reproduction; exact archive permission is required; retain the [launch-alert investigation](https://github.com/benletchford/systemless/issues/1633). See [exact archive receipt](CATALOGUE-SOURCES.md#five-classic-demo-archive-identities). |
| King’s Bounty | Unqualified | Exact archive and bundled redistribution terms needed. |
| Afterlife | Unqualified | Separate Afterlife Demo (68040) and Afterlife Demo (PowerPC) applications; the latter has a PPC PEF slice. Bundled README specifies both architectures but supplies no affirmative archive grant. See [exact archive receipt](CATALOGUE-SOURCES.md#five-classic-demo-archive-identities). |
| A-Train | Unqualified | Exact archive and bundled redistribution terms needed. |
| Allied General | Unqualified | Exact public download now matches [the 68K demo issue](https://github.com/benletchford/systemless/issues/4171); a PPC PEF slice is also present. Rights remain unestablished and battle status text is missing. See [original archive receipts](CATALOGUE-SOURCES.md#additional-original-archive-receipts). |
| Mario’s Game Gallery | Unqualified | Original demo sourced and hashed; bundled instructions contain no affirmative distribution grant. See [archive receipts](CATALOGUE-SOURCES.md#downloaded-archive-receipts). |
| Uninvited | Unqualified | Original ScummVM demo sourced and hashed; its 68K CODE app halts during startup. Continue [qualification](https://github.com/benletchford/systemless/issues/3355) and the [Lo3Bytes mask fix](https://github.com/benletchford/systemless/issues/4235); exact-archive rights remain unestablished. |
| Dark Castle (original) | Unqualified | Original demo sourced and hashed; no standalone licence or TEXT grant found in inspected contents. See [archive receipts](CATALOGUE-SOURCES.md#downloaded-archive-receipts). [Color Dark Castle](catalogue/color-dark-castle.md) is a different entry. |
| Stunt Copter | [Catalogued](catalogue/stunt-copter.md), 68K | Check any broader gameplay claim against the recorded 1.2 archive. |
| Alley 19 Bowling | Unqualified | Exact original demo sourced and hashed, with a PPC PEF slice present. Inspected README/resource text supplies no affirmative distribution grant. See [archive receipts](CATALOGUE-SOURCES.md#additional-original-archive-receipts); verify any claimed 68K slice separately. |
| Space Cab | Unqualified | Unchanged 1.2 installer sourced and hashed; its terms allow installer distribution only. Installation completes 23 files, but the installed application returns to the launcher. Continue [original installer research](https://github.com/benletchford/systemless/issues/2642); see [source inspection](CATALOGUE-SOURCES.md#qualified-installer-evidence). |
| Meteor Storm | [Catalogued](catalogue/meteor-storm.md), **PPC** | Existing archive is PowerPC, despite this intake grouping; verify any proposed 68K version separately. |
| Pararena 2.0 | Unqualified | Public source contains **Pararena Demo 2.01**; the bundled contact note is not a distribution grant. Resolve requested-version identity and rights; see [archive receipts](CATALOGUE-SOURCES.md#downloaded-archive-receipts). |
| Zork I (Infocom sampler) | Unqualified | Public sampler archive contains MaxZip-wrapped game data. Interpreter permissions preserve separate game-data restrictions; identify the exact requested sampler and establish its rights. See [archive receipts](CATALOGUE-SOURCES.md#downloaded-archive-receipts). |

## PowerPC intake (25)

| Candidate | Repository state | Next evidence or work |
| --- | --- | --- |
| Gridz | Unqualified | [Gridz 1.2 installer qualification](https://github.com/benletchford/systemless/issues/4227) establishes intact-installer distribution permission and bounded PPC board interaction. [Absolute resource lookup fix](https://github.com/benletchford/systemless/pull/4229) advances 68K to a corrupted title animation. [Hosted promotion](https://github.com/benletchford/systemless/pull/4232) is complete with launch disabled; sustained gameplay and browser validation remain required; #1979 concerns a different 1.0 archive. |
| Tomb Raider Gold demo | Retired | The catalogue entry was removed in v0.82.1; do not restore without exact-archive rights and a new qualification review. |
| Ferazel’s Wand demo | [Catalogued](catalogue/ferazels-wand-demo.md), PPC | Check any broader gameplay claim against the recorded 1.0.3 archive. |
| King of Dragon Pass demo | Unqualified | Exact demo sourced and hashed, with a PPC PEF application. Linked tutorial PDF inspected without an archive redistribution grant. Remaining bundled terms need inspection; continue [PowerPC CFM startup investigation](https://github.com/benletchford/systemless/issues/4026). See [archive receipts](CATALOGUE-SOURCES.md#additional-original-archive-receipts). |
| Myth: The Fallen Lords demo | Unqualified | Original installer sourced and expanded through the public loader; PPC application and bundled READMEs identified. No archive distribution grant found in those documents. See [archive receipts](CATALOGUE-SOURCES.md#additional-original-archive-receipts). |
| Myth II: Soulblighter demo | Unqualified | Exact original demo sourced and hashed, with a PPC PEF slice present. Rights remain unestablished; investigate [tutorial loading](https://github.com/benletchford/systemless/issues/4132) and [application memory](https://github.com/benletchford/systemless/issues/3215). See [archive receipts](CATALOGUE-SOURCES.md#additional-original-archive-receipts). |
| Quake II demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Unreal Tournament demo | Unqualified | Original 348m4 archive sourced and hashed, with a PPC PEF application. Bundled HTML READMEs supply no affirmative archive distribution grant. See [archive receipts](CATALOGUE-SOURCES.md#additional-original-archive-receipts). |
| Bugdom demo | Unqualified | PPC PEF demo application; both bundled READMEs inspected without an affirmative archive distribution grant. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). Investigate [menu artwork](https://github.com/benletchford/systemless/issues/2682) and [missing labels](https://github.com/benletchford/systemless/issues/2777); exact archive terms still needed. |
| Age of Empires demo | Unqualified | Original trial archive sourced and hashed, with a PPC PEF application. Resource copyright/reproduction warnings do not grant redistribution. See [archive receipts](CATALOGUE-SOURCES.md#additional-original-archive-receipts). |
| Age of Empires II demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Total Annihilation demo | Unqualified | PPC PEF TA Demo application; no standalone original README found in the extracted package. Exact-package permission remains unestablished. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). Investigate [DrawSprocket resolution](https://github.com/benletchford/systemless/issues/3222); exact archive terms still needed. |
| Sid Meier’s Alpha Centauri demo | Unqualified | PPC PEF demo application. README mentions a demo distribution, but supplies no affirmative permission to host it. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). |
| Heroes of Might and Magic III demo | Unqualified | PPC PEF application; bundled README includes copyright notices but no affirmative archive grant. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). |
| Caesar III demo | Unqualified | PPC PEF application. Bundled Read Me includes an EULA requiring written consent for reproduction and transfer of copies. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). |
| Combat Mission: Beyond Overlord demo | Unqualified | PPC PEF application; exact bundled archive permission remains unestablished. Linked demo documentation does not itself clear hosting. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). |
| Tony Hawk’s Pro Skater 2 demo | Unqualified | Separate OS9 and OSX demo applications expose PPC PEF slices. Bundled README inspected without a complete-archive grant; validate the intended classic route separately. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). |
| Oni demo | Unqualified | PPC PEF application plus bundled CarbonLib, InputSprocket, and Bink libraries. Both READMEs inspected without a complete-package grant. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). |
| Deus Ex demo | Unqualified | PPC PEF game and Relauncher applications. Both bundled HTML READMEs inspected without an affirmative complete-archive grant. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). |
| The Sims demo | Unqualified | Exact archive and bundled redistribution terms needed. |
| Cro-Mag Rally demo | Unqualified | PPC PEF demo application. Bundled OpenGL 1.2.1 Software Redistribution notice expressly prohibits distribution of those Apple files; permission must cover the intact package. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). |
| Otto Matic demo | Unqualified | PPC PEF demo application; original manual reviewed without a complete-archive redistribution grant. Remaining bundled terms need inspection. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). Continue [CarbonLib qualification](https://github.com/benletchford/systemless/issues/3401) and [OpenGL startup work](https://github.com/benletchford/systemless/issues/3405). |
| Worms Blast demo | Unqualified | PPC PEF application; bundled HTML README inspected without an affirmative complete-archive distribution grant. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). |
| Aliens versus Predator demo | Unqualified | Main game exposes a PPC PEF slice. GameRanger is also bundled; the game README supplies no affirmative complete-package grant. See [archive receipt](CATALOGUE-SOURCES.md#thirteen-remaining-public-demo-receipts). |
| Nanosaur | Unqualified | Official original 1.3.4 disk image sourced and hashed; its embedded licence restricts distribution and network transmission. Obtain [exact-image permission](https://github.com/benletchford/systemless/issues/4233), then validate architecture, gameplay, hosting, and browser behavior. |
