# The plugin reference

**Provenance: design.** An index of the reference pages. The pages marked *generated* are written
from the plugin interface's own code by the test
`dereth/testkit/tests/cpu/plugins.rs :: the_plugin_reference_is_what_the_code_generates`, which
fails when a page differs from what the code says; `DERETH_TEST_GOLDEN=1` rewrites them. They are
never edited by hand.

| page | what it lists | source |
|---|---|---|
| [operations.md](operations.md) | every host operation: its WIT signature, its Lua function where one is the operation one to one, and what it does | generated from the API's schema |
| [permissions.md](permissions.md) | every permission, what it covers and how it is granted; each automation domain's rule in a world with no policy and in a published one | generated from the permissions and the world policy |
| [capabilities.md](capabilities.md) | every host capability | generated from the capabilities |
| [ui-nodes.md](ui-nodes.md) | every UI node kind, its properties and the type each takes | generated from the UI tree's checks |
| [limits.md](limits.md) | the numbers the host holds every plugin to | generated from the host's constants |
| [conformance-cases.md](conformance-cases.md) | every conformance case by profile, what it proves and what it runs in | generated from the cases' own files |
| [../events.md](../events.md) | every event and the fields of its table, in delivery order | written by hand; a test holds its table to the host's list of events |
| [../actions.md](../actions.md) | every action: what it sends, what ends it, its timeout and its domain | written by hand |
| [../api.md](../api.md) | the Lua API: `on`, promises, handles, the libraries | written by hand |
| [../wit/dereth-plugin.wit](../wit/dereth-plugin.wit), [../dereth.d.luau](../dereth.d.luau) | the compiled-plugin interface and the Luau type definitions | generated from the API's schema |

The [specification](../README.md) states the rules these pages list; the [guide](../guide/README.md)
explains them with worked plugins.
