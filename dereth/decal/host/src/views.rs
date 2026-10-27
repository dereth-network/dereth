//! Declared windows as Decal views.
//!
//! Each plugin window is laid out by the rules every host follows (`dereth_plugin_api::layout`),
//! in the measures of Decal's controls, and written as a Decal view: one fixed layout of Decal's
//! own controls at the places the layout gives them, a tab's pages as a notebook's. Every control
//! is named after its node's place in the tree (`c`, then the child indices), so a click on it is
//! traced back to its node.
//!
//! | node | Decal control |
//! |---|---|
//! | label, a group's title | static text |
//! | button, hotkey | push button (a hotkey's runs its command; Decal binds no key) |
//! | toggle | check box |
//! | meter | progress bar |
//! | list | list of text columns, its headings as static text above it |
//! | tabs | notebook |
//! | choice | drop-down choice |
//! | input, editable text box | edit field |
//! | text box | list of its lines, wrapped |
//! | slider | slider |
//! | number | static text between a `-` and a `+` button |
//! | image, canvas, chart | static text saying what is not drawn |
//!
//! A view is built again only when its controls or their places change; otherwise only the
//! controls whose state changed are sent ([`crate::Op::Update`]). Text widths are measured in steps
//! of a few characters, and a view never shrinks, so a label whose number grows does not move
//! the controls after it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use dereth_client_sdk::plugin_api::layout::{self, Measure, Metrics};
use dereth_client_sdk::plugin_api::ui::cell_parts;
use dereth_client_sdk::plugin_api::{NodeKind, UiNode, Value};
use dereth_client_sdk::plugins::{PluginHost, UiInput, HOST_ID, TOASTS};

/// The room round a view's controls, in pixels.
const PAD: f32 = 4.0;
/// The largest view, in pixels.
const MAX_SIZE: (f32, f32) = (640.0, 560.0);
/// The characters a width is measured in steps of.
const STEP: usize = 4;
/// The characters of a text box's line.
const WRAP: usize = 60;
/// The icon a plugin's view shows in Decal's bar.
pub const ICON: u32 = 8712;

/// The measures of Decal's controls: a character about 6 pixels wide, a line 16 high, a check box
/// 14 a side, a scrollbar 16 wide.
#[must_use]
pub fn metrics() -> Metrics {
    Metrics::of(6.0, 16.0, 14.0, 16.0)
}

/// Text measured in steps of [`STEP`] characters.
#[derive(Debug, Clone, Copy)]
struct Stepped(f32);

impl Measure for Stepped {
    fn text(&self, text: &str) -> f32 {
        let n = text.chars().count().div_ceil(STEP) * STEP;
        #[allow(clippy::cast_precision_loss)] // a few hundred characters at most
        let n = n as f32;
        n * self.0
    }
}

/// How a control's events reach its node.
#[derive(Debug, Clone, PartialEq)]
enum Binding {
    Click,
    Hotkey,
    Toggle,
    Select,
    /// A choice: its options, by index.
    Choice(Vec<String>),
    Text,
    Slider,
    /// A number's step down or up: the value, the step, and the bounds.
    Step {
        value: f64,
        by: f64,
        min: f64,
        max: f64,
    },
    Tabs,
}

/// One control of a view: its name, the node it stands for and how its events reach it.
#[derive(Debug, Clone, PartialEq)]
struct Bound {
    node: String,
    binding: Binding,
}

/// A view built from a window's tree.
#[derive(Debug, Clone, PartialEq)]
struct Built {
    xml: String,
    /// Everything about the controls but their state: when it changes, the view is built again.
    skeleton: String,
    size: (f32, f32),
    states: BTreeMap<String, Value>,
    bound: BTreeMap<String, Bound>,
}

/// A window's view as it was last sent.
#[derive(Debug, Clone)]
struct Shown {
    plugin: String,
    window: String,
    revision: u64,
    open: bool,
    built: Built,
}

