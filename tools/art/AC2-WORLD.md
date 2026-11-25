# The AC2-in-AC1 world

Asheron's Call 2's landmass, rebuilt in Asheron's Call's own formats: AC2's ground, water and roads
at their true size, its trees, plants, rocks and grass, its placed objects and buildings, its map
names, and one starting area by Cragstone. Asheron's Call's own landblocks are replaced; its
dungeons stay under the new land. The world is a **data overlay**: a set of records laid over the
game's own files, which an Empyrean server serves and patches into every Dereth client that joins.

Nothing of either game is distributed. `dereth-art` builds the world on your machine from your own
installs, and it stays with you and the players of your server.

## What you need

- **Asheron's Call**, the final retail client's files: `client_cell_1.dat`, `client_portal.dat`,
  `client_local_English.dat` and `client_highres.dat` from acclient 00.00.11.6096 (June 2015).
- **Asheron's Call 2**, its end-of-retail files: `cell_1.dat`, `portal.dat`, `local_English.dat`
  and `highres.dat`, as the AC Community Preservation Project archives them and Dereth's launcher
  fetches them.
- `dereth-art`, built from this repository (`cargo build --release -p dereth-art`). A build takes
  about ten minutes.

The tool checks each file by its SHA-256 and refuses any other version, so a world built anywhere
from the same files is the same world, byte for byte. `--any-install` builds from other files
anyway; the world it makes may then match no one else's, and a player who built it from the
expected files would hold a different one.

## Build it

```text
dereth-art ac2-world --ac1 <your AC1 install> --ac2 <your AC2 install> --out ac2-world/overlay
```

It writes the overlay folder `ac2-world/overlay`:

| File | What |
|---|---|
| `overlay_cell.dat`, `overlay_portal.dat` | The world's records over the two game files. |
| `world.toml` | The world's own rules: its dungeons are told apart by cell, and it places its own things. |
| `places.tsv` | AC2's towns and landmarks, which the map shows. |

Beside the folder go `generations.tsv` (the ledger, below), a `README.txt` saying how the world was
made, `stats.txt` and a set of reports in tab-separated text: how the ground was rounded, which
placed objects were left out and why, where ledges were made climbable, and more. The reports are
the same bytes on every build from the same files.

Options:

- `--names off` leaves out `places.tsv`.
- `--frills off` grows the cover without AC2's small grass and plants.
- `--sky ac1` keeps AC1's own sky. By default the world has AC2's: its seven days, its sun, moon,
  stars and clouds, colored by the hour as AC2 colors them (the overlay's sky tints, which Dereth
  draws). `--sky-day 1` to `7` keeps one of AC2's days alone.
- `--levels 2` or `3` gives placed objects more levels of detail (one by default).
- `--table ac2` keeps AC2's own height table instead of one fitted to the converted land.
- `--vaults tools/art/ac2-world/arwic-vault.txt --placements tools/art/ac2-world/arwic-vault-placements.tsv`
  adds Arwic's vault: AC2's dungeon under its entrance, with a portal down from the entrance hall
  and one back up. The placements are written as `placements.tsv`, which the server places.

### Rebuilding: keep the ledger

A client keeps a world's records and is sent them again only when the world holds newer ones. So
every build that changes the world's content must be numbered past the last. `generations.tsv` (or
the file `--ledger` names) lists each content built so far with its number. Keep it beside the
overlay folder and every later build uses it: the same content keeps its number, and new content
takes the next. Lose it and a changed world can't reach the clients that hold the old one; they
must clear their kept copy.

## Serve it

Point Empyrean at the overlay folder in `empyrean.toml` ([SETUP.md](../../empyrean/SETUP.md),
"Serving a converted world"), beside your usual world database:

```toml
[server]
dat_files_directory = "<your AC1 install>"

[dat_overlay]
path = "ac2-world/overlay"
patching = true
```

The server reads the world as your AC1 files with the overlay over them, reads the world's rules
from `world.toml` (it places no world-database content on this world's land, only what the world
places itself), and publishes `places.tsv` to the map. Players' clients take the overlay from the
server the first time they join: about 125 MB, a minute or so on a local network. Open the server's
TCP port as well as its UDP ports so the stream can use it (SETUP.md, section 3.5).

## Play it

Connect with the Dereth client as to any Empyrean server. The first login streams the world, then
puts you in it. A new character starts at Cragstone. The map shows AC2's towns and landmarks.

A client keeps each world's records in its own folder for that server, so the AC2-in-AC1 world
never changes your client's AC1 files. If a server rebuilt its world without its ledger, clear the
client's kept copy for that server to take the new one.

To look round the world with no server, give the client the overlay folder:

```text
dereth-client --dat-dir <your AC1 install> --overlay-dat-dir ac2-world/overlay --explore "camera 0x5C8C [96 96 110] yaw 0 pitch -1.2"
```

That starts the camera over Cragstone.
