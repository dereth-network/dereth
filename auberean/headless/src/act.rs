//! The contract's actions in text: `act <action> <words>...`, the action's name in lowercase with
//! hyphens and its fields in order ([`TEXT_FORMS`] lists each with what it takes).
//!
//! A number is decimal, or hexadecimal after `0x`; an object is named by its id; a yes-or-no is
//! `on` or `off`; a kind of object is one of `creature`, `item`, `player`, `foe`, `fellow`, `pet`;
//! bytes are written in hexadecimal; a name or a line of text is the rest of the line.
//!
//! The actions a front end builds from a whole panel (a new character, the toolbar's shortcuts,
//! the options window's changes, the panels' places, the story's scenes played) and a jump at a
//! velocity have no text form: the script's own verbs (`create`, `say`, the controls, `press
//! jump`) cover what a script does instead.

use auberean_client_sdk::contract::control::{Control, ControlId};
use auberean_client_sdk::contract::{
    ChatHistory, ChatWindowAction, CraftAction, PetAction, SkillAction, SocialAction, StoreAction,
    TradeAction,
};
use auberean_client_sdk::{Action, Motion};

use crate::script::{kind, number};

/// Every action with a text form, and the words it takes.
pub const TEXT_FORMS: &[(&str, &str)] = &[
    ("play-character", "<character>"),
    ("log-off", ""),
    ("delete-character", "<character>"),
    ("restore-character", "<character> <name>"),
    ("chat-packet", "<bytes>"),
    ("server-call", "<function> [<bytes>]"),
    ("look-at", "<object>"),
    ("select", "<object>"),
    ("set-selection", "<object>"),
    ("select-cycle", "<kind> [previous]"),
    ("select-nearest", "<kind>"),
    ("select-self", ""),
    ("select-last", ""),
    ("select-last-attacker", ""),
    ("name-amount", "<item> <quantity>"),
    ("use-selected", ""),
    ("use-toolbar-slot", "<slot>"),
    (
        "put-item",
        "<item> <from> <to> <old slot> <new slot> <quantity>",
    ),
    ("wear", "<item> <location>"),
    ("take-off", "<item> <slot>"),
    ("into-money", "<item> <quantity> <slot> <from>"),
    ("out-of-money", "<amount> <slot> <container>"),
    ("take-all", "<container>"),
    ("melt-all", "<container>"),
    ("set-open", "<container> on|off"),
    ("attack", "on|off"),
    ("examination", "<tabs>|close"),
    ("cancel-quest", "<quest>"),
    ("resurrect", ""),
    ("decline-resurrection", ""),
    ("dismount", ""),
    ("keep-options", ""),
    ("apply-totem", "<target> <source> <effect>"),
    ("skill-train", "<skill>"),
    ("skill-untrain", "<skill>"),
    ("skill-raise", "<skill>"),
    ("skill-cancel-untrain", ""),
    ("craft-open", "<recipe>"),
    ("craft-raise-skill", "<skill>"),
    (
        "craft-add-ingredient",
        "<recipe> <ordinal> <item> <spinner>",
    ),
    ("craft-execute", "<recipe> <spinner> [<ordinal>:<item>]..."),
    ("craft-inquire", "<recipe> <spinner>"),
    ("trade-open", "<partner>"),
    ("trade-close", ""),
    ("trade-accept", ""),
    ("trade-decline", ""),
    ("trade-offer", "<item> <quantity>"),
    ("trade-revoke", "<item>"),
    ("store-purchase", "<item> <quantity> <slot>"),
    ("store-confirm", "on|off"),
    ("store-leave", ""),
    ("catalog-leave", ""),
    ("consignment-leave", ""),
    ("consignment-enter", ""),
    ("store-enter-shop", "<shop>"),
    ("store-next-sales", ""),
    ("store-previous-sales", ""),
    ("store-sales", "<from>"),
    ("store-restock", "<item> <quantity> <sale>"),
    ("store-sell", "<item> <quantity> <price>"),
    ("store-collect", "<sale> coins|item"),
    ("pet-attack", "<object>"),
    ("pet-return", ""),
    ("pet-buff", ""),
    ("pet-heal", ""),
    ("pet-mode", "<pet> <mode>"),
    ("pet-name", "<pet> <name>"),
    ("sale-reminders", ""),
    ("examine", "<kind> <enum> <item> <data id> <selection>"),
    ("say", "<text>"),
    ("know-commands", ""),
    ("ask-history", "commands|retells|replies [earlier]"),
    ("chat-opacity", "<window> <opacity>"),
    ("chat-filter", "<window> <mask>"),
    ("chat-font-size", "<kind> <size>"),
    ("chat-font-color", "<kind> <color>"),
    ("chat-popup", "<kind> on|off"),
    ("chat-devote", "<window> <kind>"),
    ("chat-keep", ""),
    ("cease-attack", "<object>"),
    ("show-object-names", "on|off"),
    ("ask-targets", ""),
    ("ask-saved-places", ""),
    ("play-story-scene", "<act> <scene>"),
    ("turn-head", "<x> <y>"),
    ("request-data", "<kind> <id>"),
    ("control", "<control id> <extent>"),
    ("release-controls", "[typing]"),
    ("move", "<forward> <sideways> <heading>"),
    ("toolbar-set", "<set>"),
    ("show-radar-coordinates", "on|off"),
    ("fellowship-create", "<social on|off> <looting> <name>"),
    ("fellowship-disband", ""),
    ("fellowship-leave", ""),
    ("recruit", "<player>"),
    ("dismiss", "<player>"),
    ("accept-recruitment", ""),
    ("decline-recruitment", "<reason>"),
    ("query-allegiance", "<player>"),
    ("swear", ""),
    ("break", "<player>"),
    ("take-oath", ""),
    ("refuse-oath", "<reason>"),
    ("add-friend", "<name>"),
    ("remove-friend", "<name>"),
    ("get-friends", ""),
    ("set-friends-state", "<mode> [<location>]"),
    ("emote", "<string> <table>"),
    ("assisted-tell", "<name>"),
    ("answer-question", "<question> on|off"),
];

