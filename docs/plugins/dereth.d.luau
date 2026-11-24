-- Luau definitions of the Dereth plugin API's functions that are host operations one to
-- one. Generated from the plugin API's schema, the same schema as the WIT interface.
-- Do not edit: regenerate it (`docs/plugins/wasm.md` says how).

declare chat: {
	-- Print a line in the chat window, for the player only; `style` is a chat kind.
	print: (text: string, style: string?) -> (),
	-- Say a line, or type a command, as the player would in the chat bar.
	say: (text: string) -> (),
}

declare commands: {
	-- Give up a claimed `/verb`.
	remove: (verb: string) -> (),
}

declare files: {
	-- A text file of the plugin's own package.
	package: (path: string) -> string?,
	-- Every file of the plugin's own package, by path.
	package_list: () -> any,
	-- A text file of the plugin's data folder (the player's files for it), or null.
	data: (path: string) -> string?,
	-- Every file in the plugin's data folder, by path.
	data_list: () -> any,
	-- Save a text file to the plugin's data folder (at most 4 MiB, 1000 files).
	save_data: (path: string, text: string) -> boolean,
}

declare game: {
	-- The character's own integer property `key`, or null.
	player_int: (key: number) -> any,
}

declare magic: {
	-- A spell of the spell table, or null.
	spell: (spell: number) -> any,
	-- Every spell of the spell table with this name, case aside, as `magic.spell` gives each: the ones the character knows first, then the rest, each in id order. One name can be several spells.
	find: (name: string) -> any,
	-- The character's spell book, in id order: every spell it knows that the spell table has, or `count` of them from the `from`th (from 1).
	known: (from: number?, count: number?) -> any,
}

declare nav: {
	-- The plugin moving the character, or none.
	owner: () -> string?,
	-- The journey this plugin has under way (its legs, the leg being carried out, where that leg goes, the way still to go, the goal), or null.
	journey: () -> any,
	-- Show or hide the host's drawing of this plugin's journey (the arrow, the way on the map and radar); false when it has none.
	show_journey: (on: boolean) -> boolean,
	-- The portals and places the client knows whose names have words beginning with each word of `text` (every one when it is empty), nearest first: `{ total, items }`, the nearest `limit` (8 unless given) of the `total` there are, each as `nav.known_portals` or `nav.known_places` gives it with `list` (`portal` or `place`). `opts`: `kind`, a kind or a list of kinds; `portals` and `places` (false leaves that list out); `near` and `within` as `nav.known_portals` takes them. Indexed by the host, so it costs the few it answers, not the world's thousands.
	search: (text: string, opts: any?) -> any,
	-- How many portals and places the client knows of this world, by where each was learned: `{ portals = { learned, published, hint }, places = { data, learned, published, hint } }`, a source left out when none is known from it. `hint` is the era's package; `data`, the places the game's data names.
	knowledge: () -> any,
	-- The portals this world's store knows: where each stands and sends, how it is known (learned, published, hint), its uses, failures and level range, and why it was retired. A `filter` keeps those whose name holds `name`, within `within` meters of `near` (the character when left out), nearest first when measured, at most `limit`.
	known_portals: (filter: any?) -> any,
	-- The places the client knows of this world: the towns, landmarks and houses the game's data names, and each lifestone, portal, vendor, person and house it was shown or the server publishes, where it stands, how it is known, when it was seen, and a house's owner. A `filter` as `nav.known_portals` takes, and `kind`, a kind or a list of kinds.
	known_places: (filter: any?) -> any,
	-- Where the character's recalls land it: each recall (`spell <id>`, `lifestone`, `marketplace`, `house`, `mansion`, `allegiance`) and its landing, learned from its uses and from what sets it (a tie, a lifestone, the last portal taken).
	recalls: () -> any,
}

declare permissions: {
	-- Seconds left of the activation the player's last gesture gave.
	activation: () -> number,
	-- Whether the plugin may use what a permission covers: every one but net.http, which its manifest must name.
	has: (permission: string) -> boolean,
}

declare policy: {
	-- The world's plugin policy.
	info: () -> any,
}

declare regex: {
	-- The first match of a pattern in a text, with its groups, or null.
	match: (pattern: string, text: string, ignore_case: boolean) -> any,
}

declare services: {
	-- The services this plugin may call: `{ plugin, service }` each.
	list: () -> any,
}

declare session: {
	-- The session: its phase, the character list, portal space.
	info: () -> any,
}

declare task: {
	-- Cancel a timer.
	cancel: (timer: number) -> (),
}

declare trade: {
	-- The open trade, or null.
	info: () -> any,
}

declare ui: {
	-- Show a line briefly at the top of the screen with the plugin's name (1 to 30 seconds, 5 when left out; three at once a plugin).
	toast: (text: string, seconds: number?) -> (),
	-- What the plugin bar shows the plugin doing: running, paused or idle.
	status: (status: string) -> (),
}

declare vendor: {
	-- The open vendor, or null.
	info: () -> any,
}
