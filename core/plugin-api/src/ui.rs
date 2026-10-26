//! The declared UI tree: what a plugin's window is, never where its pixels go.
//!
//! A plugin describes a window as a tree of typed nodes; each host renders it its own way. The tree
//! crosses from the plugin as a [`Value`] of this shape:
//!
//! ```text
//! { kind = "window", id = "main", title = "XP", children = { { kind = "label", id = "rate", text = "0/h" } } }
//! ```
//!
//! [`UiNode::from_value`] checks it: a known kind, only that kind's properties, each of the right
//! type, and ids unique within the window. A property changed later goes through
//! [`UiNode::set_prop`], which applies the same checks.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

use crate::value::Value;

/// A node's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Window,
    /// A small always-on-screen window with no frame, title or close box, placed by an anchor.
    Hud,
    Row,
    Column,
    Group,
    Separator,
    Label,
    Button,
    Toggle,
    Meter,
    List,
    /// A picture: a PNG from the plugin's package or one it fetched, by name.
    Image,
    /// Pages under tabs: its children are groups, each a page with its title on its tab.
    Tabs,
    /// A number the player steps down and up between a least and a most.
    Number,
    /// One of a list of words, the player stepping through them.
    Choice,
    /// A free drawing: lines, rectangles, circles and text at pixels of its own.
    Canvas,
    /// Series of numbers drawn as lines or bars.
    Chart,
    /// One line of text the player types.
    Input,
    /// Lines of text: shown, or typed when `editable`.
    TextBox,
    /// A number the player slides between a least and a most.
    Slider,
    /// A key the host binds to one of the plugin's commands.
    Hotkey,
}

/// The type a property holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PropType {
    Text,
    Bool,
    Int,
    Number,
    /// One of a fixed set of words.
    Word(&'static [&'static str]),
    /// A list of strings.
    Strings,
    /// A list of rows, each a list of strings or numbers.
    Rows,
    /// A row index, or nil for none.
    Index,
    /// A canvas's draw commands (see [`crate::draw`]).
    Drawing,
    /// A chart's series (see [`crate::draw`]).
    Series,
    /// A colour, `"#RRGGBB"` or a number.
    Color,
    /// A key chord, `Ctrl+…` or `Alt+…` (see [`chord`]).
    Chord,
    /// A share of the room left over: a number from 0 up.
    Grow,
    /// A list's columns: each a heading, or `{ title, width, grow, min_width }`.
    Columns,
}