/// The words of one action, read in order.
struct Words<'a> {
    name: &'a str,
    words: &'a [&'a str],
    at: usize,
}

impl<'a> Words<'a> {
    fn wrong(&self) -> String {
        let takes = TEXT_FORMS
            .iter()
            .find(|(n, _)| *n == self.name)
            .map_or("", |(_, t)| *t);
        if takes.is_empty() {
            format!("{} takes nothing more", self.name)
        } else {
            format!("{} takes {takes}", self.name)
        }
    }

    fn next(&mut self) -> Result<&'a str, String> {
        let w = self.words.get(self.at).ok_or_else(|| self.wrong())?;
        self.at += 1;
        Ok(w)
    }

    fn more(&self) -> bool {
        self.at < self.words.len()
    }

    fn id(&mut self) -> Result<u64, String> {
        let w = self.next()?;
        number(w).ok_or_else(|| self.wrong())
    }

    fn word(&mut self) -> Result<u32, String> {
        let w = self.next()?;
        number(w)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| self.wrong())
    }

    fn int(&mut self) -> Result<i32, String> {
        let w = self.next()?;
        w.parse().map_err(|_| self.wrong())
    }

    fn float(&mut self) -> Result<f32, String> {
        let w = self.next()?;
        w.parse::<f32>()
            .ok()
            .filter(|f| f.is_finite())
            .ok_or_else(|| self.wrong())
    }

    fn on(&mut self) -> Result<bool, String> {
        match self.next()? {
            "on" => Ok(true),
            "off" => Ok(false),
            _ => Err(self.wrong()),
        }
    }

    fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let w = self.next()?;
        let hex = w.strip_prefix("0x").unwrap_or(w);
        if hex.len() % 2 != 0 {
            return Err(self.wrong());
        }
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| self.wrong()))
            .collect()
    }

    /// The rest of the line, its words joined by a space.
    fn rest(&mut self) -> Result<String, String> {
        if !self.more() {
            return Err(self.wrong());
        }
        let text = self.words[self.at..].join(" ");
        self.at = self.words.len();
        Ok(text)
    }

    fn units(&mut self) -> Result<Vec<u16>, String> {
        Ok(self.rest()?.encode_utf16().collect())
    }

    fn done(&self) -> Result<(), String> {
        if self.more() {
            Err(self.wrong())
        } else {
            Ok(())
        }
    }
}

