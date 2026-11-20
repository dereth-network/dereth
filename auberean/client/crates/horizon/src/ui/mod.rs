//! The interface, drawn each frame from the game's state with the widget kit: the HUD and its menu
//! bar ([`hud`]), the selection's marks in the world ([`marker`]), the hotbar and the attack button
//! ([`hotbar`]), the minimap ([`minimap`]), the log
//! window and its entry ([`chat`]), the game's questions ([`questions`]), the skill window
//! ([`skills`]), the examine window ([`examine`]), the party list and the conflict lamp
//! ([`party`]), the experience band, the link line and the vitae lamp ([`status`]), the social
//! window ([`social`]), the windows of the player's things ([`goods`]: the inventory, loot, trade,
//! shops and the market), the crafting window ([`crafting`]) and the making window ([`making`]), the pets and the Dismount button ([`pets`]), the map ([`map`]), the
//! journal ([`journal`]), a book read ([`book`]), the story's scenes ([`story`]) and the settings
//! window ([`settings`]). The HUD's elements stand where the HUD layout puts them ([`layout`]); the
//! plugins' windows and HUDs are drawn over the HUD ([`plugins`]), and in gamepad mode the cross
//! hotbars over the hotbar ([`cross`]). The windows the player opens stack, the one opened or
//! pressed last in front ([`stack`]), and each opens first where it keeps clear of the HUD
//! ([`place`]).
//!
//! Each frame [`HorizonUi::frame`] draws into a draw list and says what the player asked for
//! ([`Outcome`]). The places it drew that answer the pointer are kept, so a press over one of
//! them is the interface's and one anywhere else is the world's ([`HorizonUi::covers`]).

pub mod bindings;
pub mod book;
pub mod chat;
pub mod crafting;
pub mod creation;
pub mod cross;
pub mod examine;
pub mod goods;
pub mod hotbar;
pub mod hud;
pub mod inventory;
pub mod items;
pub mod journal;
pub mod layout;
pub mod leaving;
pub mod loot;
pub mod making;
pub mod map;
pub mod marker;
pub mod market;
pub mod minimap;
pub mod options;
pub mod party;
pub mod pets;
pub mod place;
pub mod plugins;
pub mod pregame;
pub mod questions;
pub mod settings;
pub mod shop;
pub mod skills;
pub mod social;
pub mod stack;
pub mod status;
pub mod story;
pub mod trade;

use std::sync::Arc;

use auberean_client_sdk::Action;
use auberean_shell::interface::{Choice, Request};
pub use dereth_ui_kit::input::{vk, InputFrame, PointerOver};
use dereth_ui_kit::paint::Painter;
use dereth_ui_kit::widgets::Ctx;

use crate::art::Art;
use crate::draw::{DrawList, Rect, WHITE};
use crate::state::GameState;

/// What the player asked for in a frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// A line typed into the chat entry and sent.
    pub say: Option<String>,
    /// The chat entry asked for the line it sent before the one shown (`true`) or after.
    pub recall: Option<bool>,
    /// An interface chosen in the settings.
    pub choose: Option<Choice>,
    /// One of the orbit camera's preferences set in the settings.
    pub orbit: Option<settings::OrbitChange>,
    /// Log out was asked and its question answered Yes.
    pub log_out: bool,
    /// Exit Game was pressed.
    pub quit: bool,
    /// The client's actions asked for, in order (a hotbar slot used, an object picked on the
    /// minimap, a question answered, a skill trained).
    pub acts: Vec<Action>,
    /// What the player asked of the shell, in order (the world hidden while the story's scenes
    /// play, their sounds).
    pub requests: Vec<Request>,
}

/// A tooltip to draw over everything at the end of the frame: its title and its lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tip {
    pub title: String,
    pub lines: Vec<String>,
}

/// The interface.
#[derive(Debug)]
pub struct HorizonUi {
    art: Arc<Art>,
    time: f64,
    pub chat: chat::Chat,
    pub settings: settings::Settings,
    /// The orbit camera's preferences the settings show, as the client last listed them.
    pub orbit: Option<settings::Orbit>,
    pub hotbar: hotbar::Hotbar,
    pub skills: skills::SkillsWindow,
    pub examine: examine::ExamineWindow,
    /// A shortcut held by the pointer, from the hotbar or the skill window.
    pub drag: Option<hotbar::Drag>,
    pub minimap: minimap::Minimap,
    pub questions: questions::Questions,
    /// The question Log Out asks first.
    pub log_out: questions::LogOutQuestion,
    pub party: party::PartyList,
    pub social: social::SocialWindow,
    /// The windows of the player's things.
    pub goods: goods::Goods,
    pub map: map::MapWindow,
    pub journal: journal::JournalWindow,
    pub book: book::BookWindow,
    pub story: story::StoryPlayer,
    /// Where the HUD's elements stand, and its editor.
    pub layout: layout::HudLayout,
    /// Whether the HUD layout's editor was open as the log window last followed it.
    layout_followed: bool,
    /// The cross hotbars, shown in gamepad mode.
    pub crosses: Option<cross::Crosses>,
    /// The plugins' windows and HUDs.
    pub plugins: plugins::PluginSeat,
    /// The crafting window, the making window and the pets.
    pub crafting: crafting::CraftingWindow,
    pub making: making::MakingWindow,
    pub pets: pets::PetsPanel,
    /// The screens before the world.
    pub pregame: pregame::Screens,
    /// Which window is drawn over which.
    pub stack: stack::Stack,
    /// The menu bar's system menu is open.
    pub system_menu: bool,
    /// The game's crosshair, drawn over the world while the pointer is held for mouse look: its
    /// picture and where it stands.
    pub crosshair: Option<(u32, Rect)>,
    /// Where the crosshair was drawn this frame.
    crosshair_drawn: Option<Rect>,
    /// Where the stacking windows and the game's question were drawn this frame, for the ones
    /// drawn over them.
    windows_drawn: Vec<Rect>,
    covers: Vec<Rect>,
    tip: Option<Tip>,
}

