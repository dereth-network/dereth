# Third-party code in the plugin host and its samples

**Provenance: design.** The plugin interface is this client's own addition, not the original's.

## VTClassic (loot profiles)

The loot profile reader and matcher (`dereth.loot`, in `core/plugin-host/src/loot.rs`) is ported
from VTClassic, the loot plugin for the retail client's Virindi Tank, under its MIT licence:

```text
This file is Copyright (c) 2009-2010 VirindiPlugins

Permission is hereby granted, free of charge, to any person obtaining a copy
  of this software and associated documentation files (the "Software"), to deal
  in the Software without restriction, including without limitation the rights
  to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
  copies of the Software, and to permit persons to whom the Software is
  furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
  all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
  IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
  FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
  AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
  LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
  OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
  THE SOFTWARE.
```

## UtilityBelt (the VTank route format)

`dereth.nav`'s reader of VTank `.nav` route files (`core/plugin-host/src/navfile.rs`) follows the
file layout as UtilityBelt's route reader reads it (UtilityBelt is MIT-licensed); the format is
described in [nav.md](nav.md). No code is copied.

## UtilityBelt's script API (UtilityBelt scripts)

The host's library for scripts written for UtilityBelt's scripting (`dereth.ubscript`, in
`core/plugin-lua/src/ubscript/`) matches the API of UtilityBelt.Scripting and of
UtilityBelt.Service's script modules (`utilitybelt.views`, `imgui`, `acclient`, `filesystem`,
`ubnet`, `http`), under their MIT licence: the names and shapes of its classes, members, events
and actions, the actions' types and default priorities, retries and timeouts. Its enumerations
(`ubscript/enums.lua`) are generated from those of UtilityBelt.Scripting and UtilityBelt.Common,
also MIT. The code is this library's own, written over the plugin interface; no code is
translated. The [description](ubscript.md) says what each part is here.

```text
MIT License

Copyright (c) 2022 utilitybelt

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

The ImGui flag values the `imgui` module gives (`ImGuiWindowFlags` and the rest) are Dear ImGui's,
under its MIT licence (Copyright (c) 2014-2024 Omar Cornut).

## Alinco (the Buffbot's own buffs, ID Notices)

The Buffbot sample's self profiles (`dereth/plugins/buffbot/selfbuffs.lua`) and ID Notices' lists
of names, scroll notices and alerts (`dereth/plugins/id-notices/`) follow what Alinco3 and its
AlincoBuffs do for a player, under their MIT licence; no code is translated.

```text
Copyright (c) 2015 Edward Kozlowski

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```

## Mag-Tools (the Inventory sample)

The Inventory sample (`dereth/plugins/inventory/`) does what Mag-Tools' inventory logger, item
information, stacking, packing and trading by a vendor's profile do for a player, written for
Dereth from that behavior alone: Mag-Tools is under the LGPL, and none of its code was read,
copied or translated.

## SuitBuilder (the Inventory sample's suits)

The Inventory sample's suit builder (`dereth/plugins/inventory/suits.lua`) does what SuitBuilder
does for a player, the best suit from every character's logged gear, written for Dereth from that
behavior alone: SuitBuilder is under the LGPL, and none of its code was read, copied or
translated.

## VTank metas, meta views and expressions

`dereth.meta`'s import of VTank `.met` files (`core/plugin-host/src/metfile.rs`), its meta views
and `dereth.expr`'s expression language follow the formats as they appear in meta files players
publish; no program's code was read or copied. What is carried is described in
[automation.md](automation.md).

## The script engine

- **Luau** (MIT, Roblox Corporation), built from source by the `luau0-src` crate.
- **mlua** (MIT), carried in `vendor/mlua` with its licence and one change for the browser build
  (`vendor/mlua/Cargo.toml` says what).
- In the browser build only, the C++ exception runtime and a few out-of-line parts of the C++
  standard library, compiled from the LLVM project's libc++, libc++abi and libunwind sources that
  zig ships (Apache-2.0 with LLVM exceptions).
