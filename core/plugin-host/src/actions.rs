//! The action queue: what plugins ask the game to do, sent one request at a time and settled when
//! the client sees how it ended.
//!
//! A plugin's action becomes the requests the retail interface makes for the same gesture
//! ([`UiRequest`]), so it passes the same checks and sends the same messages. The queue holds every
//! plugin's actions in the order they were asked for and sends the next only when nothing of
//! theirs is outstanding and the client itself is not busy with an item (the interface's own rule:
//! one item request at a time). Each action has a completion signal, read from the frame (an
//! appraisal delivered, an item where it was sent, a use acknowledged, a vendor's answer), and a
//! timeout; its promise is settled with the one that comes first.

use std::collections::{BTreeMap, VecDeque};

use dereth_client_contract::view::{InventoryIntent, PlayerOption, Vital};
use dereth_client_contract::UiRequest;
use dereth_client_runtime::frame_hook::{HookFrame, Outcome};
use dereth_plugin_api::{AutoDomain, CallError, Value};
use dereth_primitives::ObjectId;

/// How long a gesture's activation lasts, in seconds of the frame clock.
pub const ACTIVATION_SECS: f64 = 30.0;
/// The most appraisals one plugin may ask for in one second.
pub const APPRAISALS_PER_SECOND: usize = 5;
/// The longest an action waits in the queue before it fails as `busy`, in seconds.
pub const QUEUE_WAIT_SECS: f64 = 30.0;
/// The most actions one plugin may have queued.
pub const MAX_QUEUED: usize = 256;

/// What an action does, with what its completion signal compares against.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Select(ObjectId),
    Use(ObjectId),
    UseOn {
        item: ObjectId,
        target: ObjectId,
    },
    Appraise(ObjectId),
    Move {
        item: ObjectId,
        container: ObjectId,
    },
    Split {
        item: ObjectId,
        container: ObjectId,
    },
    Merge {
        item: ObjectId,
        target: ObjectId,
    },
    Drop(ObjectId),
    Give {
        item: ObjectId,
        target: ObjectId,
    },
    Wield(ObjectId),
    Inscribe(ObjectId),
    Salvage(Vec<ObjectId>),
    VendorBuy,
    VendorSell,
    VendorClose,
    FillComponents,
    TradeOpen(ObjectId),
    TradeAdd(ObjectId),
    TradeAccept,
    TradeDecline,
    TradeReset,
    TradeClose,
    FellowshipCreate,
    FellowshipQuit,
    FellowshipRecruit(ObjectId),
    FellowshipDismiss(ObjectId),
    FellowshipLeader(ObjectId),
    FellowshipOpenness,
    FriendAdd(String),
    FriendRemove(ObjectId),
    RaiseSkill(u32),
    RaiseAttribute(u32),
    RaiseVital(u32),
    SetOption(PlayerOption, bool),
    Cast(u32),
    /// Into combat (`true`) or back to peace.
    CombatMode(bool),
    Attack(ObjectId),
}

impl Kind {
    /// The action's name, as the plugin called it.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Select(_) => "act.select",
            Self::Use(_) => "act.use",
            Self::UseOn { .. } => "act.use_on",
            Self::Appraise(_) => "act.appraise",
            Self::Move { .. } => "items.move",
            Self::Split { .. } => "items.split",
            Self::Merge { .. } => "items.merge",
            Self::Drop(_) => "items.drop",
            Self::Give { .. } => "items.give",
            Self::Wield(_) => "items.wield",
            Self::Inscribe(_) => "items.inscribe",
            Self::Salvage(_) => "items.salvage",
            Self::VendorBuy => "vendor.buy",
            Self::VendorSell => "vendor.sell",
            Self::VendorClose => "vendor.close",
            Self::FillComponents => "vendor.fill_components",
            Self::TradeOpen(_) => "trade.open",
            Self::TradeAdd(_) => "trade.add",
            Self::TradeAccept => "trade.accept",
            Self::TradeDecline => "trade.decline",
            Self::TradeReset => "trade.reset",
            Self::TradeClose => "trade.close",
            Self::FellowshipCreate => "social.fellowship_create",
            Self::FellowshipQuit => "social.fellowship_quit",
            Self::FellowshipRecruit(_) => "social.fellowship_recruit",
            Self::FellowshipDismiss(_) => "social.fellowship_dismiss",
            Self::FellowshipLeader(_) => "social.fellowship_leader",
            Self::FellowshipOpenness => "social.fellowship_openness",
            Self::FriendAdd(_) => "social.friend_add",
            Self::FriendRemove(_) => "social.friend_remove",
            Self::RaiseSkill(_) => "character.raise_skill",
            Self::RaiseAttribute(_) => "character.raise_attribute",
            Self::RaiseVital(_) => "character.raise_vital",
            Self::SetOption(..) => "character.set_option",
            Self::Cast(_) => "magic.cast",
            Self::CombatMode(_) => "combat.set_mode",
            Self::Attack(_) => "combat.attack",
        }
    }

    /// Whether the client's item lock must be free before it is sent, and whether it takes the
    /// lock itself.
    fn uses_item_lock(&self) -> bool {
        matches!(
            self,
            Self::Move { .. }
                | Self::Split { .. }
                | Self::Merge { .. }
                | Self::Drop(_)
                | Self::Give { .. }
                | Self::Wield(_)
                | Self::VendorBuy
                | Self::VendorSell
        )
    }

    /// Whether it waits for the client's use acknowledgement to be free (no use or cast
    /// outstanding).
    fn waits_for_use(&self) -> bool {
        matches!(
            self,
            Self::Use(_) | Self::UseOn { .. } | Self::Cast(_) | Self::Salvage(_)
        )
    }

    /// How long, in seconds after it is sent, before it fails as `timeout`.
    fn timeout(&self) -> f64 {
        match self {
            Self::Select(_)
            | Self::Inscribe(_)
            | Self::FillComponents
            | Self::VendorClose
            | Self::SetOption(..) => 2.0,
            Self::Appraise(_) | Self::CombatMode(_) => 5.0,
            // A target out of reach is walked to first; the server gives that 15 s.
            Self::Attack(_) => 20.0,
            // Taking an item from the world (a corpse, a chest, the ground) is walked to and
            // picked up by the server, and waits while the character is busy.
            Self::Move { .. } => 20.0,
            // A use on a target (a healing kit on oneself, say) waits on the server's walk to the
            // target and on the motion; a kit used in a fighting stance takes some ten seconds.
            Self::UseOn { .. } => 20.0,
            Self::TradeOpen(_) => 20.0,
            _ => 10.0,
        }
    }
}

/// An action, planned: what it is, the requests that carry it out and its automation domain.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub kind: Kind,
    pub requests: Vec<UiRequest>,
    /// The automation domain an action with no gesture behind it needs; `None` for an action any
    /// plugin may take at any time (a selection, an appraisal).
    pub domain: Option<AutoDomain>,
    /// Actions no plugin may take without a gesture.
    pub gesture_only: bool,
}

fn bad(msg: impl Into<String>) -> CallError {
    CallError::BadArgs(msg.into())
}

fn id_of(v: &Value, what: &str) -> Result<ObjectId, CallError> {
    v.as_i64()
        .and_then(|i| u32::try_from(i).ok())
        .filter(|i| *i != 0)
        .map(ObjectId)
        .ok_or_else(|| bad(format!("{what} must be an object id")))
}