impl PropType {
    /// The type in words: the expectation a refusal names, with a word type's words listed.
    fn describe(self) -> String {
        match self {
            Self::Word(words) => format!(
                "one of {}",
                words
                    .iter()
                    .map(|w| format!("`{w}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            other => other.expected().to_owned(),
        }
    }

    /// What a value of this type must be, as a refusal says it.
    const fn expected(self) -> &'static str {
        match self {
            Self::Text => "a string",
            Self::Bool => "a boolean",
            Self::Int => "a whole number from 0 to 10000",
            Self::Number => "a number",
            Self::Word(_) => "one of the named words",
            Self::Strings => "a list of strings",
            Self::Rows => "a list of rows, each a list of cells: a string, a number, or `{ text, color, icon }`",
            Self::Index => "a row number from 1, or nil",
            Self::Drawing => "a list of draw commands (`line`, `rect`, `circle`, `text`)",
            Self::Series => "a list of series, each `{ values, color, label }`",
            Self::Color => "a colour, \"#RRGGBB\" or a number 0xRRGGBB",
            Self::Chord => "a key with Ctrl or Alt, as \"Ctrl+Shift+F5\" or \"Alt+K\"",
            Self::Grow => "a number from 0 to 1000",
            Self::Columns => "a list of columns, each a heading or `{ title, width, grow, min_width }`",
        }
    }
}

/// The meter styles: the colour of the retail vital bars, or a plain one.
pub const METER_STYLES: &[&str] = &["neutral", "health", "stamina", "mana"];

/// How a container sets its nodes across: stretched to its width (a column) or height (a row), or
/// each at its own size at the start, the middle or the end.
pub const ALIGNS: &[&str] = &["stretch", "start", "center", "end"];

/// Where a container puts the room its nodes leave over along it.
pub const JUSTIFIES: &[&str] = &["start", "center", "end", "between"];

/// What every node in a window takes but the window itself: its share of the room left over along
/// its container, and its bounds (cells across, lines down).
const FLEX: &[(&str, PropType)] = &[
    ("grow", PropType::Grow),
    ("min_width", PropType::Int),
    ("max_width", PropType::Int),
    ("min_height", PropType::Int),
    ("max_height", PropType::Int),
];

/// What a node that sets others in a line takes: the gap between them (layout units), how they
/// are set across, and where the room they leave goes.
const LINE: &[(&str, PropType)] = &[
    ("gap", PropType::Int),
    ("align", PropType::Word(ALIGNS)),
    ("justify", PropType::Word(JUSTIFIES)),
];

/// Where a HUD sits on the screen: a hint the host places it by.
pub const HUD_ANCHORS: &[&str] = &[
    "top_left",
    "top",
    "top_right",
    "left",
    "center",
    "right",
    "bottom_left",
    "bottom",
    "bottom_right",
];

impl NodeKind {
    pub const ALL: [Self; 21] = [
        Self::Window,
        Self::Hud,
        Self::Row,
        Self::Column,
        Self::Group,
        Self::Separator,
        Self::Label,
        Self::Button,
        Self::Toggle,
        Self::Meter,
        Self::List,
        Self::Image,
        Self::Tabs,
        Self::Number,
        Self::Choice,
        Self::Canvas,
        Self::Chart,
        Self::Input,
        Self::TextBox,
        Self::Slider,
        Self::Hotkey,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Hud => "hud",
            Self::Row => "row",
            Self::Column => "column",
            Self::Group => "group",
            Self::Separator => "separator",
            Self::Label => "label",
            Self::Button => "button",
            Self::Toggle => "toggle",
            Self::Meter => "meter",
            Self::List => "list",
            Self::Image => "image",
            Self::Tabs => "tabs",
            Self::Number => "number",
            Self::Choice => "choice",
            Self::Canvas => "canvas",
            Self::Chart => "chart",
            Self::Input => "input",
            Self::TextBox => "textbox",
            Self::Slider => "slider",
            Self::Hotkey => "hotkey",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == s)
    }

    /// Whether a HUD may hold the kind. A HUD's only controls are buttons, toggles and list rows:
    /// nothing in it is typed in, dragged, stepped or opened over the screen.
    #[must_use]
    pub const fn in_hud(self) -> bool {
        !matches!(
            self,
            Self::Input | Self::TextBox | Self::Number | Self::Choice | Self::Slider | Self::Tabs
        )
    }

    /// Whether the kind holds other nodes.
    #[must_use]
    pub const fn is_container(self) -> bool {
        matches!(
            self,
            Self::Window | Self::Hud | Self::Row | Self::Column | Self::Group | Self::Tabs
        )
    }

    fn props(self) -> &'static [(&'static str, PropType)] {
        const W: (&str, PropType) = ("width", PropType::Int);
        const T: (&str, PropType) = ("tooltip", PropType::Text);
        match self {
            Self::Window => &[
                ("title", PropType::Text),
                W,
                ("height", PropType::Int),
                ("resizable", PropType::Bool),
                ("min_width", PropType::Int),
                ("min_height", PropType::Int),
                ("max_width", PropType::Int),
                ("max_height", PropType::Int),
            ],
            Self::Hud => &[("anchor", PropType::Word(HUD_ANCHORS)), W],
            Self::Row | Self::Column | Self::Separator => &[],
            Self::Group => &[("title", PropType::Text)],
            Self::Label => &[("text", PropType::Text), ("color", PropType::Color), W, T],
            Self::Button => &[("text", PropType::Text), ("enabled", PropType::Bool), W, T],
            Self::Toggle => &[
                ("text", PropType::Text),
                ("value", PropType::Bool),
                ("enabled", PropType::Bool),
                T,
            ],
            Self::Meter => &[
                ("value", PropType::Number),
                ("max", PropType::Number),
                ("style", PropType::Word(METER_STYLES)),
                ("text", PropType::Text),
                W,
                T,
            ],
            Self::List => &[
                ("columns", PropType::Columns),
                ("rows", PropType::Rows),
                ("selected", PropType::Index),
                ("height", PropType::Int),
                W,
                T,
            ],
            Self::Image => &[
                ("src", PropType::Text),
                ("icon", PropType::Number),
                ("height", PropType::Int),
                ("text", PropType::Text),
                W,
                T,
            ],
            Self::Tabs => &[("selected", PropType::Index)],
            Self::Number => &[
                ("text", PropType::Text),
                ("value", PropType::Number),
                ("min", PropType::Number),
                ("max", PropType::Number),
                ("step", PropType::Number),
                ("enabled", PropType::Bool),
                T,
            ],
            Self::Choice => &[
                ("text", PropType::Text),
                ("options", PropType::Strings),
                ("value", PropType::Text),
                ("enabled", PropType::Bool),
                T,
            ],
            Self::Input => &[
                ("text", PropType::Text),
                ("value", PropType::Text),
                ("placeholder", PropType::Text),
                ("enabled", PropType::Bool),
                W,
                T,
            ],
            Self::TextBox => &[
                ("value", PropType::Text),
                ("editable", PropType::Bool),
                ("height", PropType::Int),
                W,
                T,
            ],
            Self::Slider => &[
                ("text", PropType::Text),
                ("value", PropType::Number),
                ("min", PropType::Number),
                ("max", PropType::Number),
                ("step", PropType::Number),
                ("enabled", PropType::Bool),
                W,
                T,
            ],
            Self::Hotkey => &[
                ("command", PropType::Text),
                ("text", PropType::Text),
                ("key", PropType::Chord),
                T,
            ],
            Self::Canvas => &[
                ("width", PropType::Int),
                ("height", PropType::Int),
                ("background", PropType::Color),
                ("draw", PropType::Drawing),
            ],
            Self::Chart => &[
                ("width", PropType::Int),
                ("height", PropType::Int),
                ("series", PropType::Series),
                ("min", PropType::Number),
                ("max", PropType::Number),
                ("style", PropType::Word(crate::draw::CHART_STYLES)),
            ],
        }
    }

    /// What the node is, in a sentence, for the generated reference.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Window => "A window: the root of a declared tree, with a frame, a title and a close box.",
            Self::Hud => "A small always-on-screen window with no frame, title or close box, placed by an anchor.",
            Self::Row => "Its children side by side.",
            Self::Column => "Its children one above the other.",
            Self::Group => "Its children under a title.",
            Self::Separator => "A dividing line.",
            Self::Label => "A line of text.",
            Self::Button => "A button; a click is a `ui.clicked` event and a player gesture.",
            Self::Toggle => "A box the player ticks; a change is a `ui.changed` event and a player gesture.",
            Self::Meter => "A bar filled to `value` of `max`.",
            Self::List => "A table of rows under column headings; a selection is a `ui.selected` event.",
            Self::Image => "A picture: a PNG from the plugin's package or one it fetched, by name.",
            Self::Tabs => "Pages under tabs: each child a group, its title on its tab; a tab chosen is a `ui.selected` event (`rows = { page }`) and shows that page.",
            Self::Number => "A number with a step down and a step up, kept between `min` and `max`; a step is a `ui.changed` event with the new value.",
            Self::Choice => "One of `options`, picked by the player from the list of them; a pick is a `ui.changed` event with the new value.",
            Self::Canvas => "A free drawing `width` by `height` pixels: its `draw` commands (`line`, `rect`, `circle`, `text`) in its own pixels from its top left, over its `background`.",
            Self::Chart => "Series of numbers drawn as lines (or bars, `style = \"bar\"`) `width` by `height` pixels, between `min` and `max` (the values' own when left out), each series' label along the top.",
            Self::Input => "One line of text the player types; each change is a `ui.changed` event with the text, and a player gesture.",
            Self::TextBox => "Lines of text, wrapped and scrolled; with `editable`, the player types in it and each change is a `ui.changed` event.",
            Self::Slider => "A number the player slides between `min` and `max` in `step`s; a move is a `ui.changed` event with the new value.",
            Self::Hotkey => "A key the host binds to one of the plugin's commands (`command`, a line as typed after the `/`): pressing it runs the command as if typed, a player gesture. `key` is the key asked for, with Ctrl or Alt; the host owns the actual key.",
        }
    }

    /// Every property the kind takes: its own, then its share and bounds (every node but a window
    /// and a HUD), then how it sets its nodes (a window, a HUD, a row, a column and a group).
    fn all_props(self) -> impl Iterator<Item = &'static (&'static str, PropType)> {
        let flex: &[(&str, PropType)] = if matches!(self, Self::Window | Self::Hud) {
            &[]
        } else {
            FLEX
        };
        let line: &[(&str, PropType)] = if matches!(
            self,
            Self::Window | Self::Hud | Self::Row | Self::Column | Self::Group
        ) {
            LINE
        } else {
            &[]
        };
        self.props().iter().chain(flex).chain(line)
    }

    /// The kind's properties in declaration order, each with the type of value it takes, in words.
    #[must_use]
    pub fn properties(self) -> Vec<(&'static str, String)> {
        self.all_props()
            .map(|(name, ty)| (*name, ty.describe()))
            .collect()
    }

    fn prop_type(self, name: &str) -> Option<PropType> {
        self.all_props().find(|(n, _)| *n == name).map(|(_, t)| *t)
    }
}

