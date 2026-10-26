# Services

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A plugin can offer the other plugins in the same client a **service**, and call theirs: a macro
offers "set the meta's state", a helper asks it. Plugins never share a VM and never reach each
other's values: a call is a request carried by the host, its arguments and its answer copied
across as data.

```lua
-- In the plugin that offers it (network.dereth.tank, say):
services.offer("set_meta_state", function(args, call)
  -- call.from: the calling plugin; call.gesture: whether the call followed the player's gesture
  meta:set_state(args.state)
  return meta.state
end)

-- In a plugin that calls it (it needs the plugins.call permission):
local state = await(services.call("network.dereth.tank", "set_meta_state", { state = "Hunt" }))
for _, s in ipairs(services.list()) do print(s.plugin, s.service) end
```

| function | what it does |
|---|---|
| `services.offer(name, fn [, { allow = { ids } }])` | offer a service: `fn(args, call)` answers each call; what it returns is the answer, and an error it raises fails the call. With `allow`, only the plugins listed may call it |
| `services.withdraw(name)` | stop offering it |
| `services.call(plugin, name [, args])` | a promise of the answer; needs `plugins.call` |
| `services.list()` | the services this plugin may call: `{ plugin, service }` each |

A name is letters, digits, `_`, `-` and `.`, at most 64; a plugin offers at most 64. A call reaches
the provider at the next frame as a `service.call` event (`id`, `from`, `service`, `args`,
`gesture`), which the Lua interface answers with the offered function; a compiled plugin answers it
with the `services.reply` operation. The answer settles the caller's promise the frame after. A
call fails at once as `no_plugin` (no such plugin loaded and active here), `no_service` (it offers
no such service) or `not_allowed` (its `allow` leaves the caller out); unanswered for five seconds
it fails as `timeout`; a provider that unloads fails the calls it has not answered. Arguments and
answers are at most 64 KiB of JSON, and a plugin makes at most 20 calls a second.

Nothing of the caller's passes to the provider but the data: its permissions do not, and neither
does its gesture. The provider acts under its own permissions and the world's policy; it hears
whether the call followed the player's gesture (`call.gesture`) as a fact to decide by, not as a
gesture of its own.

**The Tank's services.** The bundled Tank offers `running` (whether the macro is running; the
bundled Looter asks it, and waits while it is, so the two never loot one corpse),
`get_meta_state`, `set_meta_state` (`{ state }`: enters a state of the meta it runs),
`get_setting` (a setting's name) and `set_setting` (`{ name, value }`), the hooks VTank gave
other plugins.
