//! The world as the play screen's panels see it, gathered each frame from the client, the
//! player's scripts and the world as last drawn.
//!
//! Before each frame ([`Gather::prepare`], with the client to ask) the player's scripts are asked
//! what the panels read of them: the selection and the one before it, whether the player targets
//! what attacks it, the radar's ranges and the player's radar and names options (once, and again
//! when the options come back), how each object shows on the radar (once per object, and again
//! when the scripts say its look changed), and, when a selection key was pressed, which objects
//! are fellows and which the player may attack. The calls the panels asked of the scripts (stop
//! attacking, the options set) are made then too. Each frame ([`Gather::world`]) the client's
//! objects in the world (and what bodies there hold, where they are drawn), their qualities and
//! those answers, and the bodies the world was drawn with, make the panels' [`World`].

use std::collections::{BTreeMap, BTreeSet};

use auberean_client_sdk::model::Object;
use auberean_client_sdk::runtime::interface::function;
use auberean_client_sdk::runtime::InterfaceArg;
use auberean_client_sdk::Client;
use auberean_shell::interface::Game;
use auberean_ui_game::gameplay::Notice;
use auberean_ui_game::social::notice as social;
use auberean_ui_game::world::{Body, Camera, Thing, World};

/// Qualities the panels read: the kind bits, a pet's owner, what holds an item, the groove
/// level; whether it may be selected, whether it is dead.
mod quality {
    pub const KIND: u32 = 20;
    pub const GROOVE: u32 = 950;
    pub const OWNER: u32 = 505;
    pub const CONTAINER: u32 = 261;
    pub const SELECTABLE: u32 = 624;
    pub const DEAD: u32 = 279;
}

/// The player's scripts' notices the gathering hears: an object's radar look changed (about
/// the object; about the player, every object's); the player's options came back.
pub(crate) mod notice {
    pub const CHANGE_RADAR_LOOK: u32 = 0x0085 << 16 | 0x000A;
    pub const RELOAD_OPTIONS: u32 = 0x0085 << 16 | 0x0009;
    /// The options window changed the player's options.
    pub const OPTIONS_CHANGED: u32 = 0x46D;
}

/// A call the panels asked of the player's scripts, made before the next frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    CeaseAttack(u64),
    ShowNames(bool),
}

/// What the scripts said of one object.
#[derive(Clone, Debug, Default, PartialEq)]
struct Facts {
    /// Its name as its scripts write it, and the string reference they gave (asked once, and
    /// again when they say its radar look changed: a new name).
    name: Option<String>,
    name_info: Option<auberean_client_sdk::text::StringInfo>,
    kind: u32,
    groove: i8,
    owner: Option<u64>,
    container: Option<u64>,
    selectable: bool,
    dead: bool,
    /// How it shows on the radar (`None`: not on it); asked once.
    radar: Option<Option<([f32; 4], u32)>>,
    /// Fellow, may be attacked, its owner a fellow (asked for a selection key).
    fellow: bool,
    attackable: bool,
    owner_fellow: bool,
}

/// The world gathered for the panels.
#[derive(Debug, Default)]
pub struct Gather {
    facts: BTreeMap<u64, Facts>,
    selection: u64,
    last_selection: u64,
    auto_target: bool,
    should_auto_target: bool,
    ranges: Option<(f32, f32)>,
    options: Option<(bool, bool)>,
    /// The calls asked, for the next frame.
    pub asks: Vec<Ask>,
    /// A selection key was pressed: ask which objects are fellows and foes.
    pub want_targets: bool,
    /// The objects the last frame knew, to tell which went.
    known: BTreeSet<u64>,
    /// Objects' names as written, kept by object; and the string download a name waits on, while
    /// the string tables it reads are asked for (it shows no text until then).
    names: BTreeMap<
        u64,
        (
            auberean_client_sdk::protocol::text::StringInfo,
            String,
            Option<u32>,
        ),
    >,
    /// The fellows' shares of the experience, as the scripts last answered.
    fellow_xp: Vec<(u64, f32)>,
    /// The block the last world's places were in.
    block: (i32, i32),
}

/// Whether an object stands in the world where it can be drawn.
fn placed(o: &Object) -> bool {
    o.in_world && o.place.is_some() && o.parent.is_none() && o.container.is_none()
}

/// Whether the panels see an object: it stands in the world, or hangs from a body (a wielded
/// weapon), where it is drawn and picked as itself.
fn seen_by_panels(o: &Object) -> bool {
    placed(o) || o.parent.is_some()
}