/// One node of a declared tree.
#[derive(Debug, Clone, PartialEq)]
pub struct UiNode {
    pub kind: NodeKind,
    pub id: Option<String>,
    pub props: BTreeMap<String, Value>,
    pub children: Vec<UiNode>,
}

/// Why a declared tree or a property change was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiError {
    NotANode,
    UnknownKind(String),
    UnknownProp {
        kind: &'static str,
        prop: String,
    },
    WrongType {
        kind: &'static str,
        prop: String,
        expected: &'static str,
    },
    /// Children on a node that holds none.
    NotAContainer(&'static str),
    DuplicateId(String),
    /// The root of a declared window is not a window.
    NotAWindow,
    /// A window within a window.
    NestedWindow,
    NoSuchNode(String),
    /// The tree is deeper or larger than a host renders.
    TooLarge,
    /// A `tabs` node holds something other than `group` pages.
    TabsHoldGroups,
    /// A HUD holds a kind of node it may not ([`NodeKind::in_hud`]).
    NotInHud(&'static str),
    /// One text (a property, an entry of a list, a list's cell, a drawing's words) is over
    /// [`MAX_TEXT`] characters.
    TooLong {
        kind: &'static str,
        prop: String,
    },
    /// A canvas with more than [`crate::draw::MAX_COMMANDS`] draw commands.
    TooManyCommands,
    /// The whole tree's text is over [`MAX_TREE_TEXT`] bytes.
    TooMuchText,
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotANode => f.write_str("a UI node must be a table with a kind"),
            Self::UnknownKind(k) => write!(f, "unknown UI node kind {k:?}"),
            Self::UnknownProp { kind, prop } => write!(f, "a {kind} has no property {prop:?}"),
            Self::WrongType {
                kind,
                prop,
                expected,
            } => write!(f, "{kind}.{prop} must be {expected}"),
            Self::NotAContainer(k) => write!(f, "a {k} holds no other nodes"),
            Self::DuplicateId(id) => write!(f, "the id {id:?} is used twice in one window"),
            Self::NotAWindow => f.write_str("the root of a window must be a window or a hud node"),
            Self::NestedWindow => f.write_str("a window cannot hold another window"),
            Self::NoSuchNode(id) => write!(f, "no node with id {id:?}"),
            Self::TooLarge => f.write_str("the tree is too deep or has too many nodes"),
            Self::TabsHoldGroups => f.write_str("a tabs node holds only groups, one a page"),
            Self::NotInHud(k) => write!(
                f,
                "a hud holds no {k}: only its buttons, toggles and lists take the pointer"
            ),
            Self::TooLong { kind, prop } => write!(
                f,
                "a {kind}'s {prop} holds a text over {MAX_TEXT} characters"
            ),
            Self::TooManyCommands => write!(
                f,
                "a canvas holds at most {} draw commands",
                crate::draw::MAX_COMMANDS
            ),
            Self::TooMuchText => write!(
                f,
                "a window holds at most {MAX_TREE_TEXT} bytes of text, all its nodes' together"
            ),
        }
    }
}