/// A view built again, as [`crate::Op::View`] carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewOp {
    pub key: String,
    pub open: bool,
    pub xml: String,
    /// Every control's state: `{ name, … }`.
    pub controls: Vec<Value>,
}

impl ViewOp {
    #[must_use]
    pub fn to_value(&self) -> Value {
        Value::map()
            .with("op", "view")
            .with("key", self.key.as_str())
            .with("open", self.open)
            .with("xml", self.xml.as_str())
            .with("controls", self.controls.clone())
    }
}

/// What an event on a view asks of the plugin host.
#[derive(Debug, Clone, PartialEq)]
pub enum Asked {
    /// An input on a plugin's window.
    Input {
        plugin: String,
        window: String,
        input: UiInput,
    },
    /// A hotkey node pressed.
    Hotkey { plugin: String, node: String },
    /// The player opened a window's view from Decal's bar.
    Open { plugin: String, window: String },
    /// The player closed a window's view.
    Close { plugin: String, window: String },
}

/// Every plugin window's view, as last sent.
#[derive(Debug, Default)]
pub struct Views {
    shown: BTreeMap<String, Shown>,
}

/// A window's key: its plugin and its id.
#[must_use]
pub fn key(plugin: &str, window: &str) -> String {
    format!("{plugin}/{window}")
}

impl Views {
    /// The ops that bring Decal's views up to the windows as they stand.
    pub fn sync(&mut self, host: &PluginHost, ops: &mut Vec<crate::Op>) {
        // A plugin's window on the plugin bar keeps its view while closed, so that Decal's bar
        // shows its button; any other window has a view only while it is open.
        let bar: BTreeSet<String> = host
            .bar()
            .into_iter()
            .filter_map(|b| b.window.map(|w| key(&b.id, &w)))
            .collect();
        let mut present = BTreeSet::new();
        for (plugin, window, win) in host.windows() {
            if plugin == HOST_ID && window == TOASTS {
                continue;
            }
            let k = key(&plugin, &window);
            if !win.open && !bar.contains(&k) {
                continue;
            }
            present.insert(k.clone());
            if let Some(s) = self.shown.get(&k) {
                if s.revision == win.revision && s.open == win.open {
                    continue;
                }
            }
            let floor = self.shown.get(&k).map_or((0.0, 0.0), |s| s.built.size);
            let built = build(&win.tree, floor, &plugin);
            match self.shown.get_mut(&k) {
                Some(s) if s.built.skeleton == built.skeleton => {
                    let changed: Vec<Value> = built
                        .states
                        .iter()
                        .filter(|(n, v)| s.built.states.get(*n) != Some(*v))
                        .map(|(_, v)| v.clone())
                        .collect();
                    if !changed.is_empty() {
                        ops.push(crate::Op::Update {
                            key: k.clone(),
                            controls: changed,
                        });
                    }
                    if s.open != win.open {
                        ops.push(crate::Op::Show {
                            key: k.clone(),
                            open: win.open,
                        });
                    }
                    s.revision = win.revision;
                    s.open = win.open;
                    s.built = built;
                }
                _ => {
                    ops.push(crate::Op::View(ViewOp {
                        key: k.clone(),
                        open: win.open,
                        xml: built.xml.clone(),
                        controls: built.states.values().cloned().collect(),
                    }));
                    self.shown.insert(
                        k,
                        Shown {
                            plugin: plugin.clone(),
                            window: window.clone(),
                            revision: win.revision,
                            open: win.open,
                            built,
                        },
                    );
                }
            }
        }
        let gone: Vec<String> = self
            .shown
            .keys()
            .filter(|k| !present.contains(*k))
            .cloned()
            .collect();
        for k in gone {
            self.shown.remove(&k);
            ops.push(crate::Op::Dispose { key: k });
        }
    }