impl HorizonUi {
    #[must_use]
    pub fn new(art: Arc<Art>) -> Self {
        Self {
            art,
            time: 0.0,
            chat: chat::Chat::default(),
            settings: settings::Settings::default(),
            orbit: None,
            hotbar: hotbar::Hotbar::default(),
            skills: skills::SkillsWindow::default(),
            examine: examine::ExamineWindow::default(),
            drag: None,
            minimap: minimap::Minimap::default(),
            questions: questions::Questions::default(),
            log_out: questions::LogOutQuestion::default(),
            party: party::PartyList::default(),
            social: social::SocialWindow::default(),
            goods: goods::Goods::default(),
            map: map::MapWindow::default(),
            journal: journal::JournalWindow::default(),
            book: book::BookWindow::default(),
            story: story::StoryPlayer::default(),
            layout: layout::HudLayout::default(),
            layout_followed: false,
            crosses: None,
            plugins: plugins::PluginSeat::default(),
            crafting: crafting::CraftingWindow::default(),
            making: making::MakingWindow::default(),
            pets: pets::PetsPanel::default(),
            pregame: pregame::Screens::default(),
            stack: stack::Stack::default(),
            system_menu: false,
            crosshair: None,
            crosshair_drawn: None,
            windows_drawn: Vec::new(),
            covers: Vec::new(),
            tip: None,
        }
    }

    /// Its clock: the seconds it has drawn frames for.
    #[must_use]
    pub const fn now(&self) -> f64 {
        self.time
    }

    /// The art it draws from.
    #[must_use]
    pub fn art(&self) -> &Arc<Art> {
        &self.art
    }

    /// Where the HUD layout's editor's body was last drawn, while it is open.
    #[must_use]
    pub fn layout_editor_body(&self) -> Option<Rect> {
        self.layout.body()
    }

    /// Screen pixels per layout unit on a window of `screen` pixels, at the scale the settings
    /// chose.
    #[must_use]
    pub fn layout_scale(&self, screen: (f32, f32)) -> f32 {
        crate::scale::layout_scale_at(screen, self.settings.own.scale)
    }

    /// The tooltip the last frame showed, if any.
    #[must_use]
    pub const fn tip(&self) -> Option<&Tip> {
        self.tip.as_ref()
    }

    /// Whether the last frame drew something that answers the pointer at `(x, y)`.
    #[must_use]
    pub fn covers(&self, x: f32, y: f32) -> bool {
        self.covers.iter().any(|r| r.contains(x, y))
    }

    /// Where the game's crosshair was drawn in the last frame, while the pointer is held for mouse
    /// look.
    #[must_use]
    pub const fn crosshair_drawn(&self) -> Option<Rect> {
        self.crosshair_drawn
    }

    /// Whether the usage pointer shows at `(x, y)` while an item waits for its target, as the
    /// game's own interface shows it: over the world, and over an item's icon in the inventory
    /// window (its other panels keep the default).
    #[must_use]
    pub fn usage_pointer_at(&self, x: f32, y: f32) -> bool {
        !self.covers(x, y) || self.goods.inventory.over_item()
    }

    /// Whether the chat entry, or a box of the social window or the windows of things, has the
    /// keys.
    #[must_use]
    pub fn typing(&self) -> bool {
        self.chat.typing()
            || self.settings.typing()
            || self.social.typing()
            || self.goods.typing()
            || self.making.typing()
            || self.pets.typing()
    }