fn ids_of(v: &Value, what: &str) -> Result<Vec<ObjectId>, CallError> {
    let list = v
        .as_list()
        .ok_or_else(|| bad(format!("{what} must be a list of object ids")))?;
    list.iter().map(|e| id_of(e, what)).collect()
}

fn count_of(v: &Value, what: &str) -> Result<Option<u32>, CallError> {
    if v.is_nil() {
        return Ok(None);
    }
    v.as_i64()
        .and_then(|i| u32::try_from(i).ok())
        .filter(|i| *i > 0)
        .map(Some)
        .ok_or_else(|| bad(format!("{what} must be a whole number above 0")))
}

/// Whether `item` sits in the corpse or chest the player has open, rather than with the player.
fn in_external_container(frame: &HookFrame<'_>, item: ObjectId) -> bool {
    let w = frame.world;
    let Some(ground) = w.ground_object else {
        return false;
    };
    let mut at = w.weenie(item).and_then(|x| x.pwd.container_id);
    for _ in 0..4 {
        match at {
            Some(c) if c == ground => return true,
            Some(c) => at = w.weenie(c).and_then(|x| x.pwd.container_id),
            None => return false,
        }
    }
    false
}

/// The player's option named `name` (`view_combat_target`, `auto_repeat_attack`, …).
fn option_named(name: &str) -> Option<PlayerOption> {
    PlayerOption::ALL
        .into_iter()
        .find(|o| option_name(*o) == name)
}

/// A player option's name as plugins spell it: `view_combat_target`, `auto_repeat_attack`, ….
#[must_use]
pub fn option_name(o: PlayerOption) -> String {
    let debug = format!("{o:?}");
    let mut snake = String::new();
    for (i, c) in debug.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                snake.push('_');
            }
            snake.push(c.to_ascii_lowercase());
        } else {
            snake.push(c);
        }
    }
    snake
}

/// The spell table's flag of a spell that helps its target (a buff, a protection, a heal).
const SPELL_BENEFICIAL: u32 = 0x4;

