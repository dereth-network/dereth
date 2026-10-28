# A world's plugin policy

**Provenance: design.** The plugin interface is this client's own addition, not the original's; every statement here is what a host must do, and the conformance suite pins it.

A world says which kinds of plugin automation it allows in a small document, which the client
honours, tells the player (`/plugin policy`), and which
plugins read (`policy.info()`, and the `session.world_policy` event when it changes). The client cannot enforce anything against a modified client; the server keeps
real enforcement. The policy is what an unmodified client respects and tells the player.

```toml
policy_version = 1
world = "Example World"

[automation]              # per domain: "allow" | "interactive-only" | "deny"
loot = "allow"
buff = "allow"
heal = "interactive-only"
combat = "deny"
nav = "interactive-only"
vendor = "allow"
craft = "allow"
trade = "interactive-only"
social = "deny"
support = "allow"         # beneficial spells cast on other characters: a buffbot
session = "allow"         # a plugin choosing a character, logging in and out
default = "deny"          # a domain the document does not name (deny when absent)

[limits]
peers_local = 3           # other clients on the machine a plugin may hear from (0 = none)
pk_automation = false     # automation while the character is a player killer
read_parity = false       # plugins see only what the radar shows

[notes]
text = "Unattended macroing is not allowed; see our rules page."
url = "https://example.org/rules"
```

`support` is beneficial spells cast on another character (a buffbot's buffs, a helper's
protections), apart from `combat`, so a world that forbids unattended fighting can still allow a
buffbot; a heal asked for as one is `heal` (the domains: [permissions.md](permissions.md#gestures-and-automation)).

`allow` lets an action in the domain run with or without a gesture, `interactive-only` only with a
live gesture (see [permissions](permissions.md#gestures-and-automation)), `deny` never; a refused
action's promise fails with `denied_by_world`. Keys a later version adds are ignored; a
known key of the wrong kind, or an unknown rule, refuses the document, and the client then applies
the policy of a world that publishes none and says why. The host enforces `pk_automation` and
`read_parity` as [automation.md](automation.md) says.

**Where it comes from.** A world publishes it in its world entry (`plugins.policy` or
`plugin_policy`), inline or as an `https://` URL; the launcher hands it to the Dereth client
(`--plugin-policy <file or URL>`). The browser client takes the URL from its page
(`?plugin_policy=https://…`), and the headless client from `--plugin-policy <file>`.

**A world that publishes none** allows everything: every domain and `session` `allow`, with or
without a gesture, automation while a player killer too, and no cap on peers. A world that wants
less publishes a policy.