    /// Draw one frame of `state` on a screen of `screen` pixels into `list`, `shown` the
    /// interface the client shows, and say what the player asked for.
    pub fn frame(
        &mut self,
        list: &mut DrawList,
        screen: (f32, f32),
        dt: f64,
        state: &GameState,
        input: &mut InputFrame,
        shown: Choice,
    ) -> Outcome {
        self.time += dt;
        list.clear();
        let art = Arc::clone(&self.art);
        let colours = dereth_ui_kit::colours::Colours::default();
        let mut p = Painter {
            list,
            art: &art,
            scale: self.layout_scale(screen),
            screen,
            fade: 1.0,
        };
        let mut out = Outcome::default();
        let coordinates = state.in_world.then_some(state.radar.show_coordinates);
        self.covers.clear();
        self.crosshair_drawn = None;
        self.windows_drawn.clear();
        self.stack.drawn.clear();
        let backing = art.palette().panel;
        self.tip = None;
        self.layout.begin_frame();
        // The windows are drawn over the HUD, the settings last: what lies under one does not
        // answer the pointer.
        input.occluders.clear();
        // The game's question is drawn over the HUD too.
        let question = if state.in_world {
            self.questions
                .up(state)
                .map(|q| questions::Questions::rect(&p, q))
        } else {
            None
        };
        // The log-out question is drawn over everything, the settings too.
        let leaving = self.log_out.rect(&p).filter(|_| state.in_world);
        input.occluders.extend(
            [
                self.skills.rect(),
                self.examine.rect(),
                self.map.rect(),
                self.journal.rect(),
                self.book.rect(),
                self.social.rect(),
                self.settings.rect(),
                question,
                leaving,
            ]
            .into_iter()
            .flatten(),
        );
        input.occluders.extend(self.goods.rects());
        // The crafting and making windows are drawn over the windows of things.
        let making = [self.crafting.rect(), self.making.rect()];
        input.occluders.extend(making.into_iter().flatten());
        // The plugins' windows lie over the HUD and under Horizon's own windows.
        let under_plugins = input.occluders.len();
        input.occluders.extend_from_slice(self.plugins.rects());
        // The chat's tabs popped out and its open menu lie over the rest of the HUD.
        let under_chat = input.occluders.len();
        if state.in_world {
            input.occluders.extend(self.chat.over_hud());
        }
        let mut ctx = Ctx {
            time: self.time,
            dt,
            input,
            colours: &colours,
            hot: false,
        };
        self.journal.hear(state);
        // What the social window keeps up shown or not, and a tell the scripts started.
        self.social.follow(state, &mut out.acts);
        if let Some(to) = state.social.start_tell.as_ref().filter(|_| state.in_world) {
            self.chat.open(&format!("/tell {to}, "));
        }
        // A recipe the game opened for making, and the pets' attacks and modes. The making window
        // comes to the front for a recipe opened while it is open, as a book the game opens while
        // the book window is open does.
        self.making.follow(state, &mut out.acts);
        if !state.craft.open_asked.is_empty() && self.making.is_open() {
            self.stack.raise(stack::Window::Making);
        }
        if state
            .journal
            .book
            .as_ref()
            .is_some_and(|b| b.opened != self.book.opened)
        {
            self.stack.raise(stack::Window::Book);
        }
        self.pets.follow(state, self.time, &mut out.acts);
        if state.in_world {
            self.story
                .update(state, self.time, &mut out.requests, &mut out.acts);
        } else {
            self.story.stop(&mut out.requests, &mut out.acts);
        }
        // While the story's scenes play they cover the screen: only the game's question and the
        // settings are drawn over them.
        if let Some(r) = state
            .in_world
            .then(|| self.story.draw(&mut p, self.time, state))
            .flatten()
        {
            self.covers.push(r);
            ctx.input.occluders.clear();
            ctx.input.occluders.extend(self.settings.rect());
            ctx.input.occluders.extend(leaving);
            if let Some(r) = self.questions.frame(&mut p, &mut ctx, state, &mut out) {
                self.covers.push(r);
            }
        } else if state.in_world {
            use layout::element as e;
            let k = p.scale;
            // The selection's marks lie over the world, under the HUD; the crosshair too.
            marker::draw(&mut p, state, self.time);
            if let Some((picture, at)) = self.crosshair {
                if let Some(s) = p.art.icon(picture) {
                    p.sprite(&s, at, WHITE);
                    self.crosshair_drawn = Some(at);
                }
            }
            let screen = p.screen;
            // Health and vigor stand beside a hero's focus gauge at its default place, as they
            // do in the editor for anyone; else they close up.
            let focus_beside = (state.hero || self.layout.editing())
                && self.layout.placement(e::FOCUS).is_default();
            let o = self.layout.begin(e::PARAMETER, &p, &mut ctx);
            hud::parameter_bar(&mut p, &mut ctx, state, focus_beside, &mut self.tip);
            self.layout.end(o, &mut p, &mut ctx);
            let o = self.layout.begin(e::FOCUS, &p, &mut ctx);
            hud::focus_gauge(&mut p, &mut ctx, state, &mut self.tip);
            self.layout
                .end_in(o, &mut p, &mut ctx, hud::focus_rect(screen, k));
            // An element that may show nothing is placed by where it would show, so the editor
            // outlines it either way.
            let o = self.layout.begin(e::TARGET, &p, &mut ctx);
            hud::target_bar(&mut p, state);
            self.layout
                .end_in(o, &mut p, &mut ctx, hud::target_rect(screen, k));
            // The link line, named in the editor before the status row over it.
            let o = self.layout.begin(e::LINK, &p, &mut ctx);
            status::link_line(&mut p, &mut ctx, state, &mut self.tip);
            self.layout.end(o, &mut p, &mut ctx);
            let o = self.layout.begin(e::STATUS, &p, &mut ctx);
            hud::effects(&mut p, &mut ctx, state, &mut self.tip);
            self.layout
                .end_in(o, &mut p, &mut ctx, hud::effects_rect(screen, k));
            // The conflict lamp and the vitae lamp beside it are placed together.
            let o = self.layout.begin(e::LAMP, &p, &mut ctx);
            party::conflict_lamp(&mut p, &mut ctx, state, &mut self.tip);
            let (vitae, examine_vitae) = status::vitae_lamp(&mut p, &mut ctx, state, &mut self.tip);
            let lamps = status::vitae_rect(screen, k);
            let conflict = party::lamp_rect(screen, k);
            let lamps = Rect::new(lamps.x, lamps.y, conflict.right() - lamps.x, lamps.h);
            self.layout.end_in(o, &mut p, &mut ctx, lamps);
            self.covers
                .extend(vitae.map(|r| self.layout.map_rect(e::LAMP, r, k)));
            if let Some(ask) = examine_vitae {
                out.acts.push(ask.action());
                self.examine.open(&mut out.acts);
                self.stack.raise(stack::Window::Examine);
            }
            let o = self.layout.begin(e::EXPERIENCE, &p, &mut ctx);
            status::experience_band(&mut p, &mut ctx, state, &mut self.tip);
            self.layout
                .end_in(o, &mut p, &mut ctx, status::experience_rect(screen, k));
            let o = self.layout.begin(e::PARTY, &p, &mut ctx);
            let rows = self
                .party
                .frame(&mut p, &mut ctx, state, &mut self.tip, &mut out.acts);
            self.layout
                .end_in(o, &mut p, &mut ctx, party::row_rect(k, 0));
            self.covers.extend(
                rows.into_iter()
                    .map(|r| self.layout.map_rect(e::PARTY, r, k)),
            );
            let o = self.layout.begin(e::MENU, &p, &mut ctx);
            let lit = hud::MenuButton::ALL.map(|b| self.menu_lit(b));
            let (pick, menu) =
                hud::menu_bar(&mut p, &mut ctx, lit, &mut self.system_menu, &mut self.tip);
            self.layout.end(o, &mut p, &mut ctx);
            self.covers.extend(
                menu.into_iter()
                    .map(|r| self.layout.map_rect(e::MENU, r, k)),
            );
            if let Some(pick) = pick {
                self.menu_picked(pick, &mut out);
            }
            // The log window's own tabs popped out and its menu are over it, but the pointer over
            // them is still the log window's to know of: a press there is not a press elsewhere.
            let own = ctx
                .input
                .occluders
                .split_off(under_chat.min(ctx.input.occluders.len()));
            self.follow_layout_editor();
            let o = self.layout.begin(e::CHAT, &p, &mut ctx);
            ctx.input.occluders.extend(own);
            let log = self
                .chat
                .frame(&mut p, &mut ctx, state, &mut self.settings.own, &mut out);
            self.layout.end(o, &mut p, &mut ctx);
            let placed = &self.layout;
            self.chat.place(|r| placed.map_rect(e::CHAT, r, k));
            self.covers.push(self.layout.map_rect(e::CHAT, log, k));
            let mut asked = hotbar::Asked::default();
            let o = self.layout.begin(e::HOTBAR, &p, &mut ctx);
            let bar = self.hotbar.frame(
                &mut p,
                &mut ctx,
                state,
                &mut self.drag,
                &mut self.tip,
                &mut asked,
            );
            self.layout.end(o, &mut p, &mut ctx);
            let placed = &self.layout;
            self.hotbar.place(|r| placed.map_rect(e::HOTBAR, r, k));
            self.covers.extend(
                bar.into_iter()
                    .map(|r| self.layout.map_rect(e::HOTBAR, r, k)),
            );
            // The cross hotbars show in gamepad mode, and are placed in the editor either way.
            if self.crosses.is_some() || self.layout.editing() {
                let o = self.layout.begin(e::CROSS, &p, &mut ctx);
                if let Some(c) = self.crosses {
                    cross::frame(&mut p, state, c.held);
                }
                let rows = state.bar.as_ref().map_or(1, |b| b.rows.max(1));
                self.layout
                    .end_in(o, &mut p, &mut ctx, cross::home(screen, k, rows));
            }
            out.acts.append(&mut asked.actions);
            self.examine
                .set_classic(state.examined.classic(), &mut out.acts);
            if let Some(e) = asked.examine {
                out.acts.push(e.action());
                self.examine.open(&mut out.acts);
                self.stack.raise(stack::Window::Examine);
            }
            if state.examined.open_asked {
                self.examine.open(&mut out.acts);
                self.stack.raise(stack::Window::Examine);
            }
            let o = self.layout.begin(e::MINIMAP, &p, &mut ctx);
            let minimap = self
                .minimap
                .frame(&mut p, &mut ctx, state, &mut self.tip, &mut out);
            self.layout.end(o, &mut p, &mut ctx);
            self.covers
                .push(self.layout.map_rect(e::MINIMAP, minimap, k));
            let time = self.time;
            let o = self.layout.begin(e::PETS, &p, &mut ctx);
            let pets = self
                .pets
                .frame(&mut p, &mut ctx, state, time, &mut self.tip, &mut out.acts);
            self.layout
                .end_in(o, &mut p, &mut ctx, pets::Layout::of(screen, k, 1).panel);
            self.covers.extend(
                pets.into_iter()
                    .map(|r| self.layout.map_rect(e::PETS, r, k)),
            );
            let o = self.layout.begin(e::DISMOUNT, &p, &mut ctx);
            let dismount = pets::dismount(&mut p, &mut ctx, state, &mut out.acts);
            self.layout
                .end_in(o, &mut p, &mut ctx, pets::dismount_rect(screen, k));
            self.covers.extend(
                dismount
                    .into_iter()
                    .map(|r| self.layout.map_rect(e::DISMOUNT, r, k)),
            );
            // The chat's tabs popped out and its menu, over the HUD and under the windows.
            let chat_occluders = std::mem::take(&mut ctx.input.occluders);
            ctx.input.occluders = chat_occluders[..under_chat.min(chat_occluders.len())].to_vec();
            self.chat
                .overlays(&mut p, &mut ctx, state, &mut self.settings.own, &mut out);
            ctx.input.occluders = chat_occluders;
            if self.chat.take_kept() {
                self.settings.own_set();
            }
            self.covers.extend(self.chat.over_hud());
            self.layout.others_unsaved = self.chat.places_unsaved();
            let edited = self.layout.edit(&mut p, &mut ctx);
            match self.layout.take_pressed() {
                Some(layout::Pressed::Save) => self.chat.save_places(),
                Some(layout::Pressed::Reset) => self.chat.reset_places(),
                None => {}
            }
            if let Some(r) = edited {
                // While it edits, every element's outline is the interface's to press on.
                self.covers.push(r);
                self.covers
                    .extend(self.layout.outlines.iter().map(|(_, r)| *r));
            }
            ctx.input.occluders.truncate(under_plugins);
            let drawn = self.plugins.frame(&mut p, &mut ctx, &mut out.requests);
            self.covers.extend(drawn);
            let below = self.plugins.rects().to_vec();
            // A window opening keeps clear of the HUD as the player's layout has it.
            let placed = &self.layout;
            place::set_hud(Some(place::hud(
                screen,
                k,
                state,
                self.chat.window(screen, k),
                |name, r| placed.map_rect(name, r, k),
            )));
            self.stacked(&mut p, &mut ctx, state, question, below, &mut out);
            place::set_hud(None);
            // Follow set on the map is kept with Horizon's own settings.
            if let Some(on) = self.map.take_follow_set() {
                self.settings.own.map_follow = on;
                self.settings.own_set();
            }
            // An item pressed with the right button is examined in the examine window.
            if let Some(item) = self.goods.carry.examine.take() {
                let e = hotbar::Examine {
                    kind: auberean_client_sdk::contract::notice::examination::kind::ITEM,
                    data_enum: 0,
                    item,
                    data_id: 0,
                };
                out.acts.push(e.action());
                self.examine.open(&mut out.acts);
                self.stack.raise(stack::Window::Examine);
            }
            ctx.input.occluders.clear();
            ctx.input.occluders.extend(self.settings.rect());
            ctx.input.occluders.extend(leaving);
            let from = p.list.quads.len();
            if let Some(r) = self.questions.frame(&mut p, &mut ctx, state, &mut out) {
                stack::back(p.list, from, r, &self.windows_drawn, backing);
                self.covers.push(r);
                self.windows_drawn.push(r);
            }
            self.follow_drag(&mut p, &mut ctx, state, &mut out);
        } else {
            self.drag = None;
            self.layout.close_editor();
        }
        ctx.input.occluders.clear();
        ctx.input.occluders.extend(leaving);
        let from = p.list.quads.len();
        if let Some(r) =
            self.settings
                .frame(&mut p, &mut ctx, shown, self.orbit, coordinates, &mut out)
        {
            stack::back(p.list, from, r, &self.windows_drawn, backing);
            self.covers.push(r);
        }
        if self.settings.take_layout_asked() {
            self.layout.open_editor();
        }
        if self.settings.take_log_out_asked() {
            self.log_out.ask();
        }
        if state.in_world {
            ctx.input.occluders.clear();
            let from = p.list.quads.len();
            if let Some(r) = self.log_out.frame(&mut p, &mut ctx, &mut out) {
                stack::back(p.list, from, r, &self.windows_drawn, backing);
                self.covers.push(r);
            }
        } else {
            self.log_out.dismiss();
        }
        if state.in_world {
            self.finish_goods(&mut p, &ctx, state, question, &mut out);
        } else {
            self.goods.carry.drag = None;
        }
        if let Some(t) = &self.tip {
            dereth_ui_kit::widgets::tooltip(&mut p, &ctx, &t.title, &t.lines);
        }
        out
    }