/// An action from its text form: its name and its words.
///
/// # Errors
/// No action has the name, or its words are not what it takes.
pub fn parse(words: &[&str]) -> Result<Action, String> {
    let Some((name, words)) = words.split_first() else {
        return Err("act needs an action".into());
    };
    let mut w = Words { name, words, at: 0 };
    let a = action(name, &mut w)?;
    w.done()?;
    Ok(a)
}

#[allow(clippy::too_many_lines)]
fn action(name: &str, w: &mut Words<'_>) -> Result<Action, String> {
    use Action as A;
    Ok(match name {
        "play-character" => A::PlayCharacter { character: w.id()? },
        "log-off" => A::LogOff,
        "delete-character" => A::DeleteCharacter { character: w.id()? },
        "restore-character" => A::RestoreCharacter {
            character: w.id()?,
            name: w.rest()?,
        },
        "chat-packet" => A::Chat(w.bytes()?),
        "server-call" => A::ServerCall {
            function: w.word()?,
            args: if w.more() { w.bytes()? } else { Vec::new() },
        },
        "look-at" => A::LookAt { target: w.id()? },
        "select" => A::Select { target: w.id()? },
        "set-selection" => A::SetSelection { target: w.id()? },
        "select-cycle" => {
            let k = w.next()?;
            let kind = kind(k).ok_or_else(|| w.wrong())?;
            let previous = match w.more() {
                false => false,
                true if w.next()? == "previous" => true,
                true => return Err(w.wrong()),
            };
            A::SelectCycle { kind, previous }
        }
        "select-nearest" => {
            let k = w.next()?;
            A::SelectNearest {
                kind: kind(k).ok_or_else(|| w.wrong())?,
            }
        }
        "select-self" => A::SelectSelf,
        "select-last" => A::SelectLast,
        "select-last-attacker" => A::SelectLastAttacker,
        "name-amount" => A::NameAmount {
            item: w.id()?,
            quantity: w.word()?,
        },
        "use-selected" => A::UseSelected,
        "use-toolbar-slot" => A::UseToolbarSlot { slot: w.word()? },
        "put-item" => A::PutItem {
            item: w.id()?,
            from: w.id()?,
            to: w.id()?,
            old_slot: w.word()?,
            new_slot: w.word()?,
            quantity: w.word()?,
        },
        "wear" => A::Wear {
            item: w.id()?,
            location: w.word()?,
        },
        "take-off" => A::TakeOff {
            item: w.id()?,
            slot: w.word()?,
        },
        "into-money" => A::IntoMoney {
            item: w.id()?,
            quantity: w.word()?,
            slot: w.word()?,
            from: w.id()?,
        },
        "out-of-money" => A::OutOfMoney {
            amount: w.word()?,
            slot: w.word()?,
            container: w.id()?,
        },
        "take-all" => A::TakeAll { container: w.id()? },
        "melt-all" => A::MeltAll { container: w.id()? },
        "set-open" => A::SetOpen {
            container: w.id()?,
            open: w.on()?,
        },
        "attack" => A::Attack { start: w.on()? },
        "examination" => A::Examination {
            tabs: if w.words.get(w.at) == Some(&"close") {
                w.at += 1;
                None
            } else {
                Some(w.word()?)
            },
        },
        "cancel-quest" => A::CancelQuest { quest: w.word()? },
        "resurrect" => A::Resurrect,
        "decline-resurrection" => A::DeclineResurrection,
        "dismount" => A::Dismount,
        "keep-options" => A::KeepOptions,
        "apply-totem" => A::ApplyTotem {
            target: w.id()?,
            source: w.id()?,
            effect: w.word()?,
        },
        "skill-train" => A::Skill(SkillAction::Train(w.word()?)),
        "skill-untrain" => A::Skill(SkillAction::Untrain(w.word()?)),
        "skill-raise" => A::Skill(SkillAction::Raise(w.word()?)),
        "skill-cancel-untrain" => A::Skill(SkillAction::CancelUntrain),
        "craft-open" => A::Craft(CraftAction::Open(w.word()?)),
        "craft-raise-skill" => A::Craft(CraftAction::RaiseCraftSkill(w.word()?)),
        "craft-add-ingredient" => A::Craft(CraftAction::AddIngredient {
            recipe: w.word()?,
            ordinal: w.word()?,
            item: w.id()?,
            spinner: w.int()?,
        }),
        "craft-execute" => {
            let recipe = w.word()?;
            let spinner = w.int()?;
            let mut ingredients = Vec::new();
            while w.more() {
                let pair = w.next()?;
                let (o, i) = pair.split_once(':').ok_or_else(|| w.wrong())?;
                let o = number(o).and_then(|n| u32::try_from(n).ok());
                match (o, number(i)) {
                    (Some(o), Some(i)) => ingredients.push((o, i)),
                    _ => return Err(w.wrong()),
                }
            }
            A::Craft(CraftAction::ExecuteCraft {
                recipe,
                spinner,
                ingredients,
            })
        }
        "craft-inquire" => A::Craft(CraftAction::Inquire {
            recipe: w.word()?,
            spinner: w.int()?,
        }),
        "trade-open" => A::Trade(TradeAction::Open(w.id()?)),
        "trade-close" => A::Trade(TradeAction::Close),
        "trade-accept" => A::Trade(TradeAction::Accept),
        "trade-decline" => A::Trade(TradeAction::Decline),
        "trade-offer" => A::Trade(TradeAction::Offer {
            item: w.id()?,
            quantity: w.word()?,
        }),
        "trade-revoke" => A::Trade(TradeAction::Revoke(w.id()?)),
        "store-purchase" => A::Store(StoreAction::Purchase {
            item: w.id()?,
            quantity: w.word()?,
            slot: w.word()?,
        }),
        "store-confirm" => A::Store(StoreAction::Confirm(w.on()?)),
        "store-leave" => A::Store(StoreAction::LeaveStore),
        "catalog-leave" => A::Store(StoreAction::LeaveCatalog),
        "consignment-leave" => A::Store(StoreAction::LeaveConsignment),
        "consignment-enter" => A::Store(StoreAction::EnterConsignment),
        "store-enter-shop" => A::Store(StoreAction::EnterShop(w.word()?)),
        "store-next-sales" => A::Store(StoreAction::NextSales),
        "store-previous-sales" => A::Store(StoreAction::PrevSales),
        "store-sales" => A::Store(StoreAction::Sales(w.word()?)),
        "store-restock" => A::Store(StoreAction::Restock {
            item: w.id()?,
            quantity: w.word()?,
            sale: w.word()?,
        }),
        "store-sell" => A::Store(StoreAction::Sell {
            item: w.id()?,
            quantity: w.word()?,
            price: w.word()?,
        }),
        "store-collect" => {
            let sale = w.word()?;
            let sold = match w.next()? {
                "coins" => true,
                "item" => false,
                _ => return Err(w.wrong()),
            };
            A::Store(StoreAction::Collect { sale, sold })
        }
        "pet-attack" => A::Pet(PetAction::Attack(w.id()?)),
        "pet-return" => A::Pet(PetAction::Return),
        "pet-buff" => A::Pet(PetAction::Buff),
        "pet-heal" => A::Pet(PetAction::Heal),
        "pet-mode" => A::Pet(PetAction::Mode {
            pet: w.id()?,
            mode: w.word()?,
        }),
        "pet-name" => A::Pet(PetAction::Name {
            pet: w.id()?,
            name: w.rest()?,
        }),
        "sale-reminders" => A::SaleReminders,
        "examine" => A::Examine {
            kind: w.word()?,
            data_enum: w.word()?,
            item: w.id()?,
            data_id: w.word()?,
            selection: w.id()?,
        },
        "say" => A::Say { text: w.rest()? },
        "know-commands" => A::KnowCommands,
        "ask-history" => {
            let list = match w.next()? {
                "commands" => ChatHistory::Commands,
                "retells" => ChatHistory::Retells,
                "replies" => ChatHistory::Replies,
                _ => return Err(w.wrong()),
            };
            let earlier = match w.more() {
                false => false,
                true if w.next()? == "earlier" => true,
                true => return Err(w.wrong()),
            };
            A::AskHistory { list, earlier }
        }
        "chat-opacity" => A::ChatWindows(ChatWindowAction::Opacity {
            window: w.word()?,
            opacity: w.float()?,
        }),
        "chat-filter" => A::ChatWindows(ChatWindowAction::Filter {
            window: w.word()?,
            filter: w.word()?,
        }),
        "chat-font-size" => A::ChatWindows(ChatWindowAction::FontSize {
            kind: w.word()?,
            size: w.word()?,
        }),
        "chat-font-color" => A::ChatWindows(ChatWindowAction::FontColor {
            kind: w.word()?,
            color: w.word()?,
        }),
        "chat-popup" => A::ChatWindows(ChatWindowAction::Popup {
            kind: w.word()?,
            on: w.on()?,
        }),
        "chat-devote" => A::ChatWindows(ChatWindowAction::Devote {
            window: w.word()?,
            kind: w.word()?,
        }),
        "chat-keep" => A::ChatWindows(ChatWindowAction::Keep),
        "cease-attack" => A::CeaseAttack { target: w.id()? },
        "show-object-names" => A::ShowObjectNames { on: w.on()? },
        "ask-targets" => A::AskTargets,
        "ask-saved-places" => A::AskSavedPlaces,
        "play-story-scene" => A::PlayStoryScene {
            act: w.word()?,
            scene: w.word()?,
        },
        "turn-head" => A::TurnHead {
            x: w.float()?,
            y: w.float()?,
        },
        "request-data" => A::RequestData {
            kind: w.word()?,
            id: w.word()?,
        },
        "control" => {
            let id = ControlId(w.word()?);
            A::Control(Control::at(id, w.float()?))
        }
        "release-controls" => A::ReleaseControls {
            typing: match w.more() {
                false => false,
                true if w.next()? == "typing" => true,
                true => return Err(w.wrong()),
            },
        },
        "move" => {
            let forward = w.float()?;
            let sideways = w.float()?;
            let heading = w.float()?;
            A::Move(Motion {
                forward,
                sideways,
                heading,
                active: forward != 0.0 || sideways != 0.0,
            })
        }
        "toolbar-set" => A::ToolbarSet { set: w.word()? },
        "show-radar-coordinates" => A::ShowRadarCoordinates { on: w.on()? },
        "fellowship-create" => A::Social(SocialAction::CreateFellowship {
            social: w.on()?,
            looting: w.word()?,
            name: w.units()?,
        }),
        "fellowship-disband" => A::Social(SocialAction::DisbandFellowship),
        "fellowship-leave" => A::Social(SocialAction::LeaveFellowship),
        "recruit" => A::Social(SocialAction::Recruit(w.id()?)),
        "dismiss" => A::Social(SocialAction::Dismiss(w.id()?)),
        "accept-recruitment" => A::Social(SocialAction::AcceptRecruitment),
        "decline-recruitment" => A::Social(SocialAction::DeclineRecruitment(w.word()?)),
        "query-allegiance" => A::Social(SocialAction::QueryAllegiance(w.id()?)),
        "swear" => A::Social(SocialAction::Swear),
        "break" => A::Social(SocialAction::Break(w.id()?)),
        "take-oath" => A::Social(SocialAction::TakeOath),
        "refuse-oath" => A::Social(SocialAction::RefuseOath(w.word()?)),
        "add-friend" => A::Social(SocialAction::AddFriend(w.units()?)),
        "remove-friend" => A::Social(SocialAction::RemoveFriend(w.units()?)),
        "get-friends" => A::Social(SocialAction::GetFriends),
        "set-friends-state" => A::Social(SocialAction::SetState {
            mode: w.word()?,
            location: if w.more() { w.units()? } else { Vec::new() },
        }),
        "emote" => A::Emote {
            string: w.word()?,
            table: w.word()?,
        },
        "assisted-tell" => A::AssistedTell { name: w.rest()? },
        "answer-question" => A::AnswerQuestion {
            question: w.word()?,
            yes: w.on()?,
        },
        _ => {
            let names: Vec<&str> = TEXT_FORMS.iter().map(|(n, _)| *n).collect();
            return Err(format!("no action {name:?}: one of {}", names.join(", ")));
        }
    })
}
