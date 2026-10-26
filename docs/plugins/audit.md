# The plugin audit

**Provenance: wire format** for the four messages and the answer with no plugin loaded, which is the original client's; **design** for the answer with plugins loaded.

The game's protocol lets a server ask a client which plugins it is running:

| message | direction | payload |
|---|---|---|
| `Admin_QueryPluginList` (`0x02AE`) | server to client | `u32 context` |
| `Admin_QueryPluginListResponse` (`0x02AF`) | client to server | `u32 context`, `string pluginList` |
| `Admin_QueryPlugin` (`0x02B1`) | server to client | `u32 context`, `string pluginName` |
| `Admin_QueryPluginResponse` (`0x02B2`) | client to server | `u32 context`, `i32 success`, `string name`, `string author`, `string email`, `string webpage` |

Each answer echoes the question's context.

**With no plugin loaded** a host answers as the original client does when no third-party interface
is running: the list is `3rd party API not in use.`, and a question about one plugin is answered
with `success` 0, the name that was asked, and `Unknown` in the other three fields.

**With plugins loaded** a host answers truthfully:

- the list is one line per loaded plugin, `id version`, separated by line feeds;
- a question naming a loaded plugin (by id or by name) is answered with `success` 1; `name` is
  `name version (id)`; `author` the manifest's authors, comma-separated; `email` the permissions
  its manifest names, `permissions: chat.send, files.import` (`permissions: none` when it names
  none; the field has no other use here);
  `webpage` the manifest's homepage, or `Unknown`;
- a question naming no loaded plugin is answered as with none loaded.

A modified client can lie. The audit is honest only in an unmodified one; enforcement against a
modified client is the server's.