/// Plan the action `name` with `args`, against the frame's state.
///
/// # Errors
/// Unknown action, or arguments that do not make one.
#[allow(clippy::too_many_lines)]
pub fn plan(name: &str, args: &Value, frame: &HookFrame<'_>) -> Result<Plan, CallError> {
    let arg = |k: &str| args.get(k).clone();
    let player = frame
        .world
        .player
        .ok_or_else(|| CallError::Failed(format!("{name} needs a character in the world")))?;
    let view = frame.view;
    let one = |kind: Kind, r: UiRequest, domain: Option<AutoDomain>| Plan {
        kind,
        requests: vec![r],
        domain,
        gesture_only: false,
    };
    let loot_or_craft = |item: ObjectId| {
        if in_external_container(frame, item) {
            AutoDomain::Loot
        } else {
            AutoDomain::Craft
        }
    };
    // Taking up or putting away a weapon, a caster or ammunition is part of fighting (a macro
    // swaps them by the monster); anything else worn is the inventory's business.
    let combat_or_craft = |item: ObjectId| {
        use dereth_rules::weenie::item_type;
        let t = frame.world.weenie(item).map_or(0, |w| w.pwd.obj_type);
        if t & (item_type::MELEE_WEAPON | item_type::MISSILE_WEAPON | item_type::CASTER) != 0 {
            AutoDomain::Combat
        } else {
            AutoDomain::Craft
        }
    };
    // A spell domain named as an action's purpose (`buff`, `heal` or `support`): the magic stance
    // and its caster serve the spells cast in it, so a world that allows those spells and forbids
    // fighting lets a plugin take up its caster and enter the stance for them.
    let spell_purpose = |what: &str| -> Result<Option<AutoDomain>, CallError> {
        match arg("purpose").as_str() {
            None => Ok(None),
            Some("buff") => Ok(Some(AutoDomain::Buff)),
            Some("heal") => Ok(Some(AutoDomain::Heal)),
            Some("support") => Ok(Some(AutoDomain::Support)),
            Some(p) => Err(bad(format!(
                "{what}'s purpose is \"buff\", \"heal\" or \"support\", not {p:?}"
            ))),
        }
    };
    let is_caster = |item: ObjectId| {
        frame
            .world
            .weenie(item)
            .is_some_and(|w| w.pwd.obj_type & dereth_rules::weenie::item_type::CASTER != 0)
    };
    let is_weapon = |item: ObjectId| {
        use dereth_rules::weenie::item_type;
        frame.world.weenie(item).is_some_and(|w| {
            w.pwd.obj_type
                & (item_type::MELEE_WEAPON | item_type::MISSILE_WEAPON | item_type::CASTER)
                != 0
        })
    };
    let stack_of = |item: ObjectId| {
        frame
            .world
            .weenie(item)
            .and_then(|w| w.pwd.stack_size)
            .map_or(1, u32::from)
    };
    Ok(match name {
        "act.select" => {
            let id = id_of(&arg("id"), "act.select's id")?;
            one(Kind::Select(id), UiRequest::Select(id), None)
        }
        "act.appraise" => {
            let id = id_of(&arg("id"), "act.appraise's id")?;
            one(Kind::Appraise(id), UiRequest::Appraise(id), None)
        }
        "act.use" => {
            let id = id_of(&arg("id"), "act.use's id")?;
            let w = frame.world.weenie(id);
            let ty = w.map_or(0, |w| w.pwd.obj_type);
            let bits = w.map_or(0, |w| w.pwd.bitfield);
            let domain = if ty & dereth_rules::weenie::item_type::CONTAINER != 0
                || bits & dereth_rules::weenie::bitfield::CORPSE != 0
            {
                AutoDomain::Loot
            } else {
                AutoDomain::Nav
            };
            one(Kind::Use(id), UiRequest::Use(id), Some(domain))
        }
        "act.use_on" => {
            let item = id_of(&arg("item"), "act.use_on's item")?;
            let target = id_of(&arg("target"), "act.use_on's target")?;
            let w = frame.world.weenie(item);
            let healer =
                w.is_some_and(|w| w.pwd.bitfield & dereth_rules::weenie::bitfield::HEALER != 0);
            // A wielded item is used where it is, as the interface for plugins uses an equipped
            // item: its own spell cast (a caster's), never the item put in the pack.
            let equipped = w.is_some_and(|w| w.pwd.wielder_id == Some(player));
            let purpose_heal = match arg("purpose").as_str() {
                None => false,
                Some("heal") => true,
                Some(p) => return Err(bad(format!("a use's purpose is \"heal\", not {p:?}"))),
            };
            // A wielded item's spell is a cast: on oneself or what one holds `buff`, on anyone
            // else `support` when it helps and `combat` when not, and asked for as a heal `heal`.
            let domain = if healer || (equipped && purpose_heal) {
                AutoDomain::Heal
            } else if equipped {
                let mine = target == player
                    || frame.world.weenie(target).is_some_and(|t| {
                        t.pwd.wielder_id == Some(player) || t.pwd.container_id == Some(player)
                    });
                let beneficial = w
                    .and_then(|w| w.pwd.spell_id)
                    .and_then(|s| view.spell(u32::from(s)))
                    .is_some_and(|s| s.bitfield & SPELL_BENEFICIAL != 0);
                if mine {
                    AutoDomain::Buff
                } else if beneficial {
                    AutoDomain::Support
                } else {
                    AutoDomain::Combat
                }
            } else {
                AutoDomain::Craft
            };
            one(
                Kind::UseOn { item, target },
                UiRequest::UseOn {
                    item,
                    target,
                    equipped,
                },
                Some(domain),
            )
        }
        "items.move" => {
            let item = id_of(&arg("id"), "items.move's id")?;
            let container = match arg("container") {
                Value::Nil => player,
                v => id_of(&v, "items.move's container")?,
            };
            let place = count_of(&arg("slot"), "items.move's slot")?.map_or(0, |s| s - 1);
            one(
                Kind::Move { item, container },
                UiRequest::Inventory(InventoryIntent::PutInContainer {
                    item,
                    container,
                    place,
                }),
                Some(loot_or_craft(item)),
            )
        }
        "items.split" => {
            let item = id_of(&arg("id"), "items.split's id")?;
            let amount = count_of(&arg("count"), "items.split's count")?
                .ok_or_else(|| bad("items.split needs a count"))?;
            if amount >= stack_of(item) {
                return Err(bad("items.split's count must be less than the stack"));
            }
            let container = match arg("container") {
                Value::Nil => player,
                v => id_of(&v, "items.split's container")?,
            };
            one(
                Kind::Split { item, container },
                UiRequest::Inventory(InventoryIntent::SplitToContainer {
                    item,
                    container,
                    place: 0,
                    amount,
                }),
                Some(loot_or_craft(item)),
            )
        }
        "items.merge" => {
            let item = id_of(&arg("id"), "items.merge's id")?;
            let target = id_of(&arg("target"), "items.merge's target")?;
            let amount = count_of(&arg("count"), "items.merge's count")?.unwrap_or(stack_of(item));
            one(
                Kind::Merge { item, target },
                UiRequest::Inventory(InventoryIntent::Merge {
                    item,
                    target,
                    amount,
                }),
                Some(loot_or_craft(item)),
            )
        }
        "items.drop" => {
            let item = id_of(&arg("id"), "items.drop's id")?;
            one(
                Kind::Drop(item),
                UiRequest::Inventory(InventoryIntent::Drop { item }),
                Some(AutoDomain::Craft),
            )
        }
        "items.give" => {
            let item = id_of(&arg("id"), "items.give's id")?;
            let target = id_of(&arg("target"), "items.give's target")?;
            let amount = count_of(&arg("count"), "items.give's count")?.unwrap_or(stack_of(item));
            one(
                Kind::Give { item, target },
                UiRequest::Inventory(InventoryIntent::Give {
                    item,
                    target,
                    amount,
                }),
                Some(AutoDomain::Trade),
            )
        }
        "items.wield" => {
            let item = id_of(&arg("id"), "items.wield's id")?;
            let domain = match spell_purpose("items.wield")? {
                Some(d) if is_caster(item) => d,
                Some(_) => return Err(bad("a wield's purpose is for a caster")),
                None => combat_or_craft(item),
            };
            one(
                Kind::Wield(item),
                UiRequest::Inventory(InventoryIntent::Wield { item }),
                Some(domain),
            )
        }
        "items.unwield" => {
            let item = id_of(&arg("id"), "items.unwield's id")?;
            let domain = match spell_purpose("items.unwield")? {
                Some(d) if is_weapon(item) => d,
                Some(_) => return Err(bad("an unwield's purpose is for a weapon or a caster")),
                None => combat_or_craft(item),
            };
            one(
                Kind::Move {
                    item,
                    container: player,
                },
                UiRequest::Inventory(InventoryIntent::PutInContainer {
                    item,
                    container: player,
                    place: 0,
                }),
                Some(domain),
            )
        }
        "items.inscribe" => {
            let object = id_of(&arg("id"), "items.inscribe's id")?;
            let text = arg("text")
                .as_str()
                .ok_or_else(|| bad("items.inscribe's text must be a string"))?
                .to_owned();
            one(
                Kind::Inscribe(object),
                UiRequest::SetInscription { object, text },
                Some(AutoDomain::Craft),
            )
        }
        "items.salvage" => {
            let items = ids_of(&arg("ids"), "items.salvage's ids")?;
            if items.is_empty() {
                return Err(bad("items.salvage needs at least one item"));
            }
            let tool = match arg("tool") {
                Value::Nil => salvage_tool(frame).ok_or_else(|| {
                    CallError::Failed("items.salvage: no salvaging tool in the pack".into())
                })?,
                v => id_of(&v, "items.salvage's tool")?,
            };
            let mut reversed = items.clone();
            reversed.reverse();
            one(
                Kind::Salvage(items),
                UiRequest::SalvageItems {
                    tool,
                    items: reversed,
                },
                Some(AutoDomain::Craft),
            )
        }
        "vendor.buy" => {
            if !frame.world.shop.is_open() {
                return Err(CallError::Failed("vendor.buy: no vendor is open".into()));
            }
            let rows = arg("items");
            let rows = rows
                .as_list()
                .ok_or_else(|| bad("vendor.buy takes a list of {id, count}"))?;
            let mut requests = Vec::new();
            for r in rows {
                let item = id_of(r.get("id"), "vendor.buy's id")?;
                let n = count_of(r.get("count"), "vendor.buy's count")?.unwrap_or(1);
                // A stackable stock row takes the stack size the vendor's list gives it before it
                // is added, as the list does when it shows the stock: the whole stack it sells (or
                // all it has), so the count asked for is bought rather than one.
                if let Some(row) = frame.world.shop.stock.iter().find(|p| p.iid == item) {
                    let max = i32::from(row.pwd.max_stack_size.unwrap_or(0));
                    if max > 1 {
                        let size = if row.amount == -1 {
                            max
                        } else {
                            row.amount.min(max)
                        };
                        requests.push(UiRequest::VendorSetObjectStackSize { item, size });
                    }
                }
                requests.push(UiRequest::VendorAddToBuyList {
                    item,
                    split: i32::try_from(n).unwrap_or(i32::MAX),
                });
            }
            if requests.is_empty() {
                return Err(bad("vendor.buy needs at least one item"));
            }
            requests.push(UiRequest::VendorBuyAll);
            Plan {
                kind: Kind::VendorBuy,
                requests,
                domain: Some(AutoDomain::Vendor),
                gesture_only: false,
            }
        }
        "vendor.sell" => {
            if !frame.world.shop.is_open() {
                return Err(CallError::Failed("vendor.sell: no vendor is open".into()));
            }
            let items = ids_of(&arg("ids"), "vendor.sell's ids")?;
            if items.is_empty() {
                return Err(bad("vendor.sell needs at least one item"));
            }
            let mut requests: Vec<UiRequest> = items
                .into_iter()
                .map(|item| UiRequest::VendorAddToSell { item })
                .collect();
            requests.push(UiRequest::VendorSellAll);
            Plan {
                kind: Kind::VendorSell,
                requests,
                domain: Some(AutoDomain::Vendor),
                gesture_only: false,
            }
        }
        "vendor.close" => one(
            Kind::VendorClose,
            UiRequest::VendorClose,
            Some(AutoDomain::Vendor),
        ),
        "vendor.fill_components" => {
            let max_price = arg("max_price").as_i64().unwrap_or(0);
            one(
                Kind::FillComponents,
                UiRequest::VendorFillComponents {
                    category: None,
                    max_price: i32::try_from(max_price.max(0)).unwrap_or(i32::MAX),
                },
                Some(AutoDomain::Vendor),
            )
        }
        "trade.open" => {
            let p = id_of(&arg("id"), "trade.open's player")?;
            one(
                Kind::TradeOpen(p),
                UiRequest::Use(p),
                Some(AutoDomain::Trade),
            )
        }
        "trade.add" => {
            let item = id_of(&arg("id"), "trade.add's id")?;
            let position = u32::try_from(view.trade().self_rows.len()).unwrap_or(u32::MAX);
            one(
                Kind::TradeAdd(item),
                UiRequest::TradeAddItem { item, position },
                Some(AutoDomain::Trade),
            )
        }
        "trade.accept" => {
            let t = view.trade();
            one(
                Kind::TradeAccept,
                UiRequest::TradeAccept {
                    displayed_self: t.self_rows.len(),
                    displayed_partner: t.partner_rows.len(),
                },
                Some(AutoDomain::Trade),
            )
        }
        "trade.decline" => one(
            Kind::TradeDecline,
            UiRequest::TradeDecline,
            Some(AutoDomain::Trade),
        ),
        "trade.reset" => one(
            Kind::TradeReset,
            UiRequest::TradeReset,
            Some(AutoDomain::Trade),
        ),
        "trade.close" => one(
            Kind::TradeClose,
            UiRequest::TradeClose,
            Some(AutoDomain::Trade),
        ),
        "social.fellowship_create" => {
            let name = arg("name")
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| bad("a fellowship needs a name"))?
                .to_owned();
            let share_xp = arg("share_xp").as_bool().unwrap_or(true);
            one(
                Kind::FellowshipCreate,
                UiRequest::FellowshipCreate { name, share_xp },
                Some(AutoDomain::Social),
            )
        }
        "social.fellowship_quit" => one(
            Kind::FellowshipQuit,
            UiRequest::FellowshipQuit {
                disband: arg("disband").as_bool().unwrap_or(false),
            },
            Some(AutoDomain::Social),
        ),
        "social.fellowship_recruit" => {
            let t = id_of(&arg("id"), "the player to recruit")?;
            one(
                Kind::FellowshipRecruit(t),
                UiRequest::FellowshipRecruit { target: t },
                Some(AutoDomain::Social),
            )
        }
        "social.fellowship_dismiss" => {
            let t = id_of(&arg("id"), "the member to dismiss")?;
            one(
                Kind::FellowshipDismiss(t),
                UiRequest::FellowshipDismiss { target: t },
                Some(AutoDomain::Social),
            )
        }
        "social.fellowship_leader" => {
            let t = id_of(&arg("id"), "the new leader")?;
            one(
                Kind::FellowshipLeader(t),
                UiRequest::FellowshipAssignNewLeader { target: t },
                Some(AutoDomain::Social),
            )
        }
        "social.fellowship_openness" => one(
            Kind::FellowshipOpenness,
            UiRequest::FellowshipToggleOpenness,
            Some(AutoDomain::Social),
        ),
        "social.friend_add" => {
            let name = arg("name")
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| bad("social.friend_add needs a name"))?
                .to_owned();
            one(
                Kind::FriendAdd(name.clone()),
                UiRequest::AddFriend { name },
                Some(AutoDomain::Social),
            )
        }
        "social.friend_remove" => {
            let t = id_of(&arg("id"), "the friend to remove")?;
            one(
                Kind::FriendRemove(t),
                UiRequest::RemoveFriend { target: t },
                Some(AutoDomain::Social),
            )
        }
        "character.raise_skill" => {
            let skill = count_of(&arg("id"), "the skill")?.unwrap_or(0);
            let adv = view
                .skill_advancement(skill)
                .ok_or_else(|| CallError::Failed(format!("no skill {skill} to raise")))?;
            gesture(
                Kind::RaiseSkill(skill),
                UiRequest::TrainSkill {
                    skill,
                    xp: adv.cost_to_raise,
                },
            )
        }
        "character.raise_attribute" => {
            let attribute = count_of(&arg("id"), "the attribute")?.unwrap_or(0);
            let adv = view
                .attribute_advancement(attribute, false)
                .ok_or_else(|| CallError::Failed(format!("no attribute {attribute} to raise")))?;
            gesture(
                Kind::RaiseAttribute(attribute),
                UiRequest::TrainAttribute {
                    attribute,
                    xp: adv.cost_to_raise,
                },
            )
        }
        "character.raise_vital" => {
            let vital = count_of(&arg("id"), "the vital")?.unwrap_or(0);
            let adv = view
                .attribute_advancement(vital, true)
                .ok_or_else(|| CallError::Failed(format!("no vital {vital} to raise")))?;
            gesture(
                Kind::RaiseVital(vital),
                UiRequest::TrainAttribute2nd {
                    vital,
                    xp: adv.cost_to_raise,
                },
            )
        }
        "character.set_option" => {
            let o = arg("name");
            let o = o
                .as_str()
                .and_then(option_named)
                .ok_or_else(|| bad(format!("no player option {o:?}")))?;
            let on = arg("value")
                .as_bool()
                .ok_or_else(|| bad("character.set_option's value must be a boolean"))?;
            gesture(Kind::SetOption(o, on), UiRequest::SetPlayerOption(o, on))
        }
        "magic.cast" => {
            let spell = count_of(&arg("spell"), "the spell")?.unwrap_or(0);
            let target = match arg("target") {
                Value::Nil => None,
                v => Some(id_of(&v, "the spell's target")?),
            };
            let mut requests = Vec::new();
            if let Some(t) = target {
                requests.push(UiRequest::Select(t));
            }
            requests.push(UiRequest::CastSpell { spell_id: spell });
            // On oneself, or on an item one holds or wields (a weapon's or armour's buff).
            let mine = |t: ObjectId| {
                let w = frame.world.weenie(t);
                let wielded = w.is_some_and(|w| w.pwd.wielder_id == Some(player));
                let holder = w.and_then(|w| w.pwd.container_id);
                let held = holder == Some(player)
                    || holder.is_some_and(|c| {
                        frame.world.weenie(c).and_then(|c| c.pwd.container_id) == Some(player)
                    });
                wielded || held
            };
            let on_self = target.is_none_or(|t| t == player || mine(t));
            // A heal is asked for as one (`purpose = "heal"`), and only a Life spell is one.
            let heal = match arg("purpose").as_str() {
                None => false,
                Some("heal") => {
                    if view.spell(spell).is_none_or(|s| s.school != 2) {
                        return Err(bad("only a known Life spell is cast as a heal"));
                    }
                    true
                }
                Some(p) => return Err(bad(format!("a cast's purpose is \"heal\", not {p:?}"))),
            };
            // On another character, a spell the spell table marks beneficial (a buff, a
            // protection) is support; any other is combat.
            let beneficial = view
                .spell(spell)
                .is_some_and(|s| s.bitfield & SPELL_BENEFICIAL != 0);
            Plan {
                kind: Kind::Cast(spell),
                requests,
                domain: Some(if heal {
                    AutoDomain::Heal
                } else if on_self {
                    AutoDomain::Buff
                } else if beneficial {
                    AutoDomain::Support
                } else {
                    AutoDomain::Combat
                }),
                gesture_only: false,
            }
        }
        "combat.set_mode" => {
            let fight = match arg("mode").as_str() {
                Some("peace") => false,
                Some("combat") => true,
                _ => return Err(bad("combat.set_mode takes \"peace\" or \"combat\"")),
            };
            let in_combat = view.combat_mode() != dereth_client_contract::combat_mode::NONCOMBAT;
            // With a purpose, peace, or the magic stance (a caster held), is that spell domain's.
            let caster_held = frame
                .world
                .tables
                .weenies
                .iter()
                .any(|(id, w)| w.pwd.wielder_id == Some(player) && is_caster(id));
            let domain = match spell_purpose("combat.set_mode")? {
                Some(d) if !fight || caster_held => d,
                Some(_) => {
                    return Err(bad(
                        "a stance's purpose is for peace or the magic stance (a caster held)",
                    ))
                }
                None => AutoDomain::Combat,
            };
            Plan {
                kind: Kind::CombatMode(fight),
                requests: if in_combat == fight {
                    Vec::new()
                } else {
                    vec![UiRequest::ToggleCombatMode]
                },
                domain: Some(domain),
                gesture_only: false,
            }
        }
        "combat.attack" => {
            let target = id_of(&arg("target"), "combat.attack's target")?;
            let height = match arg("height").as_str() {
                None | Some("medium") => 2,
                Some("high") => 1,
                Some("low") => 3,
                Some(_) => return Err(bad("an attack's height is high, medium or low")),
            };
            let power = match arg("power") {
                Value::Nil => 0.5,
                v => v
                    .as_f64()
                    .filter(|p| (0.0..=1.0).contains(p))
                    .ok_or_else(|| bad("an attack's power is from 0 to 1"))?,
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=1000
            let position = (power * 1000.0).round() as u32;
            Plan {
                kind: Kind::Attack(target),
                requests: vec![
                    UiRequest::Select(target),
                    UiRequest::CombatSetDesiredPower { position },
                    UiRequest::CombatSetAttackHeight { height },
                    UiRequest::CombatEndAttack { height },
                ],
                domain: Some(AutoDomain::Combat),
                gesture_only: false,
            }
        }
        other => return Err(CallError::Unknown(other.to_owned())),
    })
}