    /// Draw one frame before the world of `pre` on a screen of `screen` pixels into `list`, the
    /// credits' lines at hand when they are shown, and say in `out` what the player asked for.
    #[allow(clippy::too_many_arguments)]
    pub fn pregame_frame(
        &mut self,
        list: &mut DrawList,
        screen: (f32, f32),
        dt: f64,
        pre: &crate::pregame::Pregame,
        input: &mut InputFrame,
        has_login: bool,
        credits: &[String],
        out: &mut pregame::PreOut,
    ) -> pregame::Screen {
        self.time += dt;
        list.clear();
        let art = Arc::clone(&self.art);
        let colours = dereth_ui_kit::colours::Colours::default();
        let mut p = Painter {
            list,
            art: &art,
            scale: crate::scale::layout_scale(screen),
            screen,
            fade: 1.0,
        };
        self.covers.clear();
        input.occluders.clear();
        let mut ctx = Ctx {
            time: self.time,
            dt,
            input,
            colours: &colours,
            hot: false,
        };
        self.pregame
            .frame(&mut p, &mut ctx, pre, has_login, credits, out)
    }

    /// A thing of the windows of things put down: where it goes, and an item put down on the
    /// hotbar made a shortcut there.
    fn finish_goods(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &Ctx<'_>,
        state: &GameState,
        question: Option<Rect>,
        out: &mut Outcome,
    ) {
        let (x, y) = ctx.input.mouse;
        // The windows stacked over the windows of things take what is put down on them, but for
        // the crafting and making windows, which take it on their own slots.
        let order = self.stack.order();
        let above = order
            .iter()
            .position(|w| *w == stack::Window::Goods)
            .map_or(&[][..], |i| &order[i + 1..]);
        let over_window = self
            .window_rects()
            .into_iter()
            .filter(|(w, _)| {
                above.contains(w) && !matches!(w, stack::Window::Crafting | stack::Window::Making)
            })
            .any(|(_, rs)| rs.iter().any(|r| r.contains(x, y)));
        let over_later = over_window
            || [self.settings.rect(), question]
                .into_iter()
                .flatten()
                .any(|r| r.contains(x, y));
        let over_any = self.covers(x, y);
        let left = self.goods.finish(p, ctx, state, over_later, over_any, out);
        // An item put down on a slot of the making window, or carried out of one.
        let left = match left {
            Some(items::Put::On(c, z)) if self.making.put(c, Some(z), &mut out.acts) => return,
            Some(items::Put::World(c, _) | items::Put::Nowhere(c))
                if self.making.put(c, None, &mut out.acts) =>
            {
                return;
            }
            other => other,
        };
        let Some(items::Put::Nowhere(c)) = left else {
            return;
        };
        let (Some(item), Some(bar)) = (c.item(), &state.bar) else {
            return;
        };
        if !matches!(c, items::Carried::Item { .. } | items::Carried::Worn { .. }) {
            return;
        }
        let d = hotbar::Drag {
            shortcut: auberean_client_sdk::contract::ToolbarShortcut {
                kind: auberean_client_sdk::contract::shortcut_kind::ITEM,
                item,
                ..Default::default()
            },
            from: None,
            icon: state.goods.item(item).map_or(0, |it| it.icon),
            origin: (x, y),
            active: true,
        };
        out.acts
            .extend(self.hotbar.drop(&d, (x, y), ctx.input.shift, bar));
    }

