# Corrections to common reimplementations

**Provenance: mixed.** Most rows are wire or file format, restated as behaviour. A few are observed
at runtime and say so. §7 is reported rather than reproduced and is fenced off accordingly.

The server emulators and the community protocol catalogue are the best secondary sources that exist,
and anyone rebuilding this game leans on them. They are also, in specific places, wrong — usually
because a field was never exercised by shipped data, or because a server does not need a behaviour the
client has.

This page collects every place where the client's behaviour contradicts a widely used reference, so
that somebody rebuilding from a mixture of sources knows which one to trust. Each row names the
reference, what it says, what the client does, and — where one exists — the test in this repository
that pins the answer.

**How to use it.** If you are porting from ACE or GDLE, treat this list as the diff you have to apply.
If you find a further disagreement, add a row.

## 1. Data formats

| Reference | It says | The client does | Pinned by |
|---|---|---|---|
| ACE's landblock reader | Terrain height in metres is twice the stored height byte. | The height byte is an **index into the land height table** in the region file. That table happens to be `2 × i` for its first 201 entries and then becomes non-linear, reaching 700.0. Using the doubling rule misplaces all high terrain — every mountain in the game is at the wrong altitude, and standable ground moves with it. | — |
| ACE's string-table reader | The table's data id field is two 16-bit counts; the variable-id list is comments; there is a language field. | It is a **single data id**, and the field ACE calls the language is really the table's version. ACE's decomposition works only because that id and the variable-name flag are zero in all shipped data, so a table that used either would be misread. The client never serialises its own language or description fields. | — |
| GDLE's string-table reader | Matches the client. | It does, except that it omits the `0xFFFF` long-length escape in the older pack string form. A string of 65,535 bytes or more is therefore misread. | — |

## 2. Physics and movement