/// Whether the action `name` may only follow a gesture: the character's own advancement and
/// options.
#[must_use]
pub fn gesture_only(name: &str) -> bool {
    name.starts_with("character.")
}

/// A character action: only ever with a gesture.
fn gesture(kind: Kind, r: UiRequest) -> Plan {
    Plan {
        kind,
        requests: vec![r],
        domain: None,
        gesture_only: true,
    }
}

/// The salvaging tool in the player's pack, if any: the first item that is a tinkering tool.
fn salvage_tool(frame: &HookFrame<'_>) -> Option<ObjectId> {
    let player = frame.world.player?;
    let tool = dereth_rules::weenie::item_type::TINKERING_TOOL;
    let mut packs = vec![player];
    packs.extend(frame.view.contained_containers(player).iter().copied());
    packs.into_iter().find_map(|c| {
        frame.view.container_contents(c).iter().copied().find(|i| {
            frame
                .world
                .weenie(*i)
                .is_some_and(|w| w.pwd.obj_type & tool != 0)
        })
    })
}

/// An action's promise, settled.
#[derive(Debug, Clone, PartialEq)]
pub struct Settled {
    pub plugin: String,
    pub id: i64,
    pub action: &'static str,
    /// `Ok(value)` or `Err(reason)`.
    pub result: Result<Value, String>,
}