    /// What an event on a view's control asks: `event` is `click`, `change`, `select`, `opened` or
    /// `closed`, with its value (a box's tick, a field's text, a row or an option counted from 0, a
    /// slider's place). `None` for an event no node takes.
    #[must_use]
    pub fn event(&self, key: &str, control: &str, event: &str, value: &Value) -> Option<Asked> {
        let s = self.shown.get(key)?;
        let input = |input: UiInput| Asked::Input {
            plugin: s.plugin.clone(),
            window: s.window.clone(),
            input,
        };
        match event {
            "opened" => {
                return (!s.open).then(|| Asked::Open {
                    plugin: s.plugin.clone(),
                    window: s.window.clone(),
                });
            }
            "closed" => {
                return s.open.then(|| Asked::Close {
                    plugin: s.plugin.clone(),
                    window: s.window.clone(),
                });
            }
            _ => {}
        }
        let b = s.built.bound.get(control)?;
        let node = b.node.clone();
        let index = || value.as_i64().filter(|i| *i >= 0);
        Some(match (&b.binding, event) {
            (Binding::Click, "click") => input(UiInput::Click { node }),
            (Binding::Hotkey, "click") => Asked::Hotkey {
                plugin: s.plugin.clone(),
                node,
            },
            (
                Binding::Step {
                    value,
                    by,
                    min,
                    max,
                },
                "click",
            ) => {
                let down = control.ends_with('d');
                let v = if down { value - by } else { value + by }.clamp(*min, *max);
                input(UiInput::Change {
                    node,
                    value: number(v),
                })
            }
            (Binding::Toggle, "change") => input(UiInput::Change {
                node,
                value: Value::Bool(value.as_bool()?),
            }),
            (Binding::Text, "change") => input(UiInput::Change {
                node,
                value: Value::Str(value.as_str()?.to_owned()),
            }),
            (Binding::Slider, "change") => input(UiInput::Change {
                node,
                value: number(value.as_f64()?),
            }),
            (Binding::Choice(options), "change") => {
                let i = usize::try_from(index()?).ok()?;
                input(UiInput::Change {
                    node,
                    value: Value::Str(options.get(i)?.clone()),
                })
            }
            (Binding::Select, "select") | (Binding::Tabs, "change") => input(UiInput::Select {
                node,
                rows: vec![index()? + 1],
            }),
            _ => return None,
        })
    }
}

/// A view built from `tree`, its body no smaller than `floor`, its title the window's or else
/// `plugin`'s.
fn build(tree: &UiNode, floor: (f32, f32), plugin: &str) -> Built {
    let m = metrics();
    let measure = Stepped(m.cell);
    let want = layout::want(tree, &m, &measure);
    let natural = layout::natural_body(&want);
    let body = (
        natural.0.max(floor.0).min(MAX_SIZE.0),
        natural.1.max(floor.1).min(MAX_SIZE.1),
    );
    let laid = layout::place_body(tree, &want, &m, body);
    let mut w = Writer::default();
    w.node(tree, &laid, (PAD, PAD), "c", &m);
    let title = match tree.text("title") {
        "" => plugin,
        t => t,
    };
    let (vw, vh) = (body.0 + 2.0 * PAD, body.1 + 2.0 * PAD);
    let mut xml = String::new();
    let _ = write!(
        xml,
        "<?xml version=\"1.0\"?>\n<view icon=\"{ICON}\" title=\"{}\" width=\"{vw}\" height=\"{vh}\">\n<control progid=\"DecalControls.FixedLayout\" clipped=\"\">\n{}</control>\n</view>\n",
        escape(title),
        w.xml
    );
    let skeleton = format!("{title}|{vw}x{vh}|{}", w.skeleton);
    Built {
        xml,
        skeleton,
        size: body,
        states: w.states,
        bound: w.bound,
    }
}

/// The view's controls as they are written.
#[derive(Debug, Default)]
struct Writer {
    xml: String,
    skeleton: String,
    states: BTreeMap<String, Value>,
    bound: BTreeMap<String, Bound>,
}