/// A cell's block and a place in it on the one grid across every block.
#[allow(clippy::cast_precision_loss)]
fn global(cell: u32, origin: [f32; 3]) -> [f32; 3] {
    [
        f32::from(u8::try_from(cell >> 24).unwrap_or(0)) * 160.0 + origin[0],
        f32::from(u8::try_from((cell >> 16) & 0xFF).unwrap_or(0)) * 160.0 + origin[1],
        origin[2],
    ]
}

impl Gather {
    /// The options the panels read changed: ask them again.
    pub fn options_changed(&mut self) {
        self.options = None;
    }

    /// Ask the player's scripts what the panels read of them (see the module's description).
    pub fn prepare(&mut self, client: &mut Client, notices: &[Notice]) {
        let Some(player) = client
            .model
            .player
            .filter(|p| client.model.object(*p).is_some())
        else {
            self.facts.clear();
            self.ranges = None;
            self.options = None;
            self.fellow_xp.clear();
            return;
        };
        for a in std::mem::take(&mut self.asks) {
            match a {
                // D2-00425, D2-01275
                Ask::CeaseAttack(id) => {
                    client.interface(function::STOP_ATTACKING, &[InterfaceArg::Long(id)]);
                }
                // D2-00565, D2-01265
                // D2-00567, D2-01262
                Ask::ShowNames(on) => {
                    let w = InterfaceArg::Word(u32::from(on));
                    client.interface(function::SET_SHOW_OBJECT_NAMES, &[w]);
                    self.options = None;
                }
            }
        }
        for n in notices {
            match n.native {
                // The scripts say so when an object's name quality changes too: its name is
                // asked for again with its look.
                // D2-02135
                notice::CHANGE_RADAR_LOOK => match n.about {
                    Some(id) if id != player => {
                        if let Some(f) = self.facts.get_mut(&id) {
                            f.radar = None;
                            f.name = None;
                        }
                    }
                    _ => self.facts.values_mut().for_each(|f| {
                        f.radar = None;
                        f.name = None;
                    }),
                },
                notice::RELOAD_OPTIONS | notice::OPTIONS_CHANGED => self.options = None,
                _ => {}
            }
        }
        // The fellows' shares, asked again after the notices on which the fellowship's rows read
        // them: a fellowship or a fellow described, a fellow gone, a fellow's level.
        let fellows = [
            social::UPDATE_FELLOWSHIP,
            social::UPDATE_FELLOW,
            social::CLEAR_FELLOW,
            social::UPDATE_FELLOW_LEVEL,
        ];
        if notices.iter().any(|n| fellows.contains(&n.native)) {
            self.fellow_xp = client.fellow_xp();
        }
        let selection = client.interface_long(function::GET_SELECTION);
        if selection.as_ref().map(|(_, v)| *v) != Some(self.selection) {
            tracing::debug!(?selection, "the selection");
        }
        self.selection = selection.map_or(0, |(_, v)| v);
        self.last_selection = client
            .interface_long(function::GET_LAST_SELECTION)
            .map_or(0, |(_, v)| v);
        // D2-00579, D2-01109
        self.auto_target = client
            .interface(function::GET_AUTO_TARGET, &[])
            .is_some_and(|(_, v)| v != 0);
        self.should_auto_target = client.should_auto_target().is_some_and(|v| v != 0);
        if self.ranges.is_none() {
            self.ranges = client.radar_ranges();
        }
        if self.options.is_none() {
            // D2-00566, D2-01144, D2-00568, D2-01141
            let coords = client
                .interface(function::GET_SHOW_RADAR_COORDS, &[])
                .is_none_or(|(_, v)| v != 0);
            let names = client
                .interface(function::GET_SHOW_OBJECT_NAMES, &[])
                .is_some_and(|(_, v)| v != 0);
            self.options = Some((coords, names));
        }
        let ids: Vec<u64> = client
            .model
            .objects()
            .filter(|o| seen_by_panels(o))
            .map(|o| o.id)
            .collect();
        self.facts.retain(|id, _| ids.contains(id));
        for id in ids {
            let mut f = self.facts.get(&id).cloned().unwrap_or_default();
            if f.name.is_none() {
                // D2-00521
                let quantity = client.model.object(id).map_or(1, |o| {
                    let w = &o.weenie;
                    if w.mask & auberean_client_sdk::protocol::types::WeenieDesc::NUMBER_BITS[0]
                        != 0
                    {
                        u32::try_from(w.numbers[0]).unwrap_or(1)
                    } else {
                        1
                    }
                });
                f.name_info = client.object_name_info(id, quantity);
                f.name = f.name_info.as_ref().and_then(|i| client.write_info(i));
            }
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            {
                f.kind = client.int_quality(id, quality::KIND).unwrap_or(0) as u32;
                f.groove = client.int_quality(id, quality::GROOVE).unwrap_or(0) as i8;
            }
            f.owner = client
                .instance_quality(id, quality::OWNER)
                .filter(|o| *o != 0);
            f.container = client
                .instance_quality(id, quality::CONTAINER)
                .filter(|o| *o != 0);
            f.selectable = client
                .bool_quality(id, quality::SELECTABLE)
                .unwrap_or(false);
            f.dead = client.bool_quality(id, quality::DEAD).unwrap_or(false);
            if f.radar.is_none() {
                // D2-02049
                let look = client.radar_look(id, [0.0; 4], 0);
                tracing::debug!(id = format_args!("{id:#x}"), ?look, "radar look");
                f.radar = Some(
                    look.filter(|(_, shows, _, shape)| {
                        *shows != 0 && *shape != auberean_ui_game::world::NO_BLIP
                    })
                    .map(|(_, _, c, s)| (c, s)),
                );
            }
            if self.want_targets {
                // D2-00604, D2-01184, D2-00427, D2-01073
                let ask = |c: &mut Client, function: u32, id: u64| {
                    c.interface(function, &[InterfaceArg::Long(id)])
                        .is_some_and(|(_, v)| v != 0)
                };
                f.fellow = ask(client, function::IS_FELLOWSHIP_MEMBER, id);
                f.attackable = ask(client, function::CAN_AUTO_ATTACK_TARGET, id);
                f.owner_fellow = f
                    .owner
                    .is_some_and(|o| ask(client, function::IS_FELLOWSHIP_MEMBER, o));
            }
            self.facts.insert(id, f);
        }
        self.want_targets = false;
    }