/// One queued action.
#[derive(Debug, Clone)]
struct Queued {
    id: i64,
    plugin: String,
    plan: Plan,
    queued_at: f64,
}

/// What the frame looked like when an action was sent.
#[derive(Debug, Clone, Default)]
struct Baseline {
    delivery: u64,
    /// The client's count of uses and casts awaiting their acknowledgement.
    busy: u32,
    level: i64,
    stack: u32,
    fellowship_open: Option<bool>,
}

/// The action in flight.
#[derive(Debug, Clone)]
struct Sent {
    q: Queued,
    at: f64,
    /// Frames seen since it was sent.
    frames: u32,
    /// Whether the client's item lock has been seen held for it (for an attack: whether the
    /// client has been seen building or sending it).
    locked: bool,
    base: Baseline,
    /// The last refusal the server sent while it was in flight.
    error: Option<u32>,
}

/// The queue (see the module documentation).
#[derive(Debug, Default)]
pub struct ActionQueue {
    next_id: i64,
    queue: VecDeque<Queued>,
    sent: Option<Sent>,
    /// Each plugin's recent appraisal times.
    appraisals: BTreeMap<String, VecDeque<f64>>,
    /// Actions settled without being sent (refused, or failed before their turn).
    early: Vec<Settled>,
    /// The plugin whose attack the client last sent: an attack goes on repeating after its promise
    /// settles, until something cancels it.
    last_attacker: Option<String>,
    /// An attack's swing is to be dropped (or, already out, cancelled): its target stopped being
    /// the selection, so it would go at something else.
    abort_swing: bool,
}

impl ActionQueue {
    /// Queue `plan` for `plugin`; answers the action's id.
    ///
    /// # Errors
    /// The plugin has too many actions queued.
    pub fn push(&mut self, plugin: &str, plan: Plan, now: f64) -> Result<i64, CallError> {
        let queued = self.queue.iter().filter(|q| q.plugin == plugin).count();
        if queued >= MAX_QUEUED {
            return Err(CallError::Failed(format!(
                "a plugin may have at most {MAX_QUEUED} actions queued"
            )));
        }
        self.next_id += 1;
        self.queue.push_back(Queued {
            id: self.next_id,
            plugin: plugin.to_owned(),
            plan,
            queued_at: now,
        });
        Ok(self.next_id)
    }

    /// An action refused before it was queued (a world that forbids it): its id, settled failed
    /// at the next frame.
    pub fn refuse(&mut self, plugin: &str, action: &'static str, reason: &str) -> i64 {
        self.next_id += 1;
        self.early.push(Settled {
            plugin: plugin.to_owned(),
            id: self.next_id,
            action,
            result: Err(reason.to_owned()),
        });
        self.next_id
    }

    /// A fresh action id, for a promise settled outside the queue (a movement, a session
    /// operation, a request).
    pub fn allocate(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }

    /// Settle a promise at the next frame's settlement.
    pub fn settle_later(&mut self, s: Settled) {
        self.early.push(s);
    }

    /// Take `plugin`'s attacks out of the queue, and the one in flight if it is its attack: each
    /// settled failed as `stopped`.
    pub fn stop_attacks(&mut self, plugin: &str) -> Vec<Settled> {
        let stopped = |q: &Queued| Settled {
            plugin: q.plugin.clone(),
            id: q.id,
            action: q.plan.kind.name(),
            result: Err("stopped".into()),
        };
        let mine = |q: &Queued| q.plugin == plugin && matches!(q.plan.kind, Kind::Attack(_));
        let mut out: Vec<Settled> = self.queue.iter().filter(|q| mine(q)).map(stopped).collect();
        self.queue.retain(|q| !mine(q));
        if let Some(s) = self.sent.take_if(|s| mine(&s.q)) {
            out.insert(0, stopped(&s.q));
        }
        out
    }

    /// Take all of `plugin`'s actions out of the queue and the one in flight if it is its own, each
    /// settled failed as `stopped` (the plugin is going inactive and lives on to hear it); with
    /// whether the client's current attack is the plugin's, as [`Self::forget`] answers.
    pub fn stop_all(&mut self, plugin: &str) -> (Vec<Settled>, bool) {
        let stopped = |q: &Queued| Settled {
            plugin: q.plugin.clone(),
            id: q.id,
            action: q.plan.kind.name(),
            result: Err("stopped".into()),
        };
        let mut out: Vec<Settled> = Vec::new();
        if let Some(s) = self.sent.take_if(|s| s.q.plugin == plugin) {
            out.push(stopped(&s.q));
        }
        out.extend(
            self.queue
                .iter()
                .filter(|q| q.plugin == plugin)
                .map(stopped),
        );
        self.queue.retain(|q| q.plugin != plugin);
        self.appraisals.remove(plugin);
        let attacking = self.last_attacker.take_if(|p| p == plugin).is_some();
        (out, attacking)
    }