    /// The shortcut held: dragged once the pointer moves far enough, drawn under the pointer, and
    /// let go of when the button comes up.
    fn follow_drag(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let Some(d) = &mut self.drag else {
            return;
        };
        let (x, y) = ctx.input.mouse;
        if !d.active
            && ((x - d.origin.0).abs() > hotbar::DRAG_SLOP
                || (y - d.origin.1).abs() > hotbar::DRAG_SLOP)
        {
            d.active = true;
        }
        hotbar::draw_drag(p, d, ctx.input.mouse);
        if ctx.input.down[0] {
            return;
        }
        let d = self.drag.take().unwrap_or_else(|| unreachable!());
        if let Some(bar) = &state.bar {
            out.acts
                .extend(self.hotbar.drop(&d, (x, y), ctx.input.shift, bar));
        }
    }

    /// How much of each stacking window is open now: the windows of things count each of theirs.
    fn shown_windows(&self) -> Vec<(stack::Window, usize)> {
        use stack::Window as W;
        W::ALL
            .into_iter()
            .map(|w| {
                let n = match w {
                    W::Skills => usize::from(self.skills.is_open()),
                    W::Examine => usize::from(self.examine.is_open()),
                    W::Social => usize::from(self.social.is_open()),
                    W::Map => usize::from(self.map.is_open()),
                    W::Journal => usize::from(self.journal.is_open()),
                    W::Book => usize::from(self.book.is_open()),
                    W::Goods => self
                        .goods
                        .rects()
                        .len()
                        .max(usize::from(self.goods.inventory.is_open())),
                    W::Crafting => usize::from(self.crafting.is_open()),
                    W::Making => usize::from(self.making.is_open()),
                };
                (w, n)
            })
            .collect()
    }