/// A whole number of pixels.
#[allow(clippy::cast_possible_truncation)]
fn px(v: f32) -> i32 {
    // LINT-OK: a view is a few hundred pixels across.
    v.round() as i32
}

impl Writer {
    /// One control at `(x, y)`, `w` by `h`, with its own XML inside it (columns, pages).
    #[allow(clippy::too_many_arguments)]
    fn control(
        &mut self,
        progid: &str,
        name: &str,
        at: (f32, f32),
        size: (f32, f32),
        inner: &str,
        state: Value,
    ) {
        let (x, y, w, h) = (px(at.0), px(at.1), px(size.0).max(1), px(size.1).max(1));
        let _ = write!(
            self.xml,
            "<control progid=\"DecalControls.{progid}\" name=\"{name}\" left=\"{x}\" top=\"{y}\" width=\"{w}\" height=\"{h}\""
        );
        if inner.is_empty() {
            self.xml.push_str(" />\n");
        } else {
            let _ = write!(self.xml, ">\n{inner}</control>\n");
        }
        let _ = write!(self.skeleton, "{progid} {name} {x},{y} {w}x{h} [{inner}];");
        self.states
            .insert(name.to_owned(), state.with("name", name));
    }

    /// `name`'s events go to `n` as `binding` says; its state names the node.
    fn bind(&mut self, name: &str, n: &UiNode, binding: Binding) {
        if let Some(id) = &n.id {
            if let Some(s) = self.states.remove(name) {
                self.states
                    .insert(name.to_owned(), s.with("node", id.as_str()));
            }
            self.bound.insert(
                name.to_owned(),
                Bound {
                    node: id.clone(),
                    binding,
                },
            );
        }
    }

    /// A control's label, before it, when the node has one.
    fn label(&mut self, n: &UiNode, name: &str, at: (f32, f32), size: (f32, f32), width: f32) {
        let text = n.text("text");
        if !text.is_empty() && width > 0.0 {
            let lname = format!("{name}l");
            self.control(
                "StaticText",
                &lname,
                at,
                (width, size.1),
                "",
                text_state(text),
            );
        }
    }