    /// Drop everything of `plugin`'s (it is being unloaded, or was suspended). Answers whether the
    /// client's current attack is the plugin's (the last attack sent was), which the caller then
    /// cancels while the client is still attacking: an attack repeats after its promise settles,
    /// and left running it would go on with no plugin to stop it. Any other request already sent
    /// stays sent; its answer is simply not waited for.
    pub fn forget(&mut self, plugin: &str) -> bool {
        self.queue.retain(|q| q.plugin != plugin);
        self.early.retain(|s| s.plugin != plugin);
        self.appraisals.remove(plugin);
        self.sent = self.sent.take().filter(|s| s.q.plugin != plugin);
        self.last_attacker.take_if(|p| p == plugin).is_some()
    }

    /// Whether the client's current attack, if any, is `plugin`'s: the last attack sent was its.
    #[must_use]
    pub fn is_last_attacker(&self, plugin: &str) -> bool {
        self.last_attacker.as_deref() == Some(plugin)
    }

    /// Whether an attack settled this frame wants its swing dropped, or cancelled if it went out:
    /// its target is no longer the selection. Asking clears it.
    pub fn take_abort_swing(&mut self) -> bool {
        std::mem::take(&mut self.abort_swing)
    }

    /// Whether anything is queued or in flight.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && self.sent.is_none() && self.early.is_empty()
    }

    /// The actions this frame settles: the one in flight, if its signal or its timeout came, and
    /// queued ones that waited too long.
    pub fn settle(&mut self, frame: &HookFrame<'_>) -> Vec<Settled> {
        let now = frame.local_time;
        let mut out = std::mem::take(&mut self.early);
        if let Some(mut s) = self.sent.take() {
            s.frames += 1;
            let result = check(&mut s, frame);
            if matches!(&result, Some(Err(r)) if r == TARGET_GONE) {
                self.abort_swing = true;
            }
            if let Some(r) = &result {
                tracing::debug!(target: "dereth::plugin::actions", "{} {} {} after {:.2} s: {r:?}",
                    s.q.plugin, s.q.id, s.q.plan.kind.name(), now - s.at);
            } else if now - s.at > s.q.plan.kind.timeout() {
                tracing::debug!(target: "dereth::plugin::actions", "{} {} {} timed out after {:.2} s ({} frames, outcomes this frame {:?})",
                    s.q.plugin, s.q.id, s.q.plan.kind.name(), now - s.at, s.frames, frame.outcomes);
            }
            match result {
                Some(result) => out.push(Settled {
                    plugin: s.q.plugin.clone(),
                    id: s.q.id,
                    action: s.q.plan.kind.name(),
                    result,
                }),
                None if now - s.at > s.q.plan.kind.timeout() => out.push(Settled {
                    plugin: s.q.plugin.clone(),
                    id: s.q.id,
                    action: s.q.plan.kind.name(),
                    result: Err("timeout".into()),
                }),
                None => self.sent = Some(s),
            }
        }
        let mut kept = VecDeque::new();
        for q in std::mem::take(&mut self.queue) {
            if now - q.queued_at > QUEUE_WAIT_SECS {
                out.push(Settled {
                    plugin: q.plugin.clone(),
                    id: q.id,
                    action: q.plan.kind.name(),
                    result: Err("busy".into()),
                });
            } else {
                kept.push_back(q);
            }
        }
        self.queue = kept;
        out
    }

    /// Send the next action, if nothing is in flight and the client is free for it.
    pub fn start(&mut self, frame: &mut HookFrame<'_>) {
        if self.sent.is_some() {
            return;
        }
        let now = frame.local_time;
        let Some(q) = self.queue.front() else {
            return;
        };
        let w = frame.world;
        if q.plan.kind.uses_item_lock() && !w.request_lock.is_idle() {
            return;
        }
        if q.plan.kind.waits_for_use() && w.magic.busy_count > 0 {
            return;
        }
        // The interface takes one use each fifth of a second; a use sent sooner would be
        // dropped by the client's own throttle.
        if matches!(q.plan.kind, Kind::Use(_) | Kind::UseOn { .. })
            && w.last_used.is_some_and(|t| {
                frame.server_time < t.0 + dereth_client_model::inventory::use_object::USE_THROTTLE
            })
        {
            return;
        }
        if matches!(q.plan.kind, Kind::Appraise(_)) {
            let times = self.appraisals.entry(q.plugin.clone()).or_default();
            while times.front().is_some_and(|t| now - t >= 1.0) {
                times.pop_front();
            }
            if times.len() >= APPRAISALS_PER_SECOND {
                return;
            }
            times.push_back(now);
        }
        let Some(q) = self.queue.pop_front() else {
            return;
        };
        // Spells are cast in magic stance, as the spell bar is only there in it; the server lets
        // a cast in any other stance end with nothing cast, so it is not sent.
        if matches!(q.plan.kind, Kind::Cast(_))
            && frame.view.combat_mode() != dereth_client_contract::combat_mode::MAGIC
        {
            self.early.push(Settled {
                plugin: q.plugin.clone(),
                id: q.id,
                action: q.plan.kind.name(),
                result: Err("not_in_magic_mode".into()),
            });
            return;
        }
        let base = baseline(&q.plan.kind, frame);
        tracing::debug!(target: "dereth::plugin::actions", "{} {} {} sent: {:?}",
            q.plugin, q.id, q.plan.kind.name(), q.plan.requests);
        for r in &q.plan.requests {
            frame.request(r.clone());
        }
        if matches!(q.plan.kind, Kind::Attack(_)) {
            self.last_attacker = Some(q.plugin.clone());
        }
        self.sent = Some(Sent {
            q,
            at: now,
            frames: 0,
            locked: false,
            base,
            error: None,
        });
    }
}

fn skill_level(frame: &HookFrame<'_>, skill: u32) -> i64 {
    frame
        .view
        .skills()
        .iter()
        .find(|s| s.id == skill)
        .map_or(0, |s| i64::from(s.level))
}

fn baseline(kind: &Kind, frame: &HookFrame<'_>) -> Baseline {
    let w = frame.world;
    let view = frame.view;
    let mut b = Baseline {
        busy: w.magic.busy_count,
        ..Baseline::default()
    };
    match kind {
        Kind::Appraise(id) => b.delivery = w.appraisal.delivery(*id),
        Kind::Split { item, .. } | Kind::Merge { item, .. } | Kind::Give { item, .. } => {
            b.stack = w
                .weenie(*item)
                .and_then(|x| x.pwd.stack_size)
                .map_or(1, u32::from);
        }
        Kind::RaiseSkill(s) => b.level = skill_level(frame, *s),
        Kind::RaiseAttribute(a) => {
            b.level = view
                .attribute_advancement(*a, false)
                .map_or(0, |x| i64::from(x.value));
        }
        Kind::RaiseVital(v) => {
            b.level = view
                .attribute_advancement(*v, true)
                .map_or(0, |x| i64::from(x.value));
        }
        Kind::FellowshipOpenness => {
            b.fellowship_open = view.fellowship().map(|f| f.open_fellow);
        }
        _ => {}
    }
    b
}

