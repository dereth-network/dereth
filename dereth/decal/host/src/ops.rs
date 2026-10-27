//! What the Decal plugin is to do: one op for each call it makes through Decal.
//!
//! The host's frame turns what the plugins asked for into ops: the requests the client's own
//! interface would have sent (as the Dereth client's interaction step builds them), each as the
//! Decal call that makes the retail client send the same request; a movement as a facing or the
//! run lock; a line to print, to hide or to run through the client's chat parser. An op's JSON is
//! what crosses the C interface ([`Op::to_value`]).
//!
//! | request | Decal call |
//! |---|---|
//! | use | `UseItem(object, 0)` |
//! | use on a target | `ApplyItem(object, target)` |
//! | appraise | `RequestId(object)` |
//! | put in a container | `MoveItem(item, container, slot, false)` |
//! | wield | the auto-wield call, on the item |
//! | drop | `DropItem(item)` |
//! | give | `GiveItem(item, target)` |
//! | cast | `CastSpell(spell, target)`, the character itself for a spell with no target |
//! | stance | `SetCombatMode(mode)` |
//! | buy, sell | the vendor's buy or sell list, filled, then `VendorBuyAll` or `VendorSellAll` |
//! | trade | `TradeAdd`, `TradeAccept`, `TradeDecline`, `TradeReset`, `TradeEnd` |
//! | fellowship | `FellowshipRecruit`, `FellowshipDismiss`, `FellowshipGrantLeader`, `FellowshipSetOpen`, `FellowshipQuit`, `FellowshipDisband` |
//! | salvage | the salvage panel: the tool used, each item added, then `SalvagePanelSalvage` |
//! | train | `AddSkillExperience`, `AddAttributeExperience`, `AddVitalExperience` |
//! | add or remove a friend | the client's own `/friends add <name>` or `/friends remove <name>` |
//!
//! Any other request the interaction makes is not sent: the retail client makes its own (the
//! queries its panels send, say). An attack is sent by none of these: with the player's leave it
//! is the game's own attack key, held for the attack's power ([`crate::keys`], [`Op::Key`]).

use dereth_client_sdk::model::{Request, World};
use dereth_client_sdk::plugin_api::Value;

/// The plugin actions a host inside the retail client cannot carry out: nothing in Decal's calls
/// makes the client send them. Each fails at once as `unavailable`; those of [`KEY_ACTIONS`] only
/// while the host may not press the game's keys.
pub const UNAVAILABLE: &[&str] = &[
    "combat.attack",
    "items.split",
    "items.merge",
    "items.inscribe",
    "social.fellowship_create",
    "character.set_option",
    "vendor.close",
    "nav.jump",
    "nav.motion",
    "session.select_character",
];

/// The plugin actions the host carries out by pressing the game's own keys, with the player's
/// leave ([`crate::keys`]): an attack, a jump and a held movement.
pub const KEY_ACTIONS: &[&str] = &["combat.attack", "nav.jump", "nav.motion"];