impl std::error::Error for UiError {}

/// The deepest tree and the most nodes one window may have.
pub const MAX_DEPTH: usize = 16;
pub const MAX_NODES: usize = 2_000;
/// The most rows a list may hold.
pub const MAX_LIST_ROWS: usize = 10_000;
/// The most characters one text may hold: a property, an entry of a list, a list's cell, a
/// drawing's words.
pub const MAX_TEXT: usize = 4_096;
/// The most bytes of text one window may hold, all its nodes' together.
pub const MAX_TREE_TEXT: usize = 64 * 1024;

/// Whether some text in `v` (a string, or one inside a list or a map) is over [`MAX_TEXT`]
/// characters.
fn any_too_long(v: &Value) -> bool {
    match v {
        Value::Str(s) => s.len() > MAX_TEXT && s.chars().count() > MAX_TEXT,
        Value::List(l) => l.iter().any(any_too_long),
        Value::Map(m) => m.values().any(any_too_long),
        _ => false,
    }
}

/// The bytes of text in `v`, its strings' and those inside it.
fn text_bytes(v: &Value) -> usize {
    match v {
        Value::Str(s) => s.len(),
        Value::List(l) => l.iter().map(text_bytes).sum(),
        Value::Map(m) => m.values().map(text_bytes).sum(),
        _ => 0,
    }
}

/// A key chord as a hotkey names it, `Ctrl+Shift+F5`: whether it holds Ctrl, Shift and Alt, and
/// the key (`F1` to `F24`, `A` to `Z`, `0` to `9`), upper case. `None` for anything else, and
/// for a chord with neither Ctrl nor Alt (a plain key is the game's).
#[must_use]
pub fn chord(s: &str) -> Option<(bool, bool, bool, String)> {
    let (mut ctrl, mut shift, mut alt) = (false, false, false);
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let (key, mods) = parts.split_last()?;
    for m in mods {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "shift" => shift = true,
            "alt" => alt = true,
            _ => return None,
        }
    }
    let key = key.to_ascii_uppercase();
    let ok = match key.as_bytes() {
        [c] => c.is_ascii_alphanumeric(),
        [b'F', rest @ ..] => std::str::from_utf8(rest)
            .ok()
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n)),
        _ => false,
    };
    (ok && (ctrl || alt)).then_some((ctrl, shift, alt, key))
}

/// Whether a list cell reads: a string, a number, or `{ text, color, icon }`.
fn cell_reads(c: &Value) -> bool {
    if c.as_str().is_some() || c.as_f64().is_some() {
        return true;
    }
    c.as_map().is_some_and(|m| {
        m.keys()
            .all(|k| ["text", "color", "icon"].contains(&k.as_str()))
            && (c.get("text").is_nil()
                || c.get("text").as_str().is_some()
                || c.get("text").as_f64().is_some())
            && (c.get("color").is_nil() || crate::draw::color(c.get("color")).is_ok())
            && (c.get("icon").is_nil() || c.get("icon").as_i64().is_some())
    })
}

