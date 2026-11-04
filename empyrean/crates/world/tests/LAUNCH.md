# Offline Launch content tests

The `launch-content` feature selects an independent, opt-in fixture. It does not
change the `real-content` tier's retail DAT or pack defaults. Tests create `World`
with Launch rules, real DAT readers, the supplied pack, in-memory shard/auth
stores, the pack's initial event states, an injected clock and a seeded RNG. They
do not start a server host, open sockets or create a client/window/GPU.

Set `EMPYREAN_TEST_LAUNCH_FIXTURE` to a local JSON manifest:

```json
{
  "capture": "launch",
  "dat_directory": "/absolute/local/dat-folder",
  "portal_sha256": "<64 hex digits>",
  "cell_sha256": "<64 hex digits>",
  "world_pack": "/absolute/local/world.pack",
  "world_pack_sha256": "<64 hex digits>"
}
```

The folder must hold raw `portal.dat` and `cell.dat`. Supply hashes from a trusted
fixture inventory, not hashes calculated merely to accept whatever files happen
to be present. The suite verifies all three hashes before loading, requires the
Launch portal/cell header iterations (105/67), and rejects a non-Launch pack.
Missing files or a missing manifest fail; there is no later-DAT fallback. No
assets or world content are distributed with the tests.

Build the pack with `empyrean-import ac1999`, whose default target is these files. The
source places the Yanshi outpost against later cell numbering, and the geometry check below
fails on a pack built for the `source` target.

```text
cargo test -p empyrean-world --profile test-release --test all --features launch-content world::launch_content -- --test-threads=1
```

The checks cover required-table decoding, creation from all three heritages,
both sexes and all eight templates, initial terrain and outpost stock in all 18
start areas, and a short ground movement through the server's position-update
path using a character produced by the real factory. All 48 creations check
their complete starter inventory, known spells, account-formula supplies and
item setup decoding. A created caster equips its granted wand, changes stance
and casts a granted self buff through the queued action path, paying mana with
component requirements enabled. A geometry check also requires the Yanshi
outpost instances to name the classic rooms containing their stored locations.

These checks do not establish exhaustive appearance or asset closure, network
onboarding, vendor transactions, portals, combat against monsters, every legal
custom skill combination, loot or historical rule fidelity. Candidate readiness
reports must retain those limits; the fixture never substitutes later content.