    /// The panels' world this frame, and the objects that went since the last. Names are written
    /// by `write`; one whose string tables the data lacks waits on a string download of
    /// `strings` until it is done.
    pub fn world(
        &mut self,
        game: &Game<'_>,
        write: &dyn Fn(&auberean_client_sdk::protocol::text::StringInfo) -> String,
        strings: &dyn auberean_ui::Assets,
    ) -> Option<(World, Vec<u64>)> {
        let client = game.client?;
        let player = client.model.player_object()?;
        let s = game.status;
        let me = global(s.cell, [s.origin.x, s.origin.y, s.origin.z]);
        let (sin, cos) = (
            dereth_primitives::num::math::sinf(s.heading.to_radians()),
            dereth_primitives::num::math::cosf(s.heading.to_radians()),
        );
        let seen = game.seen;
        self.block = seen.block;
        #[allow(clippy::cast_precision_loss)]
        let block = (seen.block.0 as f32 * 160.0, seen.block.1 as f32 * 160.0);
        let mut things = Vec::new();
        let mut now = BTreeSet::new();
        for o in client.model.objects().filter(|o| seen_by_panels(o)) {
            now.insert(o.id);
            let drawn = seen.body(o.id);
            let at = match (drawn, &o.place) {
                (Some(b), _) => [b.origin.x + block.0, b.origin.y + block.1, b.origin.z],
                (None, Some(p)) => global(p.cell, [p.origin.x, p.origin.y, p.origin.z]),
                (None, None) => continue,
            };
            let d = [at[0] - me[0], at[1] - me[1], at[2] - me[2]];
            // D2-01972
            let from_player = [d[0] * cos - d[1] * sin, d[0] * sin + d[1] * cos, d[2]];
            let f = self.facts.get(&o.id).cloned().unwrap_or_default();
            let name = match (&f.name, self.names.get_mut(&o.id)) {
                (Some(n), _) => n.clone(),
                (None, Some((info, text, download))) if *info == o.weenie.name => {
                    if let Some(d) = *download {
                        if !strings.string_download_waiting(d) {
                            // Written as a text element writes it: a refused table shows the
                            // client's error message.
                            *download = None;
                            *text = String::from_utf16_lossy(&strings.element_text(
                                &auberean_client_sdk::runtime::text::from_wire(info),
                            ));
                        }
                    }
                    text.clone()
                }
                (None, _) => {
                    let download = strings.start_string_download(
                        &auberean_client_sdk::runtime::text::from_wire(&o.weenie.name),
                    );
                    let text = if download.is_some() {
                        String::new()
                    } else {
                        write(&o.weenie.name)
                    };
                    self.names
                        .insert(o.id, (o.weenie.name.clone(), text.clone(), download));
                    text
                }
            };
            things.push(Thing {
                id: o.id,
                is_player: o.id == player.id,
                from_player,
                selectable: f.selectable,
                kind: f.kind,
                owner: f.owner,
                container: f.container,
                dead: f.dead,
                radar: f.radar.flatten(),
                groove: f.groove,
                name,
                name_info: f.name_info.clone(),
                faded: drawn.is_some_and(|b| b.faded),
                body: drawn.map(|b| Body {
                    origin: [b.origin.x, b.origin.y, b.origin.z],
                    heading: b.heading,
                    bounds: b
                        .bounds
                        .map(|(lo, hi)| ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])),
                }),
                fellow: f.fellow,
                attackable: f.attackable,
                owner_fellow: f.owner_fellow,
            });
        }
        self.names.retain(|id, (_, _, download)| {
            let keep = now.contains(id);
            if let (false, Some(d)) = (keep, *download) {
                strings.release_string_download(d);
            }
            keep
        });
        // The objects that went: the panels listening on each hear that it is being deleted.
        // D2-04049, D2-08102
        let gone: Vec<u64> = self.known.difference(&now).copied().collect();
        self.known = now;
        let (coords, names) = self.options.unwrap_or((true, false));
        let region = &game.world.region.region;
        let world = World {
            player: player.id,
            player_kind: self.facts.get(&player.id).map_or(0, |f| f.kind),
            inside: s.cell & 0xFFFF >= 0x100,
            cell: s.cell,
            blocks: (region.blocks_long, region.blocks_wide),
            heading: s.heading,
            things,
            selection: self.selection,
            last_selection: self.last_selection,
            auto_target: self.auto_target,
            should_auto_target: self.should_auto_target,
            ranges: self.ranges.unwrap_or((50.0, 75.0)),
            show_coordinates: coords,
            show_names: names,
            portal_space: client.in_portal_space(),
            portal_drawn: s.portal_drawn,
            moving: s.moving,
            fellow_xp: self.fellow_xp.clone(),
            last_attacker: client.last_attacker,
            camera: seen.camera.map(|c| Camera {
                position: [c.position.x, c.position.y, c.position.z],
                heading: c.heading,
                pitch: c.pitch,
                fov_y: c.fov_y,
            }),
            picked: game.world_pick,
        };
        Some((world, gone))
    }
}