/// Refusals of something the character said, not did: a tell to someone who is not there, a
/// channel joined, left or not listened to, a chat emote out of place, a message blocked by a
/// squelch, the hear list, a gag, and the notice that someone told is away. Each answers the
/// line that was said, so none settles an action, whatever is waiting when it comes.
const SAID: &[u32] = &[
    0x0042, 0x0044, // a chat emote in combat, or not standing
    0x0422, 0x0423, 0x0424,
    0x0425, // a channel that does not exist, or is not the character's
    0x051B, 0x051C, 0x051D, // a channel entered, left; chat on the network
    0x051F, // a message blocked by a squelch
    0x0520, 0x0521, 0x0522, 0x0524, // the hear list
    0x052B, // the one told is not there
    0x0533, 0x0534, // booted from allegiance chat
    0x0548, // channels while gagged
    0x0551, // not listening to a channel
    0x055E, // the one told is away
    0x057D, 0x057E, 0x057F, 0x0580, 0x0581, 0x0582, 0x0583, // gags
];

/// The server's refusal of the request this action made, if one came this frame. A refusal
/// carries no word of which request it answers but its object and its kind: one naming an object
/// (an inventory request or a trade add refused) is this action's only when the object is one of
/// `objects`; one of something the character said ([`SAID`]) is no action's. A late or stray
/// refusal is left in the frame's log and settles nothing. A refusal naming an object with no
/// reason takes the reason the server gave with its acknowledgement in the same frame.
fn refused(frame: &HookFrame<'_>, objects: &[ObjectId]) -> Option<String> {
    let acknowledged = frame.outcomes.iter().find_map(|o| match *o {
        Outcome::UseDone { failure } if failure != 0 => Some(failure),
        _ => None,
    });
    frame.outcomes.iter().find_map(|o| match *o {
        Outcome::AttemptFailed { object, reason } | Outcome::TradeFailed { object, reason }
            if objects.contains(&object) =>
        {
            let reason = if reason == 0 {
                acknowledged.unwrap_or(0)
            } else {
                reason
            };
            Some(format!("refused (0x{reason:04X})"))
        }
        Outcome::WeenieError { code } if !SAID.contains(&code) => {
            Some(format!("refused (0x{code:04X})"))
        }
        _ => None,
    })
}

/// What a refusal of a buy or a sale may name: the items the plan's requests name, the vendor, and
/// the character itself (the server refuses a whole transaction as a failed save of the
/// character, giving the reason with its acknowledgement).
fn trade_objects(requests: &[UiRequest], frame: &HookFrame<'_>) -> Vec<ObjectId> {
    requests
        .iter()
        .filter_map(|r| match *r {
            UiRequest::VendorAddToBuyList { item, .. }
            | UiRequest::VendorSetObjectStackSize { item, .. }
            | UiRequest::VendorAddToSell { item } => Some(item),
            _ => None,
        })
        .chain(frame.world.shop.vendor_id)
        .chain(frame.world.player)
        .collect()
}

/// A use acknowledgement this frame: `Ok` for success, `Err` with the reason.
fn use_done(frame: &HookFrame<'_>) -> Option<Result<Value, String>> {
    frame.outcomes.iter().find_map(|o| match *o {
        Outcome::UseDone { failure: 0 } => Some(Ok(Value::Nil)),
        Outcome::UseDone { failure } => Some(Err(format!("failed (0x{failure:04X})"))),
        _ => None,
    })
}

/// A use or a cast the client did not send: by the first frame after it was asked for, the
/// client's count of uses awaiting acknowledgement has not risen and no item was taken.
fn client_refused(s: &Sent, frame: &HookFrame<'_>) -> Option<Result<Value, String>> {
    (s.frames == 1 && !s.locked && frame.world.magic.busy_count <= s.base.busy)
        .then(|| Err("refused by the client".to_owned()))
}

/// An attack's state this frame. The server answers an attack request with no message of its
/// own: a swing ends with attack-done, `0` when the sequence goes on (the next swing is announced
/// by commence-attack) and `ActionCancelled` (`0x0036`) when the sequence ends, which is also how
/// a refused attack ends, after the refusal. A request made while a sequence runs joins it, and is
/// answered by that sequence's next swing. So: done on a swing ended or begun, or on the end of
/// the sequence with no refusal; refused with the refusal's code; refused by the client when the
/// client neither charged nor sent it, or gave up charging it.
///
/// The swing goes at whatever is selected when the power bar reaches the power, so it is the
/// attack's only while `target` is the selection: one whose target stops being selected before
/// it is answered (the target fell and its body went, and the client's automatic targeting
/// picked the next monster, or the player picked something) fails as [`TARGET_GONE`], and its
/// swing is dropped, or cancelled if it went out, never left to land on something else.
fn attack(s: &mut Sent, frame: &HookFrame<'_>, target: ObjectId) -> Option<Result<Value, String>> {
    let c = &frame.world.combat;
    let charging_or_sent = c.build_in_progress || c.attack_server_response_pending;
    for o in frame.outcomes {
        match *o {
            Outcome::WeenieError { code } if !SAID.contains(&code) => s.error = Some(code),
            // How the attack ended, for a plugin that wants to know: a sequence the server ended
            // (`done`), the next swing begun (`commenced`), or the sequence cancelled with no
            // refusal (`cancelled`: often the target already fell, or was out of reach).
            Outcome::AttackDone { error: 0 } => {
                return Some(Ok(Value::map().with("ended", "done")));
            }
            Outcome::AttackCommenced => {
                return Some(Ok(Value::map().with("ended", "commenced")));
            }
            Outcome::AttackDone {
                error: ACTION_CANCELLED,
            } => {
                return Some(match s.error {
                    Some(code) => Err(format!("refused (0x{code:04X})")),
                    None => Ok(Value::map().with("ended", "cancelled")),
                });
            }
            Outcome::AttackDone { error } => return Some(Err(format!("failed (0x{error:04X})"))),
            _ => {}
        }
    }
    let waiting = charging_or_sent || c.attack_when_response_received;
    if waiting && frame.world.selected != Some(target) {
        return Some(Err(TARGET_GONE.to_owned()));
    }
    if charging_or_sent {
        s.locked = true;
        return None;
    }
    (s.locked || s.frames >= 2).then(|| Err("refused by the client".to_owned()))
}

/// `ActionCancelled`: the end of an attack sequence.
const ACTION_CANCELLED: u32 = 0x0036;

/// How an attack fails whose target stopped being the selection before it was answered.
pub const TARGET_GONE: &str = "target_gone";