    /// Where each stacking window was last drawn.
    fn window_rects(&self) -> Vec<(stack::Window, Vec<Rect>)> {
        use stack::Window as W;
        W::ALL
            .into_iter()
            .map(|w| {
                let r = match w {
                    W::Skills => self.skills.rect(),
                    W::Examine => self.examine.rect(),
                    W::Social => self.social.rect(),
                    W::Map => self.map.rect(),
                    W::Journal => self.journal.rect(),
                    W::Book => self.book.rect(),
                    W::Goods => return (w, self.goods.rects()),
                    W::Crafting => self.crafting.rect(),
                    W::Making => self.making.rect(),
                };
                (w, r.into_iter().collect())
            })
            .collect()
    }

    /// The windows the player opens, the front one last ([`stack`]): one opened since last frame,
    /// or pressed on now, brought to the front first. Each window takes the pointer only where
    /// the windows over it, the settings and the game's question do not, and is backed where it
    /// lies over a window drawn before it or over `below`.
    fn stacked(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        question: Option<Rect>,
        mut below: Vec<Rect>,
        out: &mut Outcome,
    ) {
        use stack::Window as W;
        self.stack.follow(&self.shown_windows());
        let top: Vec<Rect> = [self.settings.rect(), question]
            .into_iter()
            .flatten()
            .collect();
        let last = self.window_rects();
        if ctx.input.pressed[0] || ctx.input.pressed[1] {
            let at = ctx.input.mouse;
            if !top.iter().any(|r| r.contains(at.0, at.1)) {
                if let Some(w) = self.stack.pressed_on(&last, at) {
                    self.stack.raise(w);
                }
            }
        }
        let order = self.stack.order().to_vec();
        let backing = p.art.palette().panel;
        // The places that take a carried thing are drawn anew, in whichever windows have them.
        self.goods.carry.begin(ctx);
        for (i, w) in order.iter().enumerate() {
            ctx.input.occluders.clear();
            for later in &order[i + 1..] {
                for (_, rs) in last.iter().filter(|(o, _)| o == later) {
                    ctx.input.occluders.extend_from_slice(rs);
                }
            }
            ctx.input.occluders.extend_from_slice(&top);
            // A window opening keeps clear of the others open.
            let mut open = below.clone();
            for (_, rs) in last
                .iter()
                .filter(|(o, _)| o != w && !order[..i].contains(o))
            {
                open.extend_from_slice(rs);
            }
            place::set_open(open);
            let from = p.list.quads.len();
            let drawn: Vec<Rect> = match w {
                W::Skills => self
                    .skills
                    .frame(p, ctx, state, &mut self.drag, &mut self.tip, &mut out.acts)
                    .into_iter()
                    .collect(),
                W::Examine => self
                    .examine
                    .frame(p, ctx, state, &mut out.acts)
                    .into_iter()
                    .collect(),
                W::Social => self
                    .social
                    .frame(p, ctx, state, &mut out.acts)
                    .into_iter()
                    .collect(),
                W::Map => self
                    .map
                    .frame(p, ctx, state, &mut self.tip)
                    .into_iter()
                    .collect(),
                W::Journal => self
                    .journal
                    .frame(p, ctx, state, &mut out.acts)
                    .into_iter()
                    .collect(),
                W::Book => self.book.frame(p, ctx, state).into_iter().collect(),
                W::Goods => {
                    // The windows of things back each of their own over the others themselves.
                    self.goods.below.clone_from(&below);
                    let things = self.goods.frame(p, ctx, state, out, &mut self.tip);
                    self.stack.drawn.push((*w, from..p.list.quads.len()));
                    self.covers.extend_from_slice(&things);
                    below.extend(things);
                    continue;
                }
                W::Crafting => self
                    .crafting
                    .frame(p, ctx, state, &mut self.drag, &mut self.tip, &mut out.acts)
                    .into_iter()
                    .collect(),
                W::Making => self
                    .making
                    .frame(
                        p,
                        ctx,
                        state,
                        &mut self.goods.carry,
                        &mut self.tip,
                        &mut out.acts,
                    )
                    .into_iter()
                    .collect(),
            };
            for r in &drawn {
                stack::back(p.list, from, *r, &below, backing);
            }
            self.stack.drawn.push((*w, from..p.list.quads.len()));
            self.covers.extend_from_slice(&drawn);
            below.extend(drawn);
        }
        place::set_open(Vec::new());
        self.windows_drawn = below;
    }

