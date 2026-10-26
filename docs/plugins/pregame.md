# The character list, logging in and out

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A plugin loaded before login (every host loads its plugins at start) hears the session around the
world (`session.pregame` capability):

| event | when |
|---|---|
| `session.pregame` | the character list is up: after connecting, and after each log-off. `characters` is a list of `{ id, name }` |
| `session.login_failed` | the server refused an entry (`Character_CharacterError`, `0xF659`): `code` and `reason` |
| `session.disconnect` | the connection ended |

`session.info()` answers `{ phase, characters, world, account, portal_space, context }`; `phase`
is `offline`, `connecting`, `character_select`, `entering`, `in_world` or `disconnected`, and
`context` is `pregame`, `chargen` or `world`.

A plugin meant to act here names `pregame` among its `contexts` ([manifest](manifest.md)); one
that names only `world` still hears these events, but shows nothing and runs no timers until
a character is in the world.

With `session.control` a plugin can do what the character screen's buttons and the log-out do.
Each is a promise:

| call | completes | fails |
|---|---|---|
| `session.select_character(name or id)` | the character is in the world; answers `{ id, name }` | the entry's reason, `disconnected`, `timeout` (60 s) |
| `session.logout()` | the character list is back | `disconnected`, `timeout` (30 s) |
| `session.quit()` | at once: the client logs off (if in the world) and ends | |

`select_character` needs the character list up, `logout` a character in the world; otherwise the
call is an error. One operation is pending at a time.

A world's policy decides session control (`session` in [the policy](policy.md)): `allow`, only
after a gesture, or `deny`, when the promise fails with `denied_by_world`. A world that publishes
no policy allows it.

**Reasons.** `reason` names the server's code: `logon` (1), `account_login` (3), `server_crash` (4,
8), `logoff` (5), `delete` (6), `account_invalid` (9), `account_missing` (10),
`enter_game_generic` (11), `account_stressed` (12), `character_in_world` (13: the character's last
session is still in the world; trying again after a few seconds usually succeeds),
`player_account_missing` (14), `character_not_owned` (15), `character_in_world_server` (16),
`old_character` (17), `corrupt_character` (18), `start_server_down` (19),
`could_not_place_character` (20), `server_full` (21), `character_locked` (23),
`subscription_expired` (24), `other`. A refused entry leaves the session where the client leaves
it: the character list is up again when the server keeps the connection.

**Known limit.** A server refusal that does not name the request it answers (a
`Communication_WeenieError` with no correlation) fails whichever plugin action is outstanding; see
[actions](actions.md#how-an-action-ends).