| Reference | It says | The client does | Pinned by |
|---|---|---|---|
| ACE's physics copy of the terrain split | Computes the diagonal split of a terrain cell by dividing part of a hash by 2147483648. | The split is the **sign bit** of the 32-bit hash `x*y*0x0CCAC033 + y*0x6C1AC587 - x*0x421BE3BD - 0x519B8F25` over the global cell coordinates. ACE's *mesh* code computes the right value; its *physics* copy does not, so ACE disagrees with itself. Terrain rendering and standable ground both depend on this and the client uses one rule for both, so a port that inherits the physics copy has terrain you can see and cannot stand on. | — |
| ACE's split-direction accessor | Returns true for a north-west to south-east split. | The client's flag has the opposite sense: it means the shared edge runs south-west to north-east. The *value* agrees, the *convention* does not, so a naive port flips every terrain triangle pair. | — |
| ACE's older scenery placement, the one its landblock-mesh loader used (that call is now commented out) | Stubs the on-road test and the slope check. | Implements both: a 16-case road-edge geometry of width 5, and a real slope rejection. The placement hash matches, so that path's trees and bushes look nearly right and are **not** the client's. ACE's live scenery, the set its server collides with, is not this path: it has the full road test, the slope test, the strict block bounds and the shape-dependent within-block test, and over the shipped world it places the same objects as the client, aligned objects turned as the client turns them (to face down the slope, left upright); only its heading step poses a scene object whose base orientation is pitched off the client's angle (next row). | — |
| ACE's frame heading step | Turns a frame to a direction by the direction's compass heading about Z and passes the arcsine of its height as a turn about Y; setting a heading takes the new direction's height from the forward axis and the X axis together. | Pitches the frame by that angle about its own X axis, then turns it about Z, with no roll, and setting a heading keeps the forward axis's height alone. A scene object whose base orientation is pitched (in the final data files, 8 object descriptions of 6 setups, 0.1 to 2.2 degrees, 25,727 placed objects) keeps its pitch at every heading; ACE's copy leans it sideways (facing north or south) or pitches it the other way (facing east), up to 4.4 degrees from the client's pose, and agrees only for one facing west, which for a large rock formation moves its collision surface by up to about 0.7 m. No scene object of the final data files, or of the earlier ones checked back to 1999, has a base orientation whose X axis leaves the horizontal, so the second half changes nothing there. | `empyrean-world`: `a_pitched_scenery_rock_keeps_its_pitch_when_turned_to_its_heading`, `a_pitched_scenery_cylinder_object_keeps_its_pitch_rather_than_leaning_sideways`, `a_rolled_scene_object_turned_to_a_heading_keeps_its_forward_axis_level` |
| ACE's static cell search | Lists a static object only in its own landblock's land cells; follows a cell's portal only into a cell that cell itself sees; and finds the land cells a part's box covers from the part's own position. | Lists a static in every loaded cell its boxes reach, in any landblock, through any portal, measuring the boxes from the static's land cell. A creature or missile in a neighbouring landblock, or in a room past a portal, passes through part of an object ACE lists in too few cells, and a box-registered object ACE lists in a far cell is met there. | the same test: every difference from ACE's own lists in the sample |
| ACE's friction branch | Takes the low-friction sledding branch when the contact plane is nearly flat, at a threshold of about 0.99999536, tested in that direction. | Takes it when the contact plane normal's Z is **below** `cos(10°) = 0.98480775` — that is, on slopes *steeper* than about ten degrees. Both the direction of the test and the constant differ, so ACE slides where the client grips and grips where the client slides. | — |
| ACE's physics sub-step | Uses 0.1 s, and its own comments call the value suspect. | The maximum sub-step is **0.2 s**, and the whole world is skipped below a minimum of 1/30 s. The distribution of frame times is therefore part of the result, which is why a physics trace has to be driven with a fixed sequence of deltas. | — |
| ACE's default movement parameters | Set the "can charge" flag; the walk/run switch value is 1.0. | The default bitfield is `0x1EE0F` and the walk/run switch value is **15.0**. A server or reimplementation that uses 1.0 produces a visibly different gait. | — |
| ACE's physics-script table lookup | Stubbed. | Resolves a script type to a data id by **intensity threshold in file order**, with no randomisation on that path. The same type and intensity always select the same script. | — |
| ACE's movement limits | Enforces a 50 m/s speed cap and a Z-position anti-cheat. | Has no counterpart to either. They are server additions, and a client-side reimplementation that adds them will refuse legitimate motion. | — |
| The terrain level-of-detail stitching, as this project first described it | No guard exists at the ring-2/ring-3 or ring-4/ring-5 boundaries, so a crack there is possible in principle and faithful. | There **is** a guard and it closes both boundaries exactly: the averaged outward edge of a 4-cell block is the polyline its 2-cell neighbour draws, and of a 2-cell block the single span its 1-cell neighbour draws. The pass leaves no open seam in the window. | `dereth/client/tests/gpu/rendering/terrain_lod_seams.rs` **(local)**, measured over shipped landblocks to 1 mm |
| ACE's data-id packer | Emits the 2-byte short form for ids in `[0x4000, 0x7FFF]`. | Uses the **4-byte** form in that range. Both decode to the same id and the client accepts either, so neither side is wrong — it is recorded so that nobody later "fixes" this project's encoder to match ACE. Nine of forty-seven recorded payloads of one message differ this way and every one decodes identically. | `dereth-protocol` implements the client's packer |

## 3. Magic

| Reference | It says | The client does | Pinned by |
|---|---|---|---|
| ACE's spell-formula builder | Appends prismatic tapers to an unbounded list, so a spell is asked for one to four tapers purely by the power of its strongest scarab. | Caps the components at the formula's **eight slots**, and the surplus is dropped silently: the count actually charged is `min(tapers_by_power, 8 − scarabs)`. A seven-scarab spell asked for three tapers receives **one**. The rest of ACE's port is exact; only the cap is missing, so ACE burns more tapers than the client lists on the handful of multi-scarab spells. | `core/client-model/tests/dat/magic/scarab_and_taper_formula.rs` **(local)**, over all 6,266 shipped spells |
| This project's own earlier description of the school-to-foci mapping | The five school-to-item values could not be recovered without an emulator database dump. | They are ordinary rows of a mapper table in the client's own data, and the client reads them with an ordinary lookup. The consequence of getting it wrong is concrete: a character carrying a Foci was shown, and would have been charged, the full eight-slot formula instead of scarab plus prismatic tapers. | — |