/// The in-flight action's state this frame: `Some` once it has ended.
#[allow(clippy::too_many_lines)]
fn check(s: &mut Sent, frame: &HookFrame<'_>) -> Option<Result<Value, String>> {
    let w = frame.world;
    let view = frame.view;
    let player = w.player;
    let weenie = |id: ObjectId| w.weenie(id);
    let container_of = |id: ObjectId| weenie(id).and_then(|x| x.pwd.container_id);
    let stack_of = |id: ObjectId| {
        weenie(id)
            .and_then(|x| x.pwd.stack_size)
            .map_or(1, u32::from)
    };
    // An item request: done when the item is where it was sent, or when the client's lock,
    // taken for it, is released with no refusal; refused by the client itself when the lock was
    // never taken and nothing happened.
    let item_request = |s: &mut Sent, item: ObjectId, arrived: bool| {
        if arrived {
            return Some(Ok(Value::Nil));
        }
        if let Some(r) = refused(frame, &[item]) {
            return Some(Err(r));
        }
        let held = !w.request_lock.is_idle();
        let waiting = weenie(item).is_some_and(|x| x.waiting);
        if held || waiting {
            s.locked = true;
            return None;
        }
        if s.locked {
            Some(Ok(Value::Nil))
        } else {
            Some(Err("refused by the client".into()))
        }
    };
    match s.q.plan.kind.clone() {
        Kind::Select(id) => (w.selected == Some(id)).then_some(Ok(Value::Nil)),
        Kind::Inscribe(_) | Kind::FillComponents | Kind::SetOption(..) if matches!(s.q.plan.kind, Kind::SetOption(o, v) if view.player_option(o) != v) => {
            None
        }
        Kind::Inscribe(_) | Kind::FillComponents | Kind::SetOption(..) => Some(Ok(Value::Nil)),
        Kind::Appraise(id) => {
            (w.appraisal.delivery(id) > s.base.delivery).then(|| Ok(crate::reads::appraisal(w, id)))
        }
        Kind::Use(id) | Kind::UseOn { item: id, .. } => {
            if let Some(r) = use_done(frame) {
                return Some(r);
            }
            let on = match s.q.plan.kind {
                Kind::UseOn { target, .. } => vec![id, target],
                _ => vec![id],
            };
            if let Some(r) = refused(frame, &on) {
                return Some(Err(r));
            }
            // A use the client answers itself by moving the item (into the pack, onto the body).
            if w.request_lock.object == Some(id) {
                s.locked = true;
            } else if s.locked && w.request_lock.is_idle() {
                return Some(Ok(Value::Nil));
            }
            client_refused(s, frame)
        }
        Kind::CombatMode(fight) => {
            let in_combat = view.combat_mode() != dereth_client_contract::combat_mode::NONCOMBAT;
            (in_combat == fight).then(|| Ok(Value::from(view.combat_mode())))
        }
        Kind::Attack(target) => attack(s, frame, target),
        Kind::Cast(_) => {
            if let Some(r) = use_done(frame) {
                return Some(r);
            }
            if let Some(r) = refused(frame, &[]) {
                return Some(Err(r));
            }
            client_refused(s, frame)
        }
        // A salvage takes no use the client counts, and the server's answer is the items gone
        // into bags (with a result for each material), or a use acknowledgement or a refusal.
        Kind::Salvage(items) => {
            if let Some(r) = use_done(frame) {
                return Some(r);
            }
            if let Some(r) = refused(frame, &items) {
                return Some(Err(r));
            }
            let held = crate::observe::held_items(frame);
            let gone = items.iter().all(|i| !held.contains_key(&i.0));
            (s.frames > 1 && gone).then_some(Ok(Value::Nil))
        }
        Kind::Move { item, container } => {
            let arrived = container_of(item) == Some(container)
                || (Some(container) == player
                    && weenie(item).is_some_and(|x| x.pwd.wielder_id.is_none())
                    && container_of(item)
                        .is_some_and(|c| c == container || container_of(c) == Some(container))
                    && s.frames > 1
                    && !weenie(item).is_some_and(|x| x.waiting));
            item_request(s, item, arrived)
        }
        Kind::Split { item, .. } | Kind::Merge { item, .. } | Kind::Give { item, .. } => {
            let changed = weenie(item).is_none() || stack_of(item) < s.base.stack;
            item_request(s, item, changed)
        }
        Kind::Drop(item) => {
            let on_ground = weenie(item).is_none_or(|x| {
                x.pwd.container_id.is_none() && x.pwd.wielder_id.is_none() && !x.waiting
            }) && s.frames > 1;
            item_request(s, item, on_ground)
        }
        Kind::Wield(item) => {
            let worn = player.is_some() && weenie(item).and_then(|x| x.pwd.wielder_id) == player;
            item_request(s, item, worn)
        }
        Kind::VendorBuy | Kind::VendorSell => {
            if let Some(r) = refused(frame, &trade_objects(&s.q.plan.requests, frame)) {
                return Some(Err(r));
            }
            if !w.request_lock.is_idle() {
                s.locked = true;
                return None;
            }
            Some(if s.locked {
                Ok(Value::Nil)
            } else {
                Err("refused by the client".into())
            })
        }
        Kind::VendorClose => (!w.shop.is_open()).then_some(Ok(Value::Nil)),
        Kind::TradeOpen(with) => {
            if view.trade().open {
                return Some(Ok(Value::Nil));
            }
            refused(frame, &[with]).map(Err)
        }
        Kind::TradeAdd(item) => {
            if view.trade().self_rows.iter().any(|r| r.item == item) {
                return Some(Ok(Value::Nil));
            }
            refused(frame, &[item]).map(Err)
        }
        Kind::TradeAccept => view.trade().accepted.then_some(Ok(Value::Nil)),
        Kind::TradeDecline => (!view.trade().accepted).then_some(Ok(Value::Nil)),
        Kind::TradeReset => view.trade().self_rows.is_empty().then_some(Ok(Value::Nil)),
        Kind::TradeClose => (!view.trade().open).then_some(Ok(Value::Nil)),
        Kind::FellowshipCreate => view.fellowship().is_some().then_some(Ok(Value::Nil)),
        Kind::FellowshipQuit => view.fellowship().is_none().then_some(Ok(Value::Nil)),
        Kind::FellowshipRecruit(t) => view
            .fellowship()
            .is_some_and(|f| f.members.iter().any(|m| m.id == t))
            .then_some(Ok(Value::Nil))
            .or_else(|| refused(frame, &[t]).map(Err)),
        Kind::FellowshipDismiss(t) => (!view
            .fellowship()
            .is_some_and(|f| f.members.iter().any(|m| m.id == t)))
        .then_some(Ok(Value::Nil)),
        Kind::FellowshipLeader(t) => view
            .fellowship()
            .is_some_and(|f| f.leader == t)
            .then_some(Ok(Value::Nil)),
        Kind::FellowshipOpenness => {
            let now = view.fellowship().map(|f| f.open_fellow);
            (now != s.base.fellowship_open).then_some(Ok(Value::Nil))
        }
        Kind::FriendAdd(name) => view
            .friends()
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case(&name))
            .then_some(Ok(Value::Nil))
            .or_else(|| refused(frame, &[]).map(Err)),
        Kind::FriendRemove(t) => {
            (!view.friends().iter().any(|f| f.id == t)).then_some(Ok(Value::Nil))
        }
        Kind::RaiseSkill(skill) => {
            (skill_level(frame, skill) > s.base.level).then_some(Ok(Value::Nil))
        }
        Kind::RaiseAttribute(a) => (view
            .attribute_advancement(a, false)
            .map_or(0, |x| i64::from(x.value))
            > s.base.level)
            .then_some(Ok(Value::Nil)),
        Kind::RaiseVital(v) => (view
            .attribute_advancement(v, true)
            .map_or(0, |x| i64::from(x.value))
            > s.base.level)
            .then_some(Ok(Value::Nil)),
    }
}

/// The vital a plugin names, for reads.
#[must_use]
pub fn vital_named(name: &str) -> Option<Vital> {
    match name {
        "health" => Some(Vital::Health),
        "stamina" => Some(Vital::Stamina),
        "mana" => Some(Vital::Mana),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::option_named;
    use dereth_client_contract::view::PlayerOption;

    #[test]
    fn a_player_option_is_named_in_snake_case() {
        assert_eq!(
            option_named("view_combat_target"),
            Some(PlayerOption::ViewCombatTarget)
        );
        assert_eq!(option_named("ViewCombatTarget"), None);
        assert_eq!(option_named("no_such_option"), None);
    }
}