/// Whether a list's column reads: a heading, or `{ title, width, grow, min_width }` with a string
/// title, a width and a least width in cells and a share from 0.
fn column_reads(c: &Value) -> bool {
    if c.as_str().is_some() {
        return true;
    }
    let cells = |k: &str| {
        let v = c.get(k);
        v.is_nil() || v.as_i64().is_some_and(|i| (1..=400).contains(&i))
    };
    c.as_map().is_some_and(|m| {
        m.keys()
            .all(|k| ["title", "width", "grow", "min_width"].contains(&k.as_str()))
            && (c.get("title").is_nil() || c.get("title").as_str().is_some())
            && cells("width")
            && cells("min_width")
            && (c.get("grow").is_nil()
                || c.get("grow")
                    .as_f64()
                    .is_some_and(|g| (0.0..=1000.0).contains(&g)))
    })
}

/// A list cell's text, colour and icon: a plain cell is its text in the list's own colour.
#[must_use]
pub fn cell_parts(c: &Value) -> (String, Option<u32>, Option<u32>) {
    let text = |v: &Value| match v {
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Num(n) => n.to_string(),
        _ => String::new(),
    };
    if c.as_map().is_some() {
        (
            text(c.get("text")),
            crate::draw::color(c.get("color"))
                .ok()
                .filter(|_| !c.get("color").is_nil()),
            c.get("icon").as_i64().and_then(|i| u32::try_from(i).ok()),
        )
    } else {
        (text(c), None, None)
    }
}

fn check_prop(kind: NodeKind, name: &str, v: &Value) -> Result<(), UiError> {
    let ty = kind.prop_type(name).ok_or_else(|| UiError::UnknownProp {
        kind: kind.name(),
        prop: name.to_owned(),
    })?;
    let wrong = |expected| UiError::WrongType {
        kind: kind.name(),
        prop: name.to_owned(),
        expected,
    };
    let ok = match ty {
        PropType::Text => v.as_str().is_some(),
        PropType::Bool => v.as_bool().is_some(),
        PropType::Int => v.as_i64().is_some_and(|i| (0..=10_000).contains(&i)),
        PropType::Number => v.as_f64().is_some_and(f64::is_finite),
        PropType::Word(words) => v.as_str().is_some_and(|s| words.contains(&s)),
        PropType::Strings => v
            .as_list()
            .is_some_and(|l| l.iter().all(|e| e.as_str().is_some())),
        PropType::Rows => v.as_list().is_some_and(|rows| {
            rows.len() <= MAX_LIST_ROWS
                && rows.iter().all(|r| {
                    r.as_list()
                        .is_some_and(|cells| cells.iter().all(cell_reads))
                })
        }),
        PropType::Index => v.is_nil() || v.as_i64().is_some_and(|i| i >= 1),
        PropType::Drawing => crate::draw::commands(v).is_ok(),
        PropType::Series => crate::draw::chart_commands(v, None, None, false, 10.0, 10.0).is_ok(),
        PropType::Color => crate::draw::color(v).is_ok(),
        PropType::Chord => v.as_str().is_some_and(|s| chord(s).is_some()),
        PropType::Grow => v.as_f64().is_some_and(|g| (0.0..=1000.0).contains(&g)),
        PropType::Columns => v.as_list().is_some_and(|l| l.iter().all(column_reads)),
    };
    if ty == PropType::Drawing
        && v.as_list()
            .is_some_and(|l| l.len() > crate::draw::MAX_COMMANDS)
    {
        return Err(UiError::TooManyCommands);
    }
    if !ok {
        return Err(wrong(ty.expected()));
    }
    if any_too_long(v) {
        return Err(UiError::TooLong {
            kind: kind.name(),
            prop: name.to_owned(),
        });
    }
    Ok(())
}

impl UiNode {
    /// Check and build a window's tree (its root a `window` or a `hud`).
    ///
    /// # Errors
    /// The first rule the tree breaks.
    pub fn window_from_value(v: &Value) -> Result<Self, UiError> {
        let mut count = 0;
        let node = Self::from_value(v, 0, &mut count)?;
        if !matches!(node.kind, NodeKind::Window | NodeKind::Hud) {
            return Err(UiError::NotAWindow);
        }
        if node.kind == NodeKind::Hud {
            node.check_in_hud()?;
        }
        if node.text_bytes() > MAX_TREE_TEXT {
            return Err(UiError::TooMuchText);
        }
        let mut ids = BTreeSet::new();
        node.check_ids(&mut ids)?;
        Ok(node)
    }

    /// The bytes of text the tree holds, all its nodes' properties together.
    #[must_use]
    pub fn text_bytes(&self) -> usize {
        self.props.values().map(text_bytes).sum::<usize>()
            + self.children.iter().map(Self::text_bytes).sum::<usize>()
    }

    fn check_in_hud(&self) -> Result<(), UiError> {
        if !self.kind.in_hud() {
            return Err(UiError::NotInHud(self.kind.name()));
        }
        self.children.iter().try_for_each(Self::check_in_hud)
    }