## 4. Audio

| Reference | It says | The client does | Pinned by |
|---|---|---|---|
| ACE's tweaked-sound hook reader | Reads the two middle floats as priority then probability. | Reads them as **probability then priority**. The shipped data corroborates it independently: across all 541 tweaked-sound hooks — 173 in animations, 368 in physics scripts — the first float never exceeds 1.0 while the second reaches 3.0, which cannot be a probability. Swapping them silences or mis-ranks tweaked animation sounds. | — |

## 5. Networking

| Reference | It says | The client does | Pinned by |
|---|---|---|---|
| The community catalogue's data-patch end message | `0xF7EB` is the client-to-server end-of-patching message. | The end message is **`0xF7EA`** in both directions. `0xF7EB` is a separate, receive-only "patching pending, wait" message that the client never sends. ACE agrees with the client. | `core/client-net/tests/cpu/login/session_state.rs :: the_ddd_exchange_completes_and_replies_with_f7ea` |
| The community catalogue's base-qualities type | Data-id table flag `0x0040`, instance-id table flag `0x0008`. | The two are **swapped**: data-id is `0x0008` and instance-id is `0x0040`. ACE agrees with the client. | — |
| The community catalogue | The "remove player permission" action is `0x0220`. | The client sends **`0x021A`**. | — |
| The community catalogue's naming | `0xF745` is "create object" and `0xF7DB` is "update object". | The names are effectively backwards: `0xF745` **merges** into an existing object when the instance sequence matches, while `0xF7DB` always deletes and rebuilds. A reimplementation that takes the names at face value loses per-object state on every ordinary update. | — |
| The community catalogue's attacker/defender notifications (`0x01B1`, `0x01B2`) | The percentage field is a 32-bit float. | It is **eight bytes**, not four: reading it as a float shifts every subsequent field. ACE writes a double, and also writes the attack-conditions field as eight bytes where the client reads four, which leaves a trailing dword of slack that neither arm bounds-checks. All 47 recorded bodies leave exactly four zero bytes over; the recordings alone cannot choose between the two widths, because the high dword is zero in every one of them. The retail server wrote the same eight bytes (high dword zero in 41,767 of 41,767 retail-captured bodies), so the extra dword is the server's, and the client ignores it. | `core/client-net/tests/cpu/combat/attack_notification_width.rs` |
| The community catalogue's friends update (`0x0021`) | Carries a single friend structure. | Carries a **list** of them. | — |
| The community catalogue's fellowship update (`0x02C0`) | Begins with the fellow record. | Begins with an **object id** that the catalogue omits entirely. | — |
| The community catalogue's character title table (`0x0029`) | Is a display-title dword followed by a packed list of title ids. | A **version dword comes first**, and the client reads it only to skip it before reading the display title and then the list. Without it, the list's count is read out of the display title and the message overruns. Every recorded table is consistent with the corrected layout and consumes its payload exactly. | — |
| The community catalogue's add-to-trade (`0x0200`, server to client) | Two fields; the trade stamp is a float. | Three fields — there is a third dword — and the stamp is a **double**. | — |
| The community catalogue's plugin-query family | The payloads are empty. | All three carry payloads. | — |
| The community catalogue's stack-size update (`0x0197`) and house-restrictions update (`0x0248`) | Fields are dword-aligned like the rest of the protocol. | Both are **byte-packed**: an 8-bit sequence at offset 4 followed by an *unaligned* object id at offset 5. Reading them as aligned dwords corrupts every stack size and every restriction table. | — |
| ACE's damage-location enum | Defines values 0 to 8. | The client's table has **28 entries**, and the ones ACE is missing are reachable in normal play, so a port that stops at 8 prints the wrong body part for ordinary hits. | — |
| ACE's retransmit key recovery | Recovers a retransmitted packet's key by scanning up to 256 generated values forward. | Keys are **positional**: the receiver draws and stores the key for every sequence number it skips, and the sender reuses the stored key verbatim on the resend. ACE's scan is an approximation of the mechanism, not a reproduction of it, and a client that adopts the scan will desynchronise on a lost retransmission. | `core/transport/src/session.rs :: skipped_sequences_park_their_keys_and_retransmits_reuse_them`, `core/client-net/tests/cpu/net/retransmit.rs :: a_late_arrival_and_its_retransmission_do_not_desynchronise_the_key_stream` |
| The community catalogue's packed hash table | The dword header's bucket count is `1 << (packedSize >> 24)`. | The count in the low 24 bits is right; the bucket derivation is not. **Two** table classes share this wire shape and neither shifts. The newer one takes the top byte as an **index into a 23-prime bucket table**, clamped to 22; the older one takes `1 << (index − 1)` and rejects an index above `0x20`. Neither derivation is visible on the wire, but the *write* sides differ, so a rebuild that wants byte-identical packets must carry the header's top byte through rather than recompute it. | — |
| The community catalogue's house-restriction types | One shape each, beginning with a version of `0x10000002`. | Each has **three** shapes, keyed on the first dword: when its high word is zero, that dword *is* the bitmask and no monarch id follows; below `0x10000002` the guest table is the older hash form; only at or above it is the documented form used, and the access-restriction type then also reads a roommate list and aligns to 4 at the end. All shipped data uses the third shape, so the catalogue is right about what a live server sends and incomplete about what the client accepts. | — |
| This project's own earlier description of combat text | Bucket comparisons are strict `<`, so exactly 0.25 falls in the third bucket; two body-part rows render as "unknown"; the damage word is gated on the damage type being non-zero. | All three are wrong, and this project copied all three before catching them. The comparison is **`<=`**, so exactly 0.25 is the *second* bucket. The body-part table's bound is unsigned, so `-1` reaches row 0 and names *undefined*, while the gap rows and the default are lower-cased to *unknown*. And the damage word is gated on the composed *string*, not on the mask: three damage types have no name at all, and the joined list is written into a 64-byte buffer that nine names overflow, so a health drain printed an empty damage word with a doubled space. | `core/client-model/src/combat.rs :: the_hit_adjective_table_boundaries_belong_to_the_lower_bucket`, `:: damage_types_join_with_slashes_and_body_parts_lower_case` |
| ACE's object visibility | ACE's source says the client removes an object by itself once it has been out of visibility for 25 seconds. So when an object leaves the player's 3×3-landblock visibility window, ACE sends no delete. It drops the object from its own tracking 25 s later without telling the client. | The client has **no** visibility-range cull. Its 25-second destruction timer starts only when the object's landblock leaves the client's own loaded window (radius 8 landblocks by default, 3 at the lowest preset), or when an object cannot be placed. It never starts because the server has gone quiet. An object between two landblocks away and the client's window radius has been forgotten by ACE and is still drawn, frozen. If ACE destroys it then (for example a corpse decaying), the client is never told, and a ghost stays on screen until the player moves far enough away. This happens at every shipped preset, so it is the original's behaviour against ACE. A rebuild must not add its own time-to-live to hide it, because that deletes live objects the server has only stopped updating. To tell a ghost from a real object: using it does nothing at all, and examining it opens an empty panel. | — |
| The trade message names, as reimplementations read them | `0x01FE` "open trade" is what opens the trade window. | The window is opened by **`0x01FD` register trade**. `0x01FE` produces no visible change, and ACE never sends it. A client that waits for `0x01FE` never shows a trade against ACE. When a trade ends, `0x01FF` close empties both lists and clears the partner's name but **leaves the window open**. Only the player's own close button hides it. | — |