impl Gather {
    /// The camera the world was last drawn from (`seen`'s), in the block the last world's places
    /// were gathered in.
    #[must_use]
    pub fn camera_in_world(&self, seen: &auberean_scene::seen::Seen) -> Option<Camera> {
        let c = seen.camera?;
        #[allow(clippy::cast_precision_loss)]
        let shift = (
            (seen.block.0 - self.block.0) as f32 * 160.0,
            (seen.block.1 - self.block.1) as f32 * 160.0,
        );
        Some(Camera {
            position: [c.position.x + shift.0, c.position.y + shift.1, c.position.z],
            heading: c.heading,
            pitch: c.pitch,
            fov_y: c.fov_y,
        })
    }
}

/// The scripts' selection a reference names, as the names' panel reads it: its kind, then its six
/// numbers (current health, health lost to players, greatest health, current vigor, vigor lost to
/// players, greatest vigor); nothing when the scripts do not hold it.
#[must_use]
pub fn selection_info(client: Option<&Client>, reference: Option<u32>) -> Vec<u32> {
    use auberean_client_sdk::runtime::selection::Selection;
    use auberean_client_sdk::script::native_object::NativeObject;
    let (Some(scripts), Some(r)) = (client.and_then(|c| c.scripts.as_ref()), reference) else {
        return Vec::new();
    };
    let bytes = match scripts.vm.heap.native(r) {
        Ok(NativeObject::Packed { bytes, .. }) => bytes.clone(),
        _ => return Vec::new(),
    };
    let s = scripts
        .host
        .selections
        .get(&r)
        .copied()
        .filter(|w| w.pack() == bytes)
        .unwrap_or_else(|| Selection::unpack(&bytes));
    vec![
        s.kind,
        s.cur_health,
        s.pk_damage,
        s.max_health,
        s.cur_vigor,
        s.pk_vigor_loss,
        s.max_vigor,
    ]
}