    fn node(
        &mut self,
        n: &UiNode,
        laid: &layout::Laid,
        origin: (f32, f32),
        name: &str,
        m: &Metrics,
    ) {
        let at = (origin.0 + laid.x, origin.1 + laid.y);
        let size = (laid.w, laid.h);
        let child = |i: usize| format!("{name}_{i}");
        match n.kind {
            NodeKind::Window | NodeKind::Hud | NodeKind::Row | NodeKind::Column => {
                for (i, (c, cl)) in n.children.iter().zip(&laid.children).enumerate() {
                    self.node(c, cl, at, &child(i), m);
                }
            }
            NodeKind::Group => {
                let title = n.text("title");
                if !title.is_empty() {
                    self.control(
                        "StaticText",
                        &format!("{name}t"),
                        (at.0 + m.pad, at.1),
                        (size.0 - 2.0 * m.pad, m.line),
                        "",
                        text_state(title),
                    );
                }
                for (i, (c, cl)) in n.children.iter().zip(&laid.children).enumerate() {
                    self.node(c, cl, at, &child(i), m);
                }
            }
            NodeKind::Tabs => {
                let mut pages = String::new();
                for (i, (page, pl)) in n.children.iter().zip(&laid.children).enumerate() {
                    let mut inner = Writer::default();
                    // A page's nodes are placed from its own top left, under the notebook's tabs.
                    for (j, (c, cl)) in page.children.iter().zip(&pl.children).enumerate() {
                        inner.node(c, cl, (0.0, 0.0), &format!("{name}_{i}_{j}"), m);
                    }
                    let _ = write!(
                        pages,
                        "<page label=\"{}\">\n<control progid=\"DecalControls.FixedLayout\" clipped=\"\">\n{}</control>\n</page>\n",
                        escape(page.text("title")),
                        inner.xml
                    );
                    let _ = write!(
                        self.skeleton,
                        "page {i} {} {{{}}};",
                        page.text("title"),
                        inner.skeleton
                    );
                    self.states.extend(inner.states);
                    self.bound.extend(inner.bound);
                }
                let page = layout::chosen_page(n);
                self.control(
                    "Notebook",
                    name,
                    at,
                    size,
                    &pages,
                    Value::map()
                        .with("kind", "tabs")
                        .with("page", page_value(page)),
                );
                self.bind(name, n, Binding::Tabs);
            }
            NodeKind::Separator => {}
            NodeKind::Label => {
                self.control("StaticText", name, at, size, "", text_state(n.text("text")));
            }
            NodeKind::Button => {
                self.control(
                    "PushButton",
                    name,
                    at,
                    size,
                    "",
                    Value::map()
                        .with("kind", "button")
                        .with("text", n.text("text")),
                );
                self.bind(name, n, Binding::Click);
            }
            NodeKind::Hotkey => {
                let text = match n.text("text") {
                    "" => n.text("command"),
                    t => t,
                };
                self.control(
                    "PushButton",
                    name,
                    at,
                    size,
                    "",
                    Value::map().with("kind", "button").with("text", text),
                );
                self.bind(name, n, Binding::Hotkey);
            }
            NodeKind::Toggle => {
                self.control(
                    "CheckBox",
                    name,
                    at,
                    size,
                    "",
                    Value::map()
                        .with("kind", "toggle")
                        .with("text", n.text("text"))
                        .with(
                            "checked",
                            n.props
                                .get("value")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                        ),
                );
                self.bind(name, n, Binding::Toggle);
            }
            NodeKind::Meter => {
                let value = n.props.get("value").and_then(Value::as_f64).unwrap_or(0.0);
                let max = n.props.get("max").and_then(Value::as_f64).unwrap_or(1.0);
                let pct = if max > 0.0 {
                    (value / max * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                };
                #[allow(clippy::cast_possible_truncation)] // 0 to 100
                let pct = pct.round() as i64;
                self.control(
                    "Progress",
                    name,
                    at,
                    size,
                    "",
                    Value::map()
                        .with("kind", "meter")
                        .with("value", pct)
                        .with("text", n.text("text")),
                );
            }
            NodeKind::List => self.list(n, laid, at, size, name, m),
            NodeKind::Choice => {
                let label = laid.parts.first().copied().unwrap_or(0.0);
                self.label(n, name, at, size, label);
                let options: Vec<String> = n
                    .props
                    .get("options")
                    .and_then(Value::as_list)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|o| o.as_str().map(str::to_owned))
                    .collect();
                let chosen = n.text("value");
                let selected = options
                    .iter()
                    .position(|o| o == chosen)
                    .map_or(-1, |i| i64::try_from(i).unwrap_or(-1));
                self.control(
                    "Choice",
                    name,
                    (at.0 + label, at.1),
                    (size.0 - label, size.1),
                    "",
                    Value::map()
                        .with("kind", "choice")
                        .with(
                            "options",
                            options
                                .iter()
                                .map(|o| Value::from(o.as_str()))
                                .collect::<Vec<_>>(),
                        )
                        .with("selected", selected),
                );
                self.bind(name, n, Binding::Choice(options));
            }
            NodeKind::Input => {
                let label = laid.parts.first().copied().unwrap_or(0.0);
                self.label(n, name, at, size, label);
                self.control(
                    "Edit",
                    name,
                    (at.0 + label, at.1),
                    (size.0 - label, size.1),
                    "",
                    Value::map()
                        .with("kind", "input")
                        .with("text", n.text("value")),
                );
                self.bind(name, n, Binding::Text);
            }
            NodeKind::TextBox => {
                if n.props
                    .get("editable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    self.control(
                        "Edit",
                        name,
                        at,
                        (size.0, size.1.min(m.control)),
                        "",
                        Value::map()
                            .with("kind", "input")
                            .with("text", n.text("value")),
                    );
                    self.bind(name, n, Binding::Text);
                } else {
                    let lines: Vec<Value> = wrap(n.text("value"), WRAP)
                        .into_iter()
                        .map(|l| Value::List(vec![Value::from(l)]))
                        .collect();
                    let col = format!(
                        "<column progid=\"DecalControls.TextColumn\" fixedwidth=\"{}\" />\n",
                        px(size.0 - 2.0 * m.rim)
                    );
                    self.control(
                        "List",
                        name,
                        at,
                        size,
                        &col,
                        Value::map().with("kind", "list").with("rows", lines),
                    );
                }
            }
            NodeKind::Slider => {
                let label = laid.parts.first().copied().unwrap_or(0.0);
                self.label(n, name, at, size, label);
                let num = |k: &str, d: f64| n.props.get(k).and_then(Value::as_f64).unwrap_or(d);
                self.control(
                    "Slider",
                    name,
                    (at.0 + label, at.1),
                    (size.0 - label, size.1),
                    "",
                    Value::map()
                        .with("kind", "slider")
                        .with("min", num("min", 0.0))
                        .with("max", num("max", 100.0))
                        .with("value", num("value", 0.0)),
                );
                self.bind(name, n, Binding::Slider);
            }
            NodeKind::Number => {
                let label = laid.parts.first().copied().unwrap_or(0.0);
                self.label(n, name, at, size, label);
                let num = |k: &str, d: f64| n.props.get(k).and_then(Value::as_f64).unwrap_or(d);
                let (value, by) = (num("value", 0.0), num("step", 1.0));
                let (min, max) = (num("min", f64::MIN), num("max", f64::MAX));
                let x = at.0 + label;
                let room = (size.0 - label).max(3.0 * m.control);
                let button = m.control;
                for (suffix, text, bx) in [("d", "-", x), ("i", "+", x + room - button)] {
                    let bname = format!("{name}{suffix}");
                    self.control(
                        "PushButton",
                        &bname,
                        (bx, at.1),
                        (button, size.1),
                        "",
                        Value::map().with("kind", "button").with("text", text),
                    );
                    self.bind(
                        &bname,
                        n,
                        Binding::Step {
                            value,
                            by,
                            min,
                            max,
                        },
                    );
                }
                self.control(
                    "StaticText",
                    name,
                    (x + button + m.gap, at.1),
                    (room - 2.0 * (button + m.gap), size.1),
                    "",
                    text_state(&number_text(value)),
                );
            }
            NodeKind::Image | NodeKind::Canvas | NodeKind::Chart => {
                let text = match (n.kind, n.text("text")) {
                    (NodeKind::Image, t) if !t.is_empty() => t.to_owned(),
                    (NodeKind::Image, _) => "(a picture, not shown here)".to_owned(),
                    _ => "(a drawing, not shown here)".to_owned(),
                };
                self.control(
                    "StaticText",
                    name,
                    at,
                    (size.0, size.1.min(m.line)),
                    "",
                    text_state(&text),
                );
            }
        }
    }

    fn list(
        &mut self,
        n: &UiNode,
        laid: &layout::Laid,
        at: (f32, f32),
        size: (f32, f32),
        name: &str,
        m: &Metrics,
    ) {
        let headings = layout::headings(n);
        let head = if headings.iter().any(|h| !h.is_empty()) {
            m.line + 2.0
        } else {
            0.0
        };
        let widths: Vec<f32> = if laid.parts.is_empty() {
            vec![size.0 - 2.0 * m.rim]
        } else {
            laid.parts.clone()
        };
        if head > 0.0 {
            let mut x = at.0 + m.rim + m.gap;
            for (i, (h, w)) in headings.iter().zip(&widths).enumerate() {
                if !h.is_empty() {
                    self.control(
                        "StaticText",
                        &format!("{name}h{i}"),
                        (x, at.1),
                        (*w, m.line),
                        "",
                        text_state(h),
                    );
                }
                x += w + m.cell;
            }
        }
        let mut cols = String::new();
        for w in &widths {
            let _ = writeln!(
                cols,
                "<column progid=\"DecalControls.TextColumn\" fixedwidth=\"{}\" />",
                px(*w + m.cell)
            );
        }
        let rows: Vec<Value> = layout::rows(n)
            .iter()
            .map(|r| {
                let cells = r
                    .as_list()
                    .map_or_else(|| vec![r.clone()], <[Value]>::to_vec);
                Value::List(
                    cells
                        .iter()
                        .map(|c| {
                            let (text, color, _) = cell_parts(c);
                            let text = if text.is_empty() && c.as_map().is_none() {
                                layout::cell_text(c)
                            } else {
                                text
                            };
                            match color {
                                Some(rgb) => Value::map().with("text", text).with("color", rgb),
                                None => Value::from(text),
                            }
                        })
                        .collect(),
                )
            })
            .collect();
        let selected = n
            .props
            .get("selected")
            .and_then(Value::as_i64)
            .map_or(-1, |s| s - 1);
        self.control(
            "List",
            name,
            (at.0, at.1 + head),
            (size.0, size.1 - head),
            &cols,
            Value::map()
                .with("kind", "list")
                .with("rows", rows)
                .with("selected", selected),
        );
        self.bind(name, n, Binding::Select);
    }
}