    fn from_value(v: &Value, depth: usize, count: &mut usize) -> Result<Self, UiError> {
        *count += 1;
        if depth > MAX_DEPTH || *count > MAX_NODES {
            return Err(UiError::TooLarge);
        }
        let map = v.as_map().ok_or(UiError::NotANode)?;
        let kind_name = map
            .get("kind")
            .and_then(Value::as_str)
            .ok_or(UiError::NotANode)?;
        let kind =
            NodeKind::parse(kind_name).ok_or_else(|| UiError::UnknownKind(kind_name.to_owned()))?;
        let mut node = Self {
            kind,
            id: None,
            props: BTreeMap::new(),
            children: Vec::new(),
        };
        for (k, val) in map {
            match k.as_str() {
                "kind" => {}
                "id" => {
                    node.id = Some(val.as_str().map(str::to_owned).ok_or(UiError::WrongType {
                        kind: kind.name(),
                        prop: "id".into(),
                        expected: "a string",
                    })?);
                }
                "children" => {
                    let list = val.as_list().ok_or(UiError::WrongType {
                        kind: kind.name(),
                        prop: "children".into(),
                        expected: "a list of nodes",
                    })?;
                    if !kind.is_container() && !list.is_empty() {
                        return Err(UiError::NotAContainer(kind.name()));
                    }
                    for c in list {
                        let child = Self::from_value(c, depth + 1, count)?;
                        if matches!(child.kind, NodeKind::Window | NodeKind::Hud) {
                            return Err(UiError::NestedWindow);
                        }
                        if kind == NodeKind::Tabs && child.kind != NodeKind::Group {
                            return Err(UiError::TabsHoldGroups);
                        }
                        node.children.push(child);
                    }
                }
                prop => {
                    check_prop(kind, prop, val)?;
                    node.props.insert(prop.to_owned(), val.clone());
                }
            }
        }
        Ok(node)
    }