/// One thing for the Decal plugin to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Show a line in the chat window, in the color of chat type `color`.
    Print {
        text: String,
        color: u32,
    },
    /// Keep the chat window's next line with this text from showing.
    Hide {
        text: String,
    },
    /// Run a line through the client's chat parser, as if typed.
    Chat {
        line: String,
    },
    Select {
        id: u32,
    },
    Use {
        id: u32,
    },
    Apply {
        item: u32,
        target: u32,
    },
    Appraise {
        id: u32,
    },
    Move {
        item: u32,
        container: u32,
        slot: u32,
    },
    Wield {
        item: u32,
    },
    Drop {
        id: u32,
    },
    Give {
        item: u32,
        target: u32,
    },
    Cast {
        spell: u32,
        target: u32,
    },
    /// The stance: 1 peace, 2 melee, 4 missile, 8 magic.
    Stance {
        mode: u32,
    },
    VendorBuy {
        items: Vec<(u32, i32)>,
    },
    VendorSell {
        items: Vec<u32>,
    },
    TradeAdd {
        item: u32,
    },
    TradeAccept,
    TradeDecline,
    TradeReset,
    TradeEnd,
    FellowRecruit {
        id: u32,
    },
    FellowDismiss {
        id: u32,
    },
    FellowLeader {
        id: u32,
    },
    FellowOpen {
        open: bool,
    },
    FellowLeave {
        disband: bool,
    },
    Salvage {
        tool: u32,
        items: Vec<u32>,
    },
    TrainSkill {
        skill: u32,
        xp: u32,
    },
    TrainAttribute {
        attribute: u32,
        xp: u32,
    },
    TrainVital {
        vital: u32,
        xp: u32,
    },
    /// Turn to a heading, in degrees clockwise from north.
    Face {
        heading: f64,
    },
    /// Set the run lock.
    Autorun {
        on: bool,
    },
    /// Press a key of the game's (`down`) or let it go: the game's window gets the key's own
    /// messages, its virtual-key code and its place on the keyboard.
    Key {
        key: crate::keys::Key,
        down: bool,
    },
    /// Keep one of the host's settings in its settings file, as the player changed it.
    Setting {
        name: String,
        value: String,
    },
    Logout,
    /// A plugin window's view, built again: its Decal view XML and every control's state.
    View(crate::views::ViewOp),
    /// A plugin window's controls whose state changed.
    Update {
        key: String,
        controls: Vec<Value>,
    },
    /// Show or hide a plugin window's view.
    Show {
        key: String,
        open: bool,
    },
    /// A plugin window's view, gone with its window.
    Dispose {
        key: String,
    },
    /// A marker as one of Decal's 3-D objects.
    Mark(crate::marks::Mark),
    Unmark {
        key: String,
    },
}

fn op(name: &str) -> Value {
    Value::map().with("op", name)
}

impl Op {
    /// The op as the C interface hands it on.
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Self::Print { text, color } => op("print")
                .with("text", text.as_str())
                .with("color", *color),
            Self::Hide { text } => op("hide").with("text", text.as_str()),
            Self::Chat { line } => op("chat").with("line", line.as_str()),
            Self::Select { id } => op("select").with("id", *id),
            Self::Use { id } => op("use").with("id", *id),
            Self::Apply { item, target } => op("apply").with("item", *item).with("target", *target),
            Self::Appraise { id } => op("appraise").with("id", *id),
            Self::Move {
                item,
                container,
                slot,
            } => op("move")
                .with("item", *item)
                .with("container", *container)
                .with("slot", *slot),
            Self::Wield { item } => op("wield").with("item", *item),
            Self::Drop { id } => op("drop").with("id", *id),
            Self::Give { item, target } => op("give").with("item", *item).with("target", *target),
            Self::Cast { spell, target } => {
                op("cast").with("spell", *spell).with("target", *target)
            }
            Self::Stance { mode } => op("stance").with("mode", *mode),
            Self::VendorBuy { items } => op("vendor_buy").with(
                "items",
                items
                    .iter()
                    .map(|(id, n)| Value::map().with("id", *id).with("count", *n))
                    .collect::<Vec<_>>(),
            ),
            Self::VendorSell { items } => op("vendor_sell").with(
                "items",
                items.iter().map(|id| Value::from(*id)).collect::<Vec<_>>(),
            ),
            Self::TradeAdd { item } => op("trade_add").with("item", *item),
            Self::TradeAccept => op("trade_accept"),
            Self::TradeDecline => op("trade_decline"),
            Self::TradeReset => op("trade_reset"),
            Self::TradeEnd => op("trade_end"),
            Self::FellowRecruit { id } => op("fellow_recruit").with("id", *id),
            Self::FellowDismiss { id } => op("fellow_dismiss").with("id", *id),
            Self::FellowLeader { id } => op("fellow_leader").with("id", *id),
            Self::FellowOpen { open } => op("fellow_open").with("open", *open),
            Self::FellowLeave { disband } => op("fellow_leave").with("disband", *disband),
            Self::Salvage { tool, items } => op("salvage").with("tool", *tool).with(
                "items",
                items.iter().map(|id| Value::from(*id)).collect::<Vec<_>>(),
            ),
            Self::TrainSkill { skill, xp } => {
                op("train_skill").with("skill", *skill).with("xp", *xp)
            }
            Self::TrainAttribute { attribute, xp } => op("train_attribute")
                .with("attribute", *attribute)
                .with("xp", *xp),
            Self::TrainVital { vital, xp } => {
                op("train_vital").with("vital", *vital).with("xp", *xp)
            }
            Self::Face { heading } => op("face").with("heading", *heading),
            Self::Autorun { on } => op("autorun").with("on", *on),
            Self::Key { key, down } => op("key")
                .with("vk", key.vk)
                .with("scan", key.scan)
                .with("extended", key.extended)
                .with("down", *down),
            Self::Setting { name, value } => op("setting")
                .with("name", name.as_str())
                .with("value", value.as_str()),
            Self::Logout => op("logout"),
            Self::View(v) => v.to_value(),
            Self::Update { key, controls } => op("update")
                .with("key", key.as_str())
                .with("controls", controls.clone()),
            Self::Show { key, open } => op("show").with("key", key.as_str()).with("open", *open),
            Self::Dispose { key } => op("dispose").with("key", key.as_str()),
            Self::Mark(m) => m.to_value(),
            Self::Unmark { key } => op("unmark").with("key", key.as_str()),
        }
    }
}

