# The message stream

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A plugin that needs something the typed API does not give can read the game's messages
themselves: each message the client received or sent, decoded into its fields. The stream is read
only. There is no way to send a message through it, or any other way: what a plugin does to the
game goes through [actions](actions.md), as the game's own interface would ask.

Every plugin may listen (its manifest names `messages` to tell the player it does); it needs a
host with the `messages` capability (every Dereth host has it).

## Listening

```lua
messages.on("Item_SetAppraiseInfo", function(m)
  print(m.name, m.fields.object, m.fields.profile.success_flag)
end)
messages.on("any", function(m) count[m.dir] = (count[m.dir] or 0) + 1 end)
messages.on("0x02CD", function(m) end)   -- by type, in hexadecimal
```

`messages.on(name, fn)` is `on("messages." .. name, fn)`, and `messages.off(name, fn)` undoes it.
A message is named as the master opcode table names it (`Item_SetAppraiseInfo`,
`Qualities_PrivateUpdateInt`, `Movement_TurnToEvent`), by its type in hexadecimal, or `any` for
every one. Naming a message the table does not have is an error, and so is listening without the
permission (the error names it). A compiled plugin subscribes to the same event names.

Each message is one event, with this argument:

| field | what it is |
|---|---|
| `name` | the message's name, or `0x` and its type for one the table does not name |
| `opcode` | its type: the wrapped type of a game event or action, else the blob's first dword |
| `dir` | `received` (from the server) or `sent` (by the client) |
| `recipient` | the object a wrapped game event was addressed to, when the blob carries it |
| `codec` | the decoder that read it (`objects::ItemSetAppraiseInfo`), or nil when none reads that type that way or the one that does refused the bytes |
| `fields` | the decoded message, or nil with no decoder |

`fields` uses the decoder's own field names. A nested structure is a table with its fields and
`_t`, its type's name; an identifier (an object id, a data id) is its number; an optional value is
the value or nil; a list or a tuple is a list; a map is a table keyed by the key's text; a one-value
tuple of another name is `{ Name = value }`; a word-like value (an enumeration) is its name. A
value whose decoder writes it by hand is its text.

## Order and timing

A frame's messages are delivered first among its events: those received, in the order the client
applied them, then those the client sent during the previous frame. The events the typed API
derives from the same messages (an appraisal's `items.appraised`, a settled promise) follow in the
same frame, so a plugin always hears a message before what it changed.

The client keeps no copy of messages while no plugin listens to the stream.

## Versions

The stream is versioned on its own: `messages.version` (the `messages` field of `host.info`) is
`1`. Its field names come from the decoders, so they follow the protocol work rather than the API;
the version rises when a decoded message's fields change.