## 6. Login and character creation

| Reference | It says | The client does | Pinned by |
|---|---|---|---|
| ACE's character-create handler | (implied) a successful creation is followed by a fresh character-list message, which is what the wizard waits for so it can log the new character straight in. | ACE sends **only** the create response with an OK code, adds the character to the session and stops. No character list follows. So against ACE the client's auto-login branch never runs: the wizard stays on the summary page and the new character has to be entered from the character-select screen on the next connection. **Observed at runtime.** | `core/client-net/tests/cpu/login/new_character_joins_list.rs :: the_created_character_joins_the_list_without_a_second_character_set_message` |
| ACE's player factory | (robustness) an unusable creation request comes back as a verification response the client can display. | The template list is indexed with **no range check**, so a template number outside the list throws inside ACE's inbound message handler and the client is answered with **silence** — no response at all. The client's only guard is its own 110-second timeout, and its template field starts at −1, so the path is reachable. | `core/client-net/tests/cpu/login/session_state.rs :: no_player_description_within_a_hundred_and_ten_seconds_fires_server_died` |
| ACE's optional-header parser | Parses the optional sections the client may send. | It does not consume the error, error-disconnect, referral, logon-server-address or empty sections, so a client packet carrying one of those fails ACE's checksum verification and is dropped before it can be acted on. A client-sent disconnect-with-error therefore does nothing and ACE falls back on its 60-second session timeout. | — |
| ACE's connection defaults | — | ACE always stamps the connection iteration as `0x01`, so the "a newer iteration replaces the connection" path is never exercised against it, and ACE never sends a server switch, a referral or a logon-server address in normal operation. A rebuild tested only against ACE has not tested those paths at all. | `core/client-net/tests/cpu/net/referral.rs`, `core/transport/src/conn.rs :: a_newer_iteration_tears_down_and_rebuilds` |