/// The ops that make the retail client send `r` in `world` (whose character a spell with no target
/// is cast on, and whose friends list names the friend to remove): none for a request it has no
/// call for.
#[must_use]
pub fn of_request(r: &Request, world: &World) -> Vec<Op> {
    let me = world.player.map_or(0, |p| p.0);
    let one = |o: Op| vec![o];
    match r {
        Request::UseEvent(m) => one(Op::Use { id: m.object.0 }),
        Request::UseWithTargetEvent(m) => one(Op::Apply {
            item: m.object.0,
            target: m.target.0,
        }),
        Request::Appraise(m) => one(Op::Appraise { id: m.target.0 }),
        Request::PutItemInContainer(m) => one(Op::Move {
            item: m.item.0,
            container: m.container.0,
            slot: m.slot,
        }),
        Request::GetAndWieldItem(m) => one(Op::Wield { item: m.item.0 }),
        Request::DropItem(m) => one(Op::Drop { id: m.item.0 }),
        Request::GiveObjectRequest(m) => one(Op::Give {
            item: m.item.0,
            target: m.target.0,
        }),
        Request::CastTargetedSpell(m) => one(Op::Cast {
            spell: m.spell_id,
            target: m.target.0,
        }),
        Request::CastUntargetedSpell(m) => one(Op::Cast {
            spell: m.spell_id,
            target: me,
        }),
        Request::ChangeCombatMode(m) => one(Op::Stance {
            mode: m.combat_mode,
        }),
        Request::VendorBuy(m) => one(Op::VendorBuy {
            items: m.items.iter().map(|i| (i.iid.0, i.amount)).collect(),
        }),
        Request::VendorSell(m) => one(Op::VendorSell {
            items: m.items.iter().map(|i| i.iid.0).collect(),
        }),
        Request::TradeAddToTrade(m) => one(Op::TradeAdd { item: m.item.0 }),
        Request::TradeAcceptTrade(_) => one(Op::TradeAccept),
        Request::TradeDeclineTrade(_) => one(Op::TradeDecline),
        Request::TradeResetTrade(_) => one(Op::TradeReset),
        Request::TradeCloseTradeNegotiations(_) => one(Op::TradeEnd),
        Request::FellowshipRecruit(m) => one(Op::FellowRecruit { id: m.target.0 }),
        Request::FellowshipDismiss(m) => one(Op::FellowDismiss { id: m.target.0 }),
        Request::FellowshipAssignNewLeader(m) => one(Op::FellowLeader { id: m.target.0 }),
        Request::FellowshipChangeFellowOpenness(m) => one(Op::FellowOpen { open: m.open != 0 }),
        Request::FellowshipQuit(m) => one(Op::FellowLeave {
            disband: m.disband != 0,
        }),
        Request::CreateTinkeringTool(m) => one(Op::Salvage {
            tool: m.tool.0,
            items: m.items.iter().map(|i| i.0).collect(),
        }),
        Request::TrainSkill(m) => one(Op::TrainSkill {
            skill: m.skill_id,
            xp: m.xp_spent,
        }),
        Request::TrainAttribute(m) => one(Op::TrainAttribute {
            attribute: m.attribute_id,
            xp: m.xp_spent,
        }),
        Request::TrainAttribute2nd(m) => one(Op::TrainVital {
            vital: m.vital_id,
            xp: m.xp_spent,
        }),
        Request::SocialAddFriend(m) => one(Op::Chat {
            line: format!("/friends add {}", m.name),
        }),
        // The command takes a name; a friend not on the list is not sent.
        Request::SocialRemoveFriend(m) => world
            .friends()
            .iter()
            .find(|f| f.id == m.friend_id)
            .map(|f| Op::Chat {
                line: format!("/friends remove {}", f.name),
            })
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a host inside another program: requests as Decal's calls)
    use super::*;
    use dereth_client_sdk::model::player::Friend;
    use dereth_client_sdk::primitives::ObjectId;
    use dereth_client_sdk::protocol::combat::MagicCastUntargetedSpell;
    use dereth_client_sdk::protocol::social::{SocialAddFriend, SocialRemoveFriend};
    use dereth_client_sdk::protocol::trade::{ItemProfile, VendorBuy, VendorSell};

    fn world() -> World {
        let mut w = World::new();
        w.player = Some(ObjectId(0x5000_0001));
        w.player_system.social.friends.push(Friend {
            id: ObjectId(0x5000_0002),
            name: "Ash".into(),
            online: true,
            appear_offline: false,
        });
        w
    }

    fn profile(id: u32, amount: i32) -> ItemProfile {
        ItemProfile {
            amount,
            iid: ObjectId(id),
            pwd: None,
        }
    }

    #[test]
    fn a_spell_with_no_target_is_cast_on_the_character() {
        assert_eq!(
            of_request(
                &Request::CastUntargetedSpell(MagicCastUntargetedSpell { spell_id: 2 }),
                &world()
            ),
            vec![Op::Cast {
                spell: 2,
                target: 0x5000_0001
            }]
        );
    }

    #[test]
    fn a_purchase_and_a_sale_carry_every_item_with_its_count() {
        let buy = Request::VendorBuy(VendorBuy {
            vendor_id: ObjectId(9),
            items: vec![profile(10, 5), profile(11, 1)],
            alternate_currency_id: 0,
        });
        assert_eq!(
            of_request(&buy, &world()),
            vec![Op::VendorBuy {
                items: vec![(10, 5), (11, 1)]
            }]
        );
        let sell = Request::VendorSell(VendorSell {
            vendor_id: ObjectId(9),
            items: vec![profile(12, 1), profile(13, 3)],
        });
        assert_eq!(
            of_request(&sell, &world()),
            vec![Op::VendorSell {
                items: vec![12, 13]
            }]
        );
    }

    #[test]
    fn a_friend_is_added_and_removed_by_name_through_the_clients_own_command() {
        assert_eq!(
            of_request(
                &Request::SocialAddFriend(SocialAddFriend { name: "Bex".into() }),
                &world()
            ),
            vec![Op::Chat {
                line: "/friends add Bex".into()
            }]
        );
        let remove = |id| {
            of_request(
                &Request::SocialRemoveFriend(SocialRemoveFriend {
                    friend_id: ObjectId(id),
                }),
                &world(),
            )
        };
        assert_eq!(
            remove(0x5000_0002),
            vec![Op::Chat {
                line: "/friends remove Ash".into()
            }]
        );
        assert_eq!(remove(0x5000_0003), Vec::new(), "not on the list");
    }

    #[test]
    fn a_request_the_retail_client_makes_itself_is_not_sent() {
        let q = Request::QueryHealth(dereth_client_sdk::protocol::combat::CombatQueryHealth {
            target: ObjectId(7),
        });
        assert_eq!(of_request(&q, &world()), Vec::new());
    }

    #[test]
    fn an_op_crosses_the_interface_as_json_named_by_its_call() {
        let v = Op::Move {
            item: 1,
            container: 2,
            slot: 3,
        }
        .to_value();
        assert_eq!(v.get("op").as_str(), Some("move"));
        assert_eq!(
            (
                v.get("item").as_i64(),
                v.get("container").as_i64(),
                v.get("slot").as_i64()
            ),
            (Some(1), Some(2), Some(3))
        );
    }
}
