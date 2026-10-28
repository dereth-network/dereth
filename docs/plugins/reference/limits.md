# Limits

**Provenance: design.** Generated from the plugin interface's own code, so it cannot drift from it. Do not edit it: `DERETH_TEST_GOLDEN=1 cargo test -p dereth-testkit --test cpu plugins::the_plugin_reference` rewrites it.

The numbers the host holds every plugin to, from the host's own constants. Each is described where it applies: the sandbox in the [README](../README.md#how-a-plugin-runs), actions in [actions.md](../actions.md), the rate ceilings in [permissions.md](../permissions.md#rate-ceilings).

## The VM

| limit | value | what it bounds |
|---|---|---|
| memory | 64 MiB | the most memory one plugin's VM may hold (the host's default) |
| time budget | 4 ms | how long one entry (loading, or delivering one event) may run before it is stopped (the host's default) |
| compiled work | 100000 units per ms | a compiled plugin's budget, counted in function entries and loop iterations, the same on every host |

## Packages

| limit | value | what it bounds |
|---|---|---|
| files | 2000 | the most files one package may hold |
| size | 64 MiB | the most bytes one package may hold, unpacked |
| file types | `.lua`, `.luau`, `.toml`, `.json`, `.xml`, `.txt`, `.md`, `.png`, `.ttf`, `.otf`, `.utl`, `.nav`, `.met`, `.sig`, `.wasm` | the only file extensions a package may hold |

## Output, tasks and stores

| limit | value | what it bounds |
|---|---|---|
| printed lines | 50 a frame | local chat lines one plugin may print in one frame (`chat.print` and `print`) |
| printed lines kept | 200 lines | the lines of what a plugin printed (and its errors) the host keeps |
| timer period | 50 ms | the shortest period of a repeating timer |
| store | 16 MiB | the most one store scope may hold, as JSON |
| patterns | 256 | regular expressions one plugin keeps compiled; the oldest is dropped past it |

## Actions

| limit | value | what it bounds |
|---|---|---|
| activation | 30 s | how long a player's gesture makes the plugin's actions interactive |
| queue wait | 30 s | the longest an action waits for its turn before it fails as `busy` |
| queued | 256 | the most actions one plugin may have queued |
| appraisals | 5 a second | the most appraisals one plugin may ask for |
| file requests | 1 | the file requests one plugin may have waiting for the player |
| after a refusal | 30 s | how long a plugin's next file request waits after the player refuses one, doubling with each refusal |
| refusals | 5 | the refusals in a session after which a plugin is asked for no more files until it is reloaded |

## UI and markers

| limit | value | what it bounds |
|---|---|---|
| nodes | 2000 | the most nodes one window may hold |
| depth | 16 | the deepest a window's tree may be |
| list rows | 10000 | the most rows one list may hold |
| text | 4096 characters | the most one text may hold: a property, an entry of a list, a cell, a drawing's words |
| window text | 64 KiB | the most text one window may hold, all its nodes' together |
| canvas commands | 1000 | the most draw commands one canvas may hold |
| picture | 1024 by 1024 pixels | the largest picture kept, from the package or fetched |
| pictures | 32 MiB | the most one plugin's pictures may take, decoded |
| markers | 256 | the most markers one plugin may keep |
| map points | 4096 | the most points one map marker may hold |

## Movement and the session

| limit | value | what it bounds |
|---|---|---|
| turn | 15° | how far off the target's bearing the character may be and still run; beyond it, it stops and turns in place |
| turn again | 2 s | how long a turn is given to finish before it is asked for again |
| player first | 5 s | after the player moves the character, how long a movement no gesture asked for is refused (`player_moving`) |
| stuck | 6 s | without coming half a metre closer, a move fails as `stuck` |
| slowest pace | 2 m/s | a move slower than this fails as `timeout` |
| lost | 3 s | a followed object out of sight this long fails the follow as `lost` |
| face | 3 s | how long a face may take |
| entry | 60 s | how long entering the world may take |
| log-off | 30 s | how long a log-off may take |

## Other clients and the network

| limit | value | what it bounds |
|---|---|---|
| peer message | 64 KiB | the most one message's data may take, as JSON |
| peer sends | 20 a second | the most messages one plugin may send |
| requests | 4 | the most HTTPS requests one plugin may have outstanding |
| answer | 8 MiB | the most bytes an HTTPS answer's body may hold |