## 7. Server-side divergences, reported rather than reproduced

**Provenance: reported.** These came from a parallel effort porting ACE's server behaviour rather
than from reading the client, and this project has not independently reproduced them. Corroboration
is not verification. They are recorded because anyone building against ACE will meet them.

| ACE behaviour | What is wrong with it | Consequence |
|---|---|---|
| Equipment-mask composites | Five composites are wrong; one sets a bit its own "all" composite says cannot exist. Four others are right. | The wield rule misses cloaks. |
| Encumbrance capacity | Two implementations exist and the one actually enforced lacks both of the client's guards. | Above five carrying-capacity augmentations, the server accepts a pickup the client's burden bar calls impossible. |
| Generator spawn counts | Clamps by `max − current` unconditionally, *including* when max is −1 meaning unlimited, which yields a negative count that the callers loop zero times over. | Every *unlimited* generator spawns nothing. |
| The ping request (`0x01E9`) | Never answered — two requests and zero replies across the recordings. | Every client's latency readout is wrong. This is a behavioural gap, not a wire-format difference. |
| Experience pass-through in allegiances | Computes the minimum of a value with itself four times, so the time term is always 1.0. | The time-sworn allegiance mechanic is pinned dead, under a comment block that spends four paragraphs on the patch that rebalanced it. |
| Death-item selection | Orders by value descending and then by category, which is correct only because the language's sort happens to be stable; the second call replaces the key rather than refining it. | Works today; ACE's own note asks for the refining form and is right to. |

### 7.1 The fellowship split, verified from the client

This one has a client half that **is** verified here, and it is the most expensive row on the page.

| members | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | more |
|---|---|---|---|---|---|---|---|---|---|---|---|
| share | 1.0 | 0.75 | 0.6 | 0.55 | 0.5 | 0.45 | 0.4 | 0.35 | **0.3111111** | 0.28 | **0.0** |

The tell that `0.3111111` is right and ACE's `0.30` is wrong is a **plateau**: `members × share` is
exactly **2.8** at seven, eight, nine and ten members — a fellowship's total payout stops growing past
seven people — and `0.3111111` is precisely `2.8 / 9`. ACE's `0.30` gives 2.70 and breaks the plateau.

The ten-member case is the serious one. The client's default arm returns **0.0**; ACE's switch covers
one to nine members and falls through to `1.0` for anything else. So where the client's rule pays a
ten-person fellowship `0.28` each, ACE pays **1.0 each** — ten times the earned experience, not a
rounding difference.

## 8. Adding a row

A row belongs here when a reference a reimplementer would reasonably trust says something the client
does not do, and when the difference has a consequence somebody could observe. Name the reference,
state the client's behaviour with its numbers, say what goes wrong if you believe the reference, and
name the test if one exists. A row with no consequence is a footnote, not a correction.