    /// Whether the menu bar's button `b` is lit: its window is open (the social window on any
    /// tab).
    fn menu_lit(&self, b: hud::MenuButton) -> bool {
        use hud::MenuButton as B;
        match b {
            B::Map => self.map.is_open(),
            B::Examine => self.examine.is_open(),
            B::Inventory => self.goods.inventory.is_open(),
            B::Skills => self.skills.is_open(),
            B::Crafting => self.crafting.is_open(),
            B::Quests => self.journal.is_open(),
            B::Social => self.social.is_open(),
            B::System => self.settings.is_open(),
        }
    }

    /// Open the map, following the player as Horizon's settings say, or close it.
    pub fn toggle_map(&mut self) {
        self.map.toggle(self.settings.own.map_follow);
    }

    /// What a press on the menu bar does: a window opened or closed (the one opened brought to
    /// the front), the settings or the HUD layout's editor opened, logging out or leaving the game.
    fn menu_picked(&mut self, pick: hud::MenuPick, out: &mut Outcome) {
        use hud::{MenuButton as B, MenuPick, SystemItem as S};
        use stack::Window as W;
        let raise = match pick {
            MenuPick::Button(B::Map) => {
                self.toggle_map();
                Some(W::Map)
            }
            MenuPick::Button(B::Examine) => {
                self.examine.toggle(&mut out.acts);
                Some(W::Examine)
            }
            MenuPick::Button(B::Inventory) => {
                self.goods.inventory.toggle();
                Some(W::Goods)
            }
            MenuPick::Button(B::Skills) => {
                self.skills.toggle();
                Some(W::Skills)
            }
            MenuPick::Button(B::Crafting) => {
                self.crafting.toggle();
                Some(W::Crafting)
            }
            MenuPick::Button(B::Quests) => {
                self.journal.toggle();
                Some(W::Journal)
            }
            MenuPick::Button(B::Social) => {
                self.social.toggle(&mut out.acts);
                Some(W::Social)
            }
            MenuPick::Button(B::System) => None,
            MenuPick::System(S::Settings) => {
                self.settings.toggle();
                None
            }
            MenuPick::System(S::Layout) => {
                self.layout.open_editor();
                None
            }
            MenuPick::System(S::LogOut) => {
                self.log_out.ask();
                None
            }
            MenuPick::System(S::Quit) => {
                out.quit = true;
                None
            }
        };
        if let Some(w) = raise {
            self.stack.raise(w);
        }
    }