    fn check_ids<'a>(&'a self, seen: &mut BTreeSet<&'a str>) -> Result<(), UiError> {
        if let Some(id) = &self.id {
            if !seen.insert(id) {
                return Err(UiError::DuplicateId(id.clone()));
            }
        }
        self.children.iter().try_for_each(|c| c.check_ids(seen))
    }

    /// A canvas's or a chart's size in pixels: its `width` and `height`, 160 by 80 when left out.
    #[must_use]
    pub fn pixel_size(&self) -> (i64, i64) {
        let get = |k: &str, d: i64| self.props.get(k).and_then(Value::as_i64).unwrap_or(d);
        (get("width", 160), get("height", 80))
    }

    /// What a canvas or a chart draws, as canvas commands (none for any other node, or for
    /// properties that do not read).
    #[must_use]
    pub fn drawing(&self) -> Vec<crate::draw::Command> {
        let nil = Value::Nil;
        let prop = |k: &str| self.props.get(k).unwrap_or(&nil);
        match self.kind {
            NodeKind::Canvas => crate::draw::commands(prop("draw")).unwrap_or_default(),
            NodeKind::Chart => {
                let (w, h) = self.pixel_size();
                let f = |n: i64| f64::from(i32::try_from(n).unwrap_or(0));
                crate::draw::chart_commands(
                    prop("series"),
                    prop("min").as_f64(),
                    prop("max").as_f64(),
                    prop("style").as_str() == Some("bar"),
                    f(w),
                    f(h),
                )
                .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    /// The node with `id`, anywhere in this tree.
    #[must_use]
    pub fn find(&self, id: &str) -> Option<&UiNode> {
        if self.id.as_deref() == Some(id) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    /// The node with `id`, mutably.
    pub fn find_mut(&mut self, id: &str) -> Option<&mut UiNode> {
        if self.id.as_deref() == Some(id) {
            return Some(self);
        }
        self.children.iter_mut().find_map(|c| c.find_mut(id))
    }

    /// Set one property of the node with `id`, under the same rules as a declaration. Setting a
    /// property to nil removes it.
    ///
    /// # Errors
    /// No such node, or a property the node's kind does not have or a value of the wrong type.
    pub fn set_prop(&mut self, id: &str, prop: &str, v: Value) -> Result<(), UiError> {
        let node = self
            .find_mut(id)
            .ok_or_else(|| UiError::NoSuchNode(id.to_owned()))?;
        if v.is_nil()
            && node
                .kind
                .prop_type(prop)
                .is_some_and(|t| t != PropType::Index)
        {
            node.props.remove(prop);
            return Ok(());
        }
        check_prop(node.kind, prop, &v)?;
        let was = node.props.insert(prop.to_owned(), v);
        // The whole tree's text stays within its bound: over it, the change is undone.
        if self.text_bytes() > MAX_TREE_TEXT {
            if let Some(node) = self.find_mut(id) {
                match was {
                    Some(old) => node.props.insert(prop.to_owned(), old),
                    None => node.props.remove(prop),
                };
            }
            return Err(UiError::TooMuchText);
        }
        Ok(())
    }

    /// A property as text, or `""`.
    #[must_use]
    pub fn text(&self, prop: &str) -> &str {
        self.props.get(prop).and_then(Value::as_str).unwrap_or("")
    }

    /// The tree as indented text, one node a line: kind, id and properties in name order. This is
    /// what a host with no display shows for a window, and what a test compares.
    #[must_use]
    pub fn dump(&self) -> String {
        let mut out = String::new();
        self.dump_into(0, &mut out);
        out
    }

    fn dump_into(&self, depth: usize, out: &mut String) {
        let _ = write!(out, "{}{}", "  ".repeat(depth), self.kind.name());
        if let Some(id) = &self.id {
            let _ = write!(out, " #{id}");
        }
        for (k, v) in &self.props {
            let _ = write!(out, " {k}={}", v.to_json());
        }
        out.push('\n');
        for c in &self.children {
            c.dump_into(depth + 1, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NodeKind, UiError, UiNode};
    use crate::value::Value;

    fn node(kind: &str) -> Value {
        Value::map().with("kind", kind)
    }

    fn window(children: Vec<Value>) -> Value {
        node("window")
            .with("id", "main")
            .with("title", "XP")
            .with("children", children)
    }

    #[test]
    fn a_declared_window_builds_and_dumps_in_document_order() {
        let w = window(vec![
            node("label").with("id", "rate").with("text", "0/h"),
            node("meter")
                .with("id", "bar")
                .with("value", 3_i64)
                .with("max", 10_i64)
                .with("style", "health"),
            node("row").with(
                "children",
                vec![
                    node("button").with("id", "reset").with("text", "Reset"),
                    node("toggle").with("id", "t").with("value", true),
                ],
            ),
        ]);
        let tree = UiNode::window_from_value(&w).expect("valid");
        assert_eq!(
            tree.dump(),
            "window #main title=\"XP\"\n  label #rate text=\"0/h\"\n  meter #bar max=10 style=\"health\" value=3\n  row\n    button #reset text=\"Reset\"\n    toggle #t value=true\n"
        );
        assert_eq!(tree.find("reset").map(|n| n.kind), Some(NodeKind::Button));
    }

    #[test]
    fn unknown_kinds_props_and_mistyped_values_are_refused() {
        assert_eq!(
            UiNode::window_from_value(&window(vec![node("spinner")])),
            Err(UiError::UnknownKind("spinner".into()))
        );
        assert!(matches!(
            UiNode::window_from_value(&window(vec![node("label").with("txt", "x")])),
            Err(UiError::UnknownProp { .. })
        ));
        assert!(matches!(
            UiNode::window_from_value(&window(vec![node("meter").with("style", "gold")])),
            Err(UiError::WrongType { .. })
        ));
        assert!(matches!(
            UiNode::window_from_value(&window(vec![
                node("list").with("rows", vec![Value::from("not a row")])
            ])),
            Err(UiError::WrongType { .. })
        ));
    }

    #[test]
    fn ids_are_unique_within_a_window_and_windows_do_not_nest() {
        let dup = window(vec![
            node("label").with("id", "a"),
            node("button").with("id", "a"),
        ]);
        assert_eq!(
            UiNode::window_from_value(&dup),
            Err(UiError::DuplicateId("a".into()))
        );
        assert_eq!(
            UiNode::window_from_value(&window(vec![node("window")])),
            Err(UiError::NestedWindow)
        );
        assert_eq!(
            UiNode::window_from_value(&node("label")),
            Err(UiError::NotAWindow)
        );
        assert_eq!(
            UiNode::window_from_value(&window(vec![
                node("label").with("children", vec![node("label")])
            ])),
            Err(UiError::NotAContainer("label"))
        );
    }

    #[test]
    fn a_property_change_is_checked_like_a_declaration() {
        let mut tree = UiNode::window_from_value(&window(vec![node("label").with("id", "rate")]))
            .expect("valid");
        tree.set_prop("rate", "text", Value::from("1.2M/h"))
            .expect("set");
        assert_eq!(tree.find("rate").map(|n| n.text("text")), Some("1.2M/h"));
        assert!(tree.set_prop("rate", "text", Value::Int(3)).is_err());
        assert!(tree.set_prop("rate", "value", Value::Int(3)).is_err());
        assert_eq!(
            tree.set_prop("nope", "text", Value::from("x")),
            Err(UiError::NoSuchNode("nope".into()))
        );
        tree.set_prop("rate", "text", Value::Nil).expect("remove");
        assert_eq!(tree.find("rate").map(|n| n.text("text")), Some(""));
    }

    #[test]
    fn each_kind_lists_its_properties_with_the_type_a_refusal_names() {
        let meter = NodeKind::Meter.properties();
        assert_eq!(
            meter.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            [
                "value",
                "max",
                "style",
                "text",
                "width",
                "tooltip",
                "grow",
                "min_width",
                "max_width",
                "min_height",
                "max_height"
            ]
        );
        assert_eq!(meter[2].1, "one of `neutral`, `health`, `stamina`, `mana`");
        assert_eq!(
            NodeKind::Separator
                .properties()
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>(),
            ["grow", "min_width", "max_width", "min_height", "max_height"],
            "a separator has only its share and its bounds"
        );
        let Err(UiError::WrongType { expected, .. }) =
            UiNode::window_from_value(&window(vec![node("label").with("width", "wide")]))
        else {
            panic!("a mistyped width is refused");
        };
        assert_eq!(
            NodeKind::Label.properties()[2],
            ("width", expected.to_owned())
        );
    }

    /// A HUD holds buttons, toggles and lists among what it shows, and nothing typed in, dragged,
    /// stepped or opened over the screen, however deep it is put.
    #[test]
    fn a_hud_holds_no_control_but_buttons_toggles_and_lists() {
        let hud = |children: Vec<Value>| {
            UiNode::window_from_value(&node("hud").with("id", "h").with("children", children))
        };
        let kept = hud(vec![
            node("button").with("text", "Go"),
            node("toggle"),
            node("list"),
            node("label").with("text", "12/h"),
            node("meter"),
            node("image"),
            node("canvas"),
            node("chart"),
            node("separator"),
            node("hotkey"),
        ]);
        assert!(kept.is_ok(), "{kept:?}");
        for kind in ["input", "textbox", "number", "choice", "slider"] {
            assert_eq!(
                hud(vec![node("row").with("children", vec![node(kind)])]),
                Err(UiError::NotInHud(
                    NodeKind::parse(kind).expect("a kind").name()
                )),
                "{kind}"
            );
        }
        assert_eq!(
            hud(vec![node("tabs").with("children", vec![node("group")])]),
            Err(UiError::NotInHud("tabs"))
        );
        assert!(
            UiNode::window_from_value(&window(vec![node("input"), node("slider")])).is_ok(),
            "a window holds them"
        );
    }

    /// One text is at most 4,096 characters, a window's text at most 64 KiB all told and a
    /// canvas at most 1,000 draw commands, at a declaration and at a change alike; a change that
    /// would break one is undone.
    #[test]
    fn a_window_holds_bounded_text_and_a_canvas_bounded_commands() {
        let long = "W".repeat(super::MAX_TEXT + 1);
        assert_eq!(
            UiNode::window_from_value(&window(vec![node("label").with("text", long.as_str())])),
            Err(UiError::TooLong {
                kind: "label",
                prop: "text".into()
            })
        );
        assert!(
            UiNode::window_from_value(&window(vec![
                node("label").with("text", "W".repeat(super::MAX_TEXT))
            ]))
            .is_ok(),
            "4,096 characters is a text"
        );
        let cell = Value::List(vec![Value::from(long.as_str())]);
        assert!(matches!(
            UiNode::window_from_value(&window(vec![node("list").with("rows", vec![cell])])),
            Err(UiError::TooLong { .. })
        ));
        // Twenty labels of 4,000 characters: 80,000 bytes.
        let many: Vec<Value> = (0..20)
            .map(|_| node("label").with("text", "W".repeat(4_000)))
            .collect();
        assert_eq!(
            UiNode::window_from_value(&window(many)),
            Err(UiError::TooMuchText)
        );
        let circle = |i: i64| {
            Value::map()
                .with("op", "circle")
                .with("x", i)
                .with("y", 0_i64)
                .with("r", 2_i64)
        };
        assert_eq!(
            UiNode::window_from_value(&window(vec![
                node("canvas").with("draw", (0..1_001).map(circle).collect::<Vec<_>>())
            ])),
            Err(UiError::TooManyCommands)
        );
        assert!(UiNode::window_from_value(&window(vec![
            node("canvas").with("draw", (0..1_000).map(circle).collect::<Vec<_>>())
        ]))
        .is_ok());
        // A change: one text too long, and the window's text past its bound, both refused and
        // undone.
        let labels: Vec<Value> = (0..15)
            .map(|i| {
                node("label")
                    .with("id", format!("l{i}"))
                    .with("text", "W".repeat(4_000))
            })
            .collect();
        let mut tree = UiNode::window_from_value(&window(labels)).expect("60,000 bytes");
        assert!(matches!(
            tree.set_prop("l0", "text", Value::from(long.as_str())),
            Err(UiError::TooLong { .. })
        ));
        assert_eq!(
            tree.set_prop("rate", "text", Value::from("x")),
            Err(UiError::NoSuchNode("rate".into()))
        );
        tree.set_prop("l0", "tooltip", Value::from("W".repeat(4_000)))
            .expect("64,000 bytes");
        assert_eq!(
            tree.set_prop("l1", "tooltip", Value::from("W".repeat(4_000))),
            Err(UiError::TooMuchText),
            "68,000 bytes"
        );
        assert_eq!(
            tree.find("l1").map(|n| n.text("tooltip")),
            Some(""),
            "undone"
        );
        assert_eq!(
            tree.text_bytes(),
            64_000 + "XP".len(),
            "the labels and the title"
        );
    }

    #[test]
    fn a_huge_or_deep_tree_is_refused() {
        let mut v = node("column");
        for _ in 0..20 {
            v = node("column").with("children", vec![v]);
        }
        assert_eq!(
            UiNode::window_from_value(&window(vec![v])),
            Err(UiError::TooLarge)
        );
    }
}