/// A number as a plugin reads a control's value: whole when it is whole.
fn number(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        #[allow(clippy::cast_possible_truncation)] // whole, and within an i64
        Value::Int(v as i64)
    } else {
        Value::Num(v)
    }
}

fn page_value(page: usize) -> i64 {
    i64::try_from(page).unwrap_or(0)
}

fn text_state(text: &str) -> Value {
    Value::map().with("kind", "text").with("text", text)
}

/// A number as a stepper shows it: whole when it is whole.
fn number_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

/// `text` broken into lines of at most `width` characters, at spaces where it can be.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let len = line.chars().count();
            if len > 0 && len + 1 + word.chars().count() > width {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
            while line.chars().count() > width {
                let head: String = line.chars().take(width).collect();
                let rest: String = line.chars().skip(width).collect();
                out.push(head);
                line = rest;
            }
        }
        out.push(line);
    }
    out
}

/// Text for an XML attribute: the five XML characters escaped and control characters left out.
#[must_use]
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a host inside another program: its views and their controls)
    use super::*;

    fn tree(v: &Value) -> UiNode {
        UiNode::window_from_value(v).expect("a window")
    }

    fn node(kind: &str) -> Value {
        Value::map().with("kind", kind)
    }

    #[test]
    fn a_window_becomes_a_fixed_layout_of_named_controls() {
        let t = tree(&node("window").with("title", "XP & more").with(
            "children",
            vec![
                node("label").with("id", "earned").with("text", "0 xp"),
                node("row").with(
                    "children",
                    vec![
                        node("button").with("id", "reset").with("text", "Reset"),
                        node("toggle").with("id", "auto").with("text", "Auto").with("value", true),
                    ],
                ),
            ],
        ));
        let b = build(&t, (0.0, 0.0), "x");
        assert!(
            b.xml.starts_with(
                "<?xml version=\"1.0\"?>\n<view icon=\"8712\" title=\"XP &amp; more\""
            ),
            "{}",
            b.xml
        );
        assert!(
            b.xml
                .contains("progid=\"DecalControls.StaticText\" name=\"c_0\""),
            "{}",
            b.xml
        );
        assert!(
            b.xml
                .contains("progid=\"DecalControls.PushButton\" name=\"c_1_0\""),
            "{}",
            b.xml
        );
        assert!(
            b.xml
                .contains("progid=\"DecalControls.CheckBox\" name=\"c_1_1\""),
            "{}",
            b.xml
        );
        assert_eq!(b.states["c_1_1"].get("checked").as_bool(), Some(true));
        assert_eq!(b.bound["c_1_0"].node, "reset");
    }

    #[test]
    fn a_label_that_grows_a_little_keeps_the_view_and_its_places() {
        let with = |text: &str| {
            tree(&node("window").with(
                "children",
                vec![node("row").with(
                    "children",
                    vec![
                        node("label").with("id", "rate").with("text", text),
                        node("button").with("id", "go").with("text", "Go"),
                    ],
                )],
            ))
        };
        let a = build(&with("1234 xp/h"), (0.0, 0.0), "x");
        let b = build(&with("12345 xp/h"), a.size, "x");
        assert_eq!(a.skeleton, b.skeleton, "a few more characters move nothing");
        assert_ne!(
            a.states["c_0_0"], b.states["c_0_0"],
            "the text itself changes"
        );
    }

    #[test]
    fn tabs_become_a_notebook_with_a_page_each() {
        let t = tree(&node("window").with(
            "children",
            vec![node("tabs").with("id", "pages").with(
                "children",
                vec![
                        node("group")
                            .with("title", "One")
                            .with("children", vec![node("label").with("text", "a")]),
                        node("group").with("title", "Two").with(
                            "children",
                            vec![node("button").with("id", "b").with("text", "B")],
                        ),
                    ],
            )],
        ));
        let b = build(&t, (0.0, 0.0), "x");
        assert!(
            b.xml
                .contains("progid=\"DecalControls.Notebook\" name=\"c_0\""),
            "{}",
            b.xml
        );
        assert!(
            b.xml.contains("<page label=\"One\">") && b.xml.contains("<page label=\"Two\">"),
            "{}",
            b.xml
        );
        assert!(
            b.bound.contains_key("c_0_1_0"),
            "the second page's button: {:?}",
            b.bound.keys()
        );
    }

    #[test]
    fn a_list_carries_its_columns_rows_and_headings() {
        let t = tree(&node("window").with(
            "children",
            vec![node("list")
                    .with("id", "kills")
                    .with("columns", vec![Value::from("Name"), Value::from("Kills")])
                    .with(
                        "rows",
                        vec![
                            Value::List(vec![Value::from("Drudge"), Value::from(3)]),
                            Value::List(vec![
                                Value::map().with("text", "Mite").with("color", "#FF0000"),
                                Value::from(1),
                            ]),
                        ],
                    )
                    .with("selected", 2)],
        ));
        let b = build(&t, (0.0, 0.0), "x");
        assert_eq!(
            b.xml.matches("DecalControls.TextColumn").count(),
            2,
            "{}",
            b.xml
        );
        assert!(
            b.states.contains_key("c_0h0") && b.states.contains_key("c_0h1"),
            "headings: {:?}",
            b.states.keys()
        );
        let s = &b.states["c_0"];
        assert_eq!(s.get("selected").as_i64(), Some(1), "rows from 0");
        let rows = s.get("rows").as_list().expect("rows");
        assert_eq!(rows[0].as_list().expect("cells")[1].as_str(), Some("3"));
        assert_eq!(
            rows[1].as_list().expect("cells")[0].get("color").as_i64(),
            Some(0xFF_00_00)
        );
    }

    #[test]
    fn a_view_never_shrinks_below_what_it_was() {
        let t = tree(&node("window").with("children", vec![node("label").with("text", "a")]));
        let b = build(&t, (300.0, 200.0), "x");
        assert_eq!(b.size, (300.0, 200.0));
        assert!(b.xml.contains("width=\"308\" height=\"208\""), "{}", b.xml);
    }

    #[test]
    fn text_is_wrapped_at_spaces_and_escaped_for_xml() {
        assert_eq!(wrap("one two three", 8), vec!["one two", "three"]);
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(escape("a<b>&\"c'\u{7}"), "a&lt;b&gt;&amp;&quot;c&apos;");
    }
}