    /// The log window follows the HUD layout's editor: while it is open its tabs popped out are
    /// moved and sized as the HUD's elements are, and closed, what it changed of them and did not
    /// save is put back.
    fn follow_layout_editor(&mut self) {
        let editing = self.layout.editing();
        if editing != self.layout_followed {
            if editing {
                self.chat.begin_layout();
            } else {
                self.chat.revert_places();
            }
            self.layout_followed = editing;
        }
        self.chat.editing = editing;
    }

    /// Whether one of the windows is open (or the story's scenes may be cut short).
    #[must_use]
    pub fn window_open(&self) -> bool {
        self.settings.is_open()
            || self.layout.editing()
            || self.skills.is_open()
            || self.examine.is_open()
            || self.map.is_open()
            || self.journal.is_open()
            || self.book.is_open()
            || self.story.skippable()
            || self.social.is_open()
            || self.goods.any_open()
            || self.crafting.is_open()
            || self.making.is_open()
    }

    /// Close the window on top: the log-out question (unanswered), the menu bar's system menu, the
    /// settings, the HUD layout's editor, the story's scenes, else the front one of the windows
    /// (the making window only while it is not making, the windows of things their own top one);
    /// whether one was open.
    pub fn close_top(&mut self, actions: &mut Vec<Action>) -> bool {
        use stack::Window as W;
        if self.log_out.is_up() {
            self.log_out.dismiss();
            return true;
        }
        if self.system_menu {
            self.system_menu = false;
            return true;
        }
        if self.settings.is_open() {
            self.settings.close();
            return true;
        }
        if self.layout.editing() {
            self.layout.close_editor();
            return true;
        }
        if self.story.skippable() {
            self.story.skip();
            return true;
        }
        self.stack.follow(&self.shown_windows());
        let order = self.stack.order().to_vec();
        for w in order.into_iter().rev() {
            match w {
                W::Making if self.making.is_open() && !self.making.making => {
                    self.making.close(actions);
                }
                W::Crafting if self.crafting.is_open() => self.crafting.close(),
                W::Goods if self.goods.close_top(actions) => {}
                W::Book if self.book.is_open() => self.book.close(),
                W::Journal if self.journal.is_open() => self.journal.close(),
                W::Map if self.map.is_open() => self.map.close(),
                W::Social if self.social.is_open() => self.social.close(actions),
                W::Examine if self.examine.is_open() => self.examine.close(actions),
                W::Skills if self.skills.is_open() => self.skills.close(),
                _ => continue,
            }
            return true;
        }
        false
    }

    /// Every window closed and the shortcut held let go of, as the interface is left.
    pub fn close_all(&mut self, actions: &mut Vec<Action>) {
        self.chat.close();
        self.system_menu = false;
        self.settings.close();
        self.layout.close_editor();
        self.skills.close();
        self.examine.close(actions);
        self.map.close();
        self.journal.close();
        self.book.close();
        self.social.close(actions);
        self.goods.close_all();
        self.crafting.close();
        self.making.making = false;
        self.making.close(actions);
        self.drag = None;
    }

    /// What a key's action asks, the game as `state` has it: a hotbar slot used, a set shown,
    /// attacking started or stopped, a window opened or closed, the examine window's tab stepped,
    /// the log-out question asked.
    pub fn keyed(&mut self, key: crate::keys::Keyed, state: &GameState) -> Vec<Action> {
        use crate::keys::Keyed as K;
        let mut actions = Vec::new();
        match key {
            K::Shortcut { row, column } => {
                if let Some(bar) = &state.bar {
                    let slot = bar.first() + row * hotbar::PER_ROW + column;
                    if slot < auberean_client_sdk::contract::TOOLBAR_SLOTS && row < bar.rows {
                        actions.push(Action::UseToolbarSlot { slot });
                    }
                }
            }
            K::Set(set) => actions.push(Action::ToolbarSet { set }),
            K::StepSet { next } => {
                if let Some(bar) = &state.bar {
                    actions.push(Action::ToolbarSet {
                        set: bar.stepped(if next { 1 } else { -1 }),
                    });
                }
            }
            K::Attack => actions.push(Action::Attack {
                start: !state.combat.attacking,
            }),
            K::Skills => self.skills.toggle(),
            K::Inventory => self.goods.inventory.toggle(),
            K::Examine => {
                self.examine
                    .set_classic(state.examined.classic(), &mut actions);
                self.examine.toggle(&mut actions);
            }
            K::StepTab { next } => {
                self.examine
                    .set_classic(state.examined.classic(), &mut actions);
                self.examine.step(next, state.examined.made, &mut actions);
            }
            K::Map => self.toggle_map(),
            K::Journal => self.journal.toggle(),
            K::Social => self.social.toggle(&mut actions),
            K::Crafting => self.crafting.toggle(),
            K::ChatTab(tab) => self.chat.window_key(tab),
            K::Fellow(n) => {
                if let Some(f) = party::rows(state).get(n) {
                    actions.push(Action::SetSelection { target: f.id });
                }
            }
            K::LogOut => self.log_out.ask(),
            K::Chat { .. } | K::Reply | K::Settings | K::Screenshot => {}
        }
        actions
    }
}
