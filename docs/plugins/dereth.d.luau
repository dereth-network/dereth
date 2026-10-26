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
	-- The portals this world's store knows: where each stands and sends, how it is known (learned, published, hint), its uses, failures and level range, and why it was retired.
	known_portals: () -> any,
	-- Where the character's recalls have sent it: each recall (`spell <id>`, `lifestone`, `marketplace`) and its landing.
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
