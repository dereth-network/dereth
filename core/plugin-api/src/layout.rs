//! A declared window laid out: what each node needs and asks for, and where it stands, by the
//! rules every host follows (`docs/plugins/ui.md`, "Layout").
//!
//! Each node has, across and down, a **least** size it cannot be made smaller than, a **natural**
//! size it asks for, a **most** size, and a **grow** share of the room left over along its
//! container's direction. A `column` (a window, a HUD, a group, a tab's page) sets its nodes one
//! above the other, a `row` side by side; across, a node takes its container's whole width (a
//! column) or height (a row) unless it keeps its own size. Room left over goes to the growing
//! nodes in proportion to their shares; room short is taken from the nodes above their least size
//! in proportion to how far above it they are.
//!
//! The pass is pure: [`want`] reads the tree once, measuring its text, and [`place`] lays it out in
//! a given size as often as the size changes, answering a [`Laid`] tree parallel to the declared
//! one (a `tabs` node's children are its pages). Sizes are in layout units (a screen pixel at
//! scale 1); a host measures its own text ([`Measure`]) in its own font ([`Metrics`]). A host with
//! no display lays windows out by [`Metrics::reference`] and [`Cells`], each character a cell, as
//! the conformance suite does ([`reference_dump`]).

use std::fmt::Write as _;

use crate::ui::{cell_parts, NodeKind, UiNode};
use crate::value::Value;

/// The measures a tree is laid out by, in layout units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// A character cell's width: what a width hint counts.
    pub cell: f32,
    /// A line of text: what a height hint counts.
    pub line: f32,
    /// A one-line control's height: a button, a field, a row of tabs.
    pub control: f32,
    /// Between nodes stacked in a column (a row's are twice it).
    pub gap: f32,
    /// Inside a group's border.
    pub pad: f32,
    /// A check box's side.
    pub check: f32,
    /// A scrollbar's width.
    pub scrollbar: f32,
    /// The rim of the well a list and a text box are drawn in.
    pub rim: f32,
}

impl Metrics {
    /// The measures of a font whose cell is `cell` wide and whose lines are `line` apart, with
    /// check boxes `check` a side and scrollbars `scrollbar` wide.
    #[must_use]
    pub fn of(cell: f32, line: f32, check: f32, scrollbar: f32) -> Self {
        let line = line.max(8.0);
        Self {
            cell: cell.max(3.0),
            line,
            control: (line + 6.0).round().max(check + 4.0),
            gap: 3.0,
            pad: 6.0,
            check,
            scrollbar,
            rim: 2.0,
        }
    }

    /// The measures of a host with no display: a cell 7 wide, a line 16, a check box 11 and a
    /// scrollbar 8 (a control 22, a gap 3, a group's padding 6).
    #[must_use]
    pub fn reference() -> Self {
        Self::of(7.0, 16.0, 11.0, 8.0)
    }

    /// The room a button keeps at each end of its label: its art's cap (half its height) and a
    /// gap.
    #[must_use]
    pub fn button_end(&self) -> f32 {
        (self.control / 2.0 + self.gap).round()
    }

    /// The room a scrolling body or list keeps at its right for its scrollbar.
    #[must_use]
    pub fn bar_room(&self) -> f32 {
        self.scrollbar + self.gap
    }

    /// A list row's height.
    #[must_use]
    pub fn row(&self) -> f32 {
        (self.line + 2.0).round()
    }
}

/// How wide a host draws text.
pub trait Measure {
    /// The width of `text` in the windows' text.
    fn text(&self, text: &str) -> f32;
    /// The width of a button's label, in the face the host draws it in.
    fn label(&self, text: &str) -> f32 {
        self.text(text)
    }
}

/// The measure of a host with no display: each character `0` layout units wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cells(pub f32);

impl Measure for Cells {
    fn text(&self, text: &str) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let n = text.chars().count() as f32;
        n * self.0
    }
}

/// One node laid out: where it stands in its parent (its top left, from the parent's; a group's
/// nodes from the group's own top left), its size, and the parts the drawing needs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Laid {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// A labelled control: its label's width before the control. A list: each column's width.
    pub parts: Vec<f32>,
    /// A list: how many rows show at once.
    pub shown: usize,
    pub children: Vec<Laid>,
    /// A window whose body (or a tab's page) is shorter than its nodes' least height: their
    /// height, through which it scrolls. The nodes are laid out that tall and as wide as the body
    /// less its scrollbar's room.
    pub scroll: Option<f32>,
}

/// One size along an axis: the least, the natural (what the node asks for) and the most.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub least: f32,
    pub natural: f32,
    pub most: f32,
}

impl Span {
    fn fixed(v: f32) -> Self {
        Self {
            least: v,
            natural: v,
            most: f32::INFINITY,
        }
    }

    fn new(least: f32, natural: f32) -> Self {
        Self {
            least: least.min(natural),
            natural,
            most: f32::INFINITY,
        }
    }

    /// Kept within a least and a most, the natural between them.
    fn bounded(mut self, least: Option<f32>, most: Option<f32>) -> Self {
        if let Some(l) = least {
            self.least = self.least.max(l);
        }
        if let Some(m) = most {
            self.most = self.most.min(m);
        }
        self.most = self.most.max(self.least);
        self.natural = self.natural.clamp(self.least, self.most);
        self
    }

    fn clamp(&self, v: f32) -> f32 {
        v.clamp(self.least, self.most)
    }
}

/// One of a list's columns: a fixed width, or a share of the room left and a least width.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Column {
    span: Span,
    grow: f32,
}

/// What a node needs and asks for, measured once.
#[derive(Debug, Clone, PartialEq)]
pub struct Want {
    pub w: Span,
    pub h: Span,
    /// Its share of the room left over along its container's direction.
    pub grow: f32,
    /// It takes a column's whole width.
    wide: bool,
    /// It takes a row's whole height.
    tall: bool,
    /// A labelled control: its label's width.
    label: f32,
    columns: Vec<Column>,
    /// A list: its rows, and the room its headings and icons take.
    rows: usize,
    head: f32,
    icon: f32,
    children: Vec<Want>,
}

impl Want {
    fn leaf(w: Span, h: Span) -> Self {
        Self {
            w,
            h,
            grow: 0.0,
            wide: true,
            tall: false,
            label: 0.0,
            columns: Vec::new(),
            rows: 0,
            head: 0.0,
            icon: 0.0,
            children: Vec::new(),
        }
    }

    /// The least size, across and down.
    #[must_use]
    pub fn least(&self) -> (f32, f32) {
        (self.w.least, self.h.least)
    }

    /// The natural size, across and down: what the node asks for.
    #[must_use]
    pub fn natural(&self) -> (f32, f32) {
        (self.w.natural, self.h.natural)
    }

    /// The most size, across and down (unbounded where nothing bounds it).
    #[must_use]
    pub fn most(&self) -> (f32, f32) {
        (self.w.most, self.h.most)
    }
}

/// The widest a list's column asks to be, in cells.
const COLUMN_CELLS: f32 = 40.0;

/// How many of a list's rows are measured for its columns' widths.
const MEASURED_ROWS: usize = 500;

/// The widest a choice's box asks to be for its longest option, in cells.
const CHOICE_CELLS: f32 = 40.0;

/// A whole-number property, if it is one.
fn int(n: &UiNode, k: &str) -> Option<i64> {
    n.props.get(k).and_then(Value::as_i64)
}

/// A count of cells or lines as layout units.
#[allow(clippy::cast_precision_loss)]
fn units(v: i64, unit: f32) -> f32 {
    v.clamp(0, 10_000) as f32 * unit
}

/// A width hint, in layout units.
fn hint(n: &UiNode, m: &Metrics) -> Option<f32> {
    int(n, "width").map(|c| units(c.max(1), m.cell))
}

/// A count of lines (or rows), at least one.
#[allow(clippy::cast_precision_loss)]
fn lines(n: &UiNode, k: &str, default: i64) -> f32 {
    int(n, k).unwrap_or(default).clamp(1, 200) as f32
}

/// A list cell's text.
#[must_use]
pub fn cell_text(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Num(n) => format!("{n}"),
        Value::Map(_) => cell_parts(v).0,
        other => other.to_json(),
    }
}

/// A list's rows.
#[must_use]
pub fn rows(n: &UiNode) -> &[Value] {
    n.props
        .get("rows")
        .and_then(Value::as_list)
        .unwrap_or_default()
}

/// A list's column headings: each column's `title`, or the column itself where it is a string.
#[must_use]
pub fn headings(n: &UiNode) -> Vec<String> {
    n.props
        .get("columns")
        .and_then(Value::as_list)
        .unwrap_or_default()
        .iter()
        .map(|c| match c {
            Value::Map(_) => c.get("title").as_str().unwrap_or_default().to_owned(),
            other => cell_text(other),
        })
        .collect()
}

/// Whether any of a list's rows names an icon.
#[must_use]
pub fn has_icons(n: &UiNode) -> bool {
    rows(n).iter().take(MEASURED_ROWS).any(|r| {
        r.as_list()
            .unwrap_or_default()
            .iter()
            .any(|c| cell_parts(c).2.is_some())
    })
}

/// What a hotkey node shows: its text (or its command) and the key it asks for.
#[must_use]
pub fn hotkey_line(n: &UiNode) -> String {
    let what = if n.text("text").is_empty() {
        format!("/{}", n.text("command"))
    } else {
        n.text("text").to_owned()
    };
    if n.text("key").is_empty() {
        what
    } else {
        format!("{what}  [{}]", n.text("key"))
    }
}

/// Whether the kind is a control with a label before it.
fn labelled(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Number | NodeKind::Choice | NodeKind::Input | NodeKind::Slider
    )
}

/// A labelled control's label width: its text and a cell, or nothing for no text.
fn label_width(n: &UiNode, m: &Metrics, measure: &dyn Measure) -> f32 {
    let t = n.text("text");
    if t.is_empty() {
        0.0
    } else {
        (measure.text(t) + m.cell).ceil()
    }
}

/// How a container sets its nodes across: stretched (the default), or each at its own size at the
/// start, the middle or the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Align {
    Stretch,
    Start,
    Center,
    End,
}

fn align_of(n: &UiNode) -> Align {
    match n.text("align") {
        "start" => Align::Start,
        "center" => Align::Center,
        "end" => Align::End,
        _ => Align::Stretch,
    }
}

/// The gap between a container's nodes: its `gap`, else a column's own or twice it in a row.
fn gap_of(n: &UiNode, m: &Metrics) -> f32 {
    int(n, "gap").map_or_else(
        || {
            if n.kind == NodeKind::Row {
                2.0 * m.gap
            } else {
                m.gap
            }
        },
        |g| units(g, 1.0),
    )
}

/// The nodes of each part of a window that grow when nothing there says: in the window outside
/// its tabs, and in each tab's page, where no node declares `grow`, the last list or text box.
fn auto_growing(tree: &UiNode) -> Vec<*const UiNode> {
    fn scope<'a>(n: &'a UiNode, nodes: &mut Vec<&'a UiNode>, pages: &mut Vec<&'a UiNode>) {
        for c in &n.children {
            nodes.push(c);
            if c.kind == NodeKind::Tabs {
                pages.extend(c.children.iter());
            } else {
                scope(c, nodes, pages);
            }
        }
    }
    let mut out = Vec::new();
    let mut roots = vec![tree];
    while let Some(root) = roots.pop() {
        let (mut nodes, mut pages) = (Vec::new(), Vec::new());
        scope(root, &mut nodes, &mut pages);
        roots.extend(pages);
        if nodes.iter().any(|n| n.props.contains_key("grow")) {
            continue;
        }
        if let Some(last) = nodes
            .iter()
            .rev()
            .find(|n| matches!(n.kind, NodeKind::List | NodeKind::TextBox))
        {
            out.push(std::ptr::from_ref(*last));
        }
    }
    out
}

/// What `tree` (a window's or a HUD's) needs and asks for, measured once.
#[must_use]
pub fn want(tree: &UiNode, m: &Metrics, measure: &dyn Measure) -> Want {
    let auto = auto_growing(tree);
    want_node(tree, m, measure, &auto)
}

fn want_node(n: &UiNode, m: &Metrics, measure: &dyn Measure, auto: &[*const UiNode]) -> Want {
    let mut w = match n.kind {
        NodeKind::Label => {
            let natural = hint(n, m).unwrap_or_else(|| measure.text(n.text("text")) + m.cell / 2.0);
            Want::leaf(Span::fixed(natural.ceil()), Span::fixed(m.line))
        }
        NodeKind::Hotkey => Want::leaf(
            Span::fixed((measure.text(&hotkey_line(n)) + m.cell).ceil()),
            Span::fixed(m.line),
        ),
        NodeKind::Button => {
            let natural = hint(n, m).unwrap_or_else(|| {
                (measure.label(n.text("text")) + 2.0 * m.button_end()).max(3.0 * m.cell)
            });
            Want::leaf(Span::fixed(natural.ceil()), Span::fixed(m.control))
        }
        NodeKind::Toggle => Want::leaf(
            Span::fixed(
                (m.check + 2.0 * m.gap + measure.text(n.text("text")) + m.cell / 2.0).ceil(),
            ),
            Span::fixed(m.control),
        ),
        NodeKind::Meter => Want::leaf(
            Span::fixed(hint(n, m).unwrap_or(20.0 * m.cell)),
            Span::fixed((m.line * 0.9).max(12.0).ceil()),
        ),
        NodeKind::Canvas | NodeKind::Chart => {
            let (pw, ph) = n.pixel_size();
            let mut l = Want::leaf(
                Span::fixed(units(pw.clamp(1, 4096), 1.0)),
                Span::fixed(units(ph.clamp(1, 4096), 1.0)),
            );
            l.wide = false;
            l
        }
        NodeKind::Image => {
            let mut l = Want::leaf(
                Span::fixed(hint(n, m).unwrap_or(24.0 * m.cell)),
                Span::fixed(lines(n, "height", 12) * m.line),
            );
            l.wide = false;
            l
        }
        NodeKind::Separator => {
            let t = 2.0 * m.gap + 1.0;
            let mut l = Want::leaf(Span::fixed(t), Span::fixed(t));
            l.tall = true;
            l
        }
        NodeKind::Number => {
            let label = label_width(n, m, measure);
            let mut l = Want::leaf(Span::fixed(label + 14.0 * m.cell), Span::fixed(m.control));
            l.label = label;
            l
        }
        NodeKind::Choice => {
            let label = label_width(n, m, measure);
            let longest = n
                .props
                .get("options")
                .and_then(Value::as_list)
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_str)
                .chain([n.text("value")])
                .map(|o| measure.text(o))
                .fold(0.0, f32::max)
                .min(CHOICE_CELLS * m.cell);
            let field = (longest + 2.0 * m.cell + m.control)
                .max(8.0 * m.cell)
                .ceil();
            let mut l = Want::leaf(Span::fixed(label + field), Span::fixed(m.control));
            l.label = label;
            l
        }
        NodeKind::Input => {
            let label = label_width(n, m, measure);
            let field = hint(n, m).unwrap_or(20.0 * m.cell);
            let mut l = Want::leaf(Span::fixed(label + field), Span::fixed(m.control));
            l.label = label;
            l
        }
        NodeKind::Slider => {
            let label = label_width(n, m, measure);
            let track = hint(n, m).unwrap_or(20.0 * m.cell);
            let mut l = Want::leaf(
                Span::fixed(label + track + 7.0 * m.cell),
                Span::fixed(m.control),
            );
            l.label = label;
            l
        }
        NodeKind::TextBox => {
            let natural = (
                hint(n, m).unwrap_or(30.0 * m.cell),
                lines(n, "height", 4) * m.line + 8.0,
            );
            let mut l = Want::leaf(
                Span::new(natural.0.min(10.0 * m.cell), natural.0),
                Span::new(m.line + 8.0, natural.1),
            );
            l.tall = true;
            l
        }
        NodeKind::List => want_list(n, m, measure),
        NodeKind::Row | NodeKind::Column | NodeKind::Window | NodeKind::Hud | NodeKind::Group => {
            want_box(n, n.kind, m, measure, auto)
        }
        NodeKind::Tabs => want_tabs(n, m, measure, auto),
    };
    // Bounds in cells and lines: a window's are the player's bounds too.
    let cells = |k: &str| int(n, k).map(|v| units(v, m.cell));
    let rows = |k: &str| int(n, k).map(|v| units(v, m.line));
    w.w = w.w.bounded(cells("min_width"), cells("max_width"));
    w.h = w.h.bounded(rows("min_height"), rows("max_height"));
    if let Some(g) = n.props.get("grow").and_then(Value::as_f64) {
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: a share, a small number.
        let g = g as f32;
        w.grow = g;
    } else if auto.contains(&std::ptr::from_ref(n)) {
        w.grow = 1.0;
    }
    w
}

/// A list: its columns' widths from their headings and cells (or as declared), its rows' height.
fn want_list(n: &UiNode, m: &Metrics, measure: &dyn Measure) -> Want {
    let specs: Vec<Value> = n
        .props
        .get("columns")
        .and_then(Value::as_list)
        .unwrap_or_default()
        .to_vec();
    let heads = headings(n);
    let rows = rows(n);
    let count = heads.len().max(
        rows.iter()
            .take(MEASURED_ROWS)
            .map(|r| r.as_list().map_or(0, <[Value]>::len))
            .max()
            .unwrap_or(0),
    );
    let mut columns: Vec<Column> = (0..count)
        .map(|c| {
            let spec = specs.get(c).unwrap_or(&Value::Nil);
            let head = heads.get(c).map_or(0.0, |h| measure.text(h));
            let fixed = spec.get("width").as_i64().map(|v| units(v.max(1), m.cell));
            let least = spec.get("min_width").as_i64().map(|v| units(v, m.cell));
            #[allow(clippy::cast_possible_truncation)]
            // LINT-OK: a share, a small number.
            let grow = spec.get("grow").as_f64().unwrap_or(0.0) as f32;
            match fixed {
                Some(f) => Column {
                    span: Span::fixed(f),
                    grow: 0.0,
                },
                None => {
                    let natural = rows
                        .iter()
                        .take(MEASURED_ROWS)
                        .filter_map(|r| r.as_list().and_then(|cells| cells.get(c)))
                        .map(|v| measure.text(&cell_text(v)))
                        .fold(head, f32::max)
                        .min(COLUMN_CELLS * m.cell)
                        .ceil();
                    let least = least.unwrap_or_else(|| head.max(3.0 * m.cell).ceil());
                    Column {
                        span: Span {
                            least: least.min(natural.max(least)),
                            natural: natural.max(least),
                            most: f32::INFINITY,
                        },
                        grow,
                    }
                }
            }
        })
        .collect();
    // When no column says, the last one of no fixed width grows.
    let declared = specs.iter().any(|s| !s.get("grow").is_nil());
    if !declared {
        if let Some(c) = (0..count)
            .rev()
            .find(|c| specs.get(*c).is_none_or(|s| s.get("width").is_nil()))
        {
            columns[c].grow = 1.0;
        }
    }
    let head = if heads.is_empty() { 0.0 } else { m.line + 2.0 };
    let icon = if has_icons(n) { m.line + m.gap } else { 0.0 };
    #[allow(clippy::cast_precision_loss)]
    let between = m.cell * count.saturating_sub(1) as f32;
    let chrome = 2.0 * (m.rim + m.gap) + icon + between;
    let natural_rows = int(n, "height")
        .and_then(|h| usize::try_from(h.clamp(1, 200)).ok())
        .unwrap_or_else(|| rows.len().clamp(1, 8));
    let bar = if rows.len() > natural_rows {
        m.bar_room()
    } else {
        0.0
    };
    let natural_w = (chrome + bar + columns.iter().map(|c| c.span.natural).sum::<f32>())
        .max(8.0 * m.cell)
        .ceil();
    let least_w = (chrome + bar + columns.iter().map(|c| c.span.least).sum::<f32>())
        .max(8.0 * m.cell)
        .min(natural_w)
        .ceil();
    let rows_h = |shown: usize| {
        #[allow(clippy::cast_precision_loss)]
        let h = head + shown as f32 * m.row() + 2.0 * m.rim;
        h.ceil()
    };
    let mut w = Want::leaf(
        Span::new(least_w, hint(n, m).unwrap_or(natural_w).max(least_w)),
        Span::new(rows_h(natural_rows.min(3)), rows_h(natural_rows)),
    );
    w.tall = true;
    w.columns = columns;
    w.rows = rows.len();
    w.head = head;
    w.icon = icon;
    w
}

/// A column or a row of nodes (a window, a HUD and a group are columns): their sizes added up
/// along it and the largest across.
/// `kind` is how the node sets its nodes: its own, or a column for a tab's page.
fn want_box(
    n: &UiNode,
    kind: NodeKind,
    m: &Metrics,
    measure: &dyn Measure,
    auto: &[*const UiNode],
) -> Want {
    let mut children: Vec<Want> = n
        .children
        .iter()
        .map(|c| want_node(c, m, measure, auto))
        .collect();
    let along_x = kind == NodeKind::Row;
    if !along_x {
        // The labelled controls a column holds have their labels as wide as the widest, so their
        // boxes line up.
        let widest = n
            .children
            .iter()
            .zip(&children)
            .filter(|(c, _)| labelled(c.kind))
            .map(|(_, w)| w.label)
            .fold(0.0, f32::max);
        for (c, w) in n.children.iter().zip(&mut children) {
            if labelled(c.kind) && w.label > 0.0 {
                let more = widest - w.label;
                w.label = widest;
                w.w.least += more;
                w.w.natural += more;
                w.w.most += more;
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let gaps = gap_of(n, m) * children.len().saturating_sub(1) as f32;
    let along = |w: &Want| if along_x { w.w } else { w.h };
    let across = |w: &Want| if along_x { w.h } else { w.w };
    let sum = |f: fn(&Span) -> f32| children.iter().map(|c| f(&along(c))).sum::<f32>() + gaps;
    let max = |f: fn(&Span) -> f32| children.iter().map(|c| f(&across(c))).fold(0.0, f32::max);
    let main = Span::new(sum(|s| s.least), sum(|s| s.natural));
    let cross = Span::new(max(|s| s.least), max(|s| s.natural));
    let (mut wspan, mut hspan) = if along_x {
        (main, cross)
    } else {
        (cross, main)
    };
    if kind == NodeKind::Group {
        let title = n.text("title");
        let head = if title.is_empty() { 0.0 } else { m.line };
        let room = if title.is_empty() {
            0.0
        } else {
            measure.text(title) + m.cell
        };
        for v in [&mut wspan.least, &mut wspan.natural] {
            *v = (v.max(room) + 2.0 * m.pad).ceil();
        }
        for v in [&mut hspan.least, &mut hspan.natural] {
            *v += head + 2.0 * m.pad;
        }
    }
    // A window asks for at least its own width and height, and more where its nodes ask for more.
    if matches!(kind, NodeKind::Window | NodeKind::Hud) {
        if let Some(h) = hint(n, m) {
            wspan.natural = wspan.natural.max(h);
        }
        if let Some(h) = int(n, "height") {
            hspan.natural = hspan.natural.max(units(h.max(1), m.line));
        }
    }
    // A box grows in its container as its growing nodes do; a row's one-line controls grow across
    // it alone, so only the nodes that take a row's height (a list, a box) make the row grow down
    // its column.
    let grow = children
        .iter()
        .filter(|c| !along_x || c.tall)
        .map(|c| c.grow)
        .fold(0.0, f32::max);
    let mut w = Want::leaf(wspan, hspan);
    w.tall = true;
    w.grow = grow;
    w.children = children;
    w
}

/// The fewest lines a tab's page shows before its own nodes scroll.
const PAGE_LINES: f32 = 3.0;

/// Tabs: the row of tabs over the chosen page, as wide as the widest page and as tall as the
/// tallest, so the window keeps its size from page to page. Shorter than the chosen page, the
/// page scrolls under its row of tabs, so their least height is the row and the smaller of the
/// page's least and three lines.
fn want_tabs(n: &UiNode, m: &Metrics, measure: &dyn Measure, auto: &[*const UiNode]) -> Want {
    let pages: Vec<Want> = n
        .children
        .iter()
        .map(|page| want_box(page, NodeKind::Column, m, measure, auto))
        .collect();
    let tab = |p: &UiNode| (measure.text(p.text("title")) + 34.0).max(70.0);
    let bar_w: f32 = n.children.iter().map(tab).sum();
    let chosen = chosen_page(n);
    #[allow(clippy::cast_precision_loss)]
    let squeezed =
        n.children.get(chosen).map_or(0.0, tab) + 28.0 * n.children.len().saturating_sub(1) as f32;
    let bar = m.control + m.gap;
    let natural_w = pages.iter().map(|p| p.w.natural).fold(bar_w, f32::max);
    let least_w = pages.iter().map(|p| p.w.least).fold(squeezed, f32::max);
    let natural_h = bar + pages.iter().map(|p| p.h.natural).fold(0.0, f32::max);
    let least_h = bar
        + pages
            .get(chosen)
            .map_or(0.0, |p| p.h.least.min(PAGE_LINES * m.line));
    let mut w = Want::leaf(
        Span::new(least_w.ceil(), natural_w.ceil()),
        Span::new(least_h, natural_h),
    );
    w.tall = true;
    w.grow = pages.iter().map(|p| p.grow).fold(0.0, f32::max);
    w.children = pages;
    w
}

/// The page a tabs node shows: its `selected`, from 1.
#[must_use]
pub fn chosen_page(n: &UiNode) -> usize {
    n.props
        .get("selected")
        .and_then(Value::as_i64)
        .and_then(|s| usize::try_from(s - 1).ok())
        .unwrap_or(0)
        .min(n.children.len().saturating_sub(1))
}

/// Sizes shared out along a line of nodes `room` long, `gap` apart: each its natural size, the
/// room left over shared among the growing ones by their shares (none past its most), the room
/// short taken from those above their least in proportion to how far above it they are. Each
/// node's start and size, whole units, placed by `justify` where room is left over.
fn share(items: &[(Span, f32)], room: f32, gap: f32, justify: &str) -> Vec<(f32, f32)> {
    #[allow(clippy::cast_precision_loss)]
    let gaps = gap * items.len().saturating_sub(1) as f32;
    let natural: f32 = items.iter().map(|(s, _)| s.natural).sum();
    let mut sizes: Vec<f32> = items.iter().map(|(s, _)| s.natural).collect();
    let mut left = room - gaps - natural;
    if left >= 0.0 {
        let mut open: Vec<usize> = (0..items.len()).filter(|&i| items[i].1 > 0.0).collect();
        while left > 0.01 && !open.is_empty() {
            let total: f32 = open.iter().map(|&i| items[i].1).sum();
            let capped: Vec<usize> = open
                .iter()
                .copied()
                .filter(|&i| sizes[i] + left * items[i].1 / total > items[i].0.most)
                .collect();
            if capped.is_empty() {
                for &i in &open {
                    sizes[i] += left * items[i].1 / total;
                }
                left = 0.0;
            } else {
                for &i in &capped {
                    left -= items[i].0.most - sizes[i];
                    sizes[i] = items[i].0.most;
                }
                open.retain(|i| !capped.contains(i));
            }
        }
    } else {
        let short = -left;
        let room_above: f32 = items.iter().map(|(s, _)| s.natural - s.least).sum();
        for (size, (s, _)) in sizes.iter_mut().zip(items) {
            *size = if room_above <= short {
                s.least
            } else {
                s.natural - short * (s.natural - s.least) / room_above
            };
        }
        left = 0.0;
    }
    let (mut at, between) = match justify {
        "end" => (left, gap),
        "center" => (left / 2.0, gap),
        #[allow(clippy::cast_precision_loss)]
        "between" if items.len() > 1 => (0.0, gap + left / (items.len() - 1) as f32),
        _ => (0.0, gap),
    };
    sizes
        .into_iter()
        .map(|size| {
            let (a, b) = (at.round(), (at + size).round());
            at += size + between;
            (a, b - a)
        })
        .collect()
}

/// Lay `n` out in `size` (its top left at `(0, 0)`): the size is its own, its nodes shared out
/// within it.
#[must_use]
pub fn place(n: &UiNode, w: &Want, m: &Metrics, size: (f32, f32)) -> Laid {
    let (width, height) = (size.0.round(), size.1.round());
    let mut laid = Laid {
        w: width,
        h: height,
        ..Laid::default()
    };
    match n.kind {
        NodeKind::List => {
            let shown = ((height - w.head - 2.0 * m.rim) / m.row()).floor().max(1.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: a count of rows a few hundred at most.
            let shown = shown as usize;
            let bar = if w.rows > shown { m.bar_room() } else { 0.0 };
            #[allow(clippy::cast_precision_loss)]
            let between = m.cell * w.columns.len().saturating_sub(1) as f32;
            let room = width - 2.0 * (m.rim + m.gap) - w.icon - bar - between;
            let cols: Vec<(Span, f32)> = w.columns.iter().map(|c| (c.span, c.grow)).collect();
            laid.parts = share(&cols, room, 0.0, "start")
                .into_iter()
                .map(|(_, s)| s)
                .collect();
            laid.shown = shown;
        }
        NodeKind::Number | NodeKind::Choice | NodeKind::Input | NodeKind::Slider => {
            laid.parts = vec![w.label];
        }
        NodeKind::Tabs => {
            let bar = m.control + m.gap;
            let room = (height - bar).max(0.0);
            laid.children = n
                .children
                .iter()
                .zip(&w.children)
                .map(|(page, pw)| {
                    // A page shorter than its nodes' least height scrolls through them under the
                    // row of tabs, which stays where it is.
                    let mut p = if room + 0.5 >= pw.h.least {
                        place_box(page, pw, m, (width, room), NodeKind::Column)
                    } else {
                        let tall = pw.h.least.ceil();
                        let inner = (width - m.bar_room()).max(pw.w.least);
                        let mut p = place_box(page, pw, m, (inner, tall), NodeKind::Column);
                        p.w = width;
                        p.h = room;
                        p.scroll = Some(tall);
                        p
                    };
                    p.y = bar;
                    p
                })
                .collect();
        }
        NodeKind::Row | NodeKind::Column | NodeKind::Window | NodeKind::Hud | NodeKind::Group => {
            laid = place_box(n, w, m, (width, height), n.kind);
        }
        _ => {}
    }
    laid
}

/// A column or a row laid out in `size`, as `kind` sets its nodes (a tab's page is a column).
fn place_box(n: &UiNode, w: &Want, m: &Metrics, size: (f32, f32), kind: NodeKind) -> Laid {
    let (width, height) = (size.0.round(), size.1.round());
    let (inset, top) = if kind == NodeKind::Group {
        let head = if n.text("title").is_empty() {
            0.0
        } else {
            m.line
        };
        (m.pad, head + m.pad)
    } else {
        (0.0, 0.0)
    };
    let inner = (width - 2.0 * inset, height - top - inset);
    let along_x = kind == NodeKind::Row;
    let gap = gap_of(n, m);
    let items: Vec<(Span, f32)> = w
        .children
        .iter()
        .map(|c| (if along_x { c.w } else { c.h }, c.grow))
        .collect();
    let main = share(
        &items,
        if along_x { inner.0 } else { inner.1 },
        gap,
        n.text("justify"),
    );
    let align = align_of(n);
    let cross_room = if along_x { inner.1 } else { inner.0 };
    let children = n
        .children
        .iter()
        .zip(&w.children)
        .zip(main)
        .map(|((c, cw), (at, len))| {
            let (span, stretches) = if along_x {
                (cw.h, cw.tall)
            } else {
                (cw.w, cw.wide)
            };
            let across = if align == Align::Stretch && stretches {
                span.clamp(cross_room)
            } else {
                span.natural.min(span.most)
            }
            .round();
            let free = cross_room - across;
            let offset = match align {
                Align::Center => free / 2.0,
                Align::End => free,
                // A node keeping its own height in a stretching row stands in the row's middle.
                Align::Stretch if along_x && !stretches => free / 2.0,
                _ => 0.0,
            }
            .max(0.0)
            .round();
            let (cw_size, pos) = if along_x {
                ((len, across), (at, offset))
            } else {
                ((across, len), (offset, at))
            };
            let mut l = place(c, cw, m, cw_size);
            l.x = pos.0 + inset;
            l.y = pos.1 + top;
            l
        })
        .collect();
    Laid {
        w: width,
        h: height,
        children,
        ..Laid::default()
    }
}

/// A window's or a HUD's nodes laid out in its body of `size`: within it where it is at least as
/// tall as their least height (and never narrower than their least width), else at their least
/// height, as wide as the body less its scrollbar's room, for the body to scroll through.
#[must_use]
pub fn place_body(tree: &UiNode, w: &Want, m: &Metrics, size: (f32, f32)) -> Laid {
    let (width, height) = (size.0.round(), size.1.round());
    if height + 0.5 >= w.h.least {
        let mut laid = place(tree, w, m, (width.max(w.w.least), height));
        laid.w = width;
        laid
    } else {
        let tall = w.h.least.ceil();
        let mut laid = place(tree, w, m, ((width - m.bar_room()).max(w.w.least), tall));
        laid.w = width;
        laid.h = height;
        laid.scroll = Some(tall);
        laid
    }
}

/// The body a window asks for: its nodes' natural size, within their least and most.
#[must_use]
pub fn natural_body(w: &Want) -> (f32, f32) {
    (w.w.clamp(w.w.natural).ceil(), w.h.clamp(w.h.natural).ceil())
}

/// `tree` laid out as text, one node a line, indented by depth: the kind, `#id` when there is
/// one, its place in its parent and its size (`x,y wxh`); a labelled control's label width
/// (`label=`), a list's column widths and the rows it shows (`columns=` and `rows=`), and for a
/// body that scrolls the height it scrolls through (`scrolls=`).
#[must_use]
pub fn dump(tree: &UiNode, laid: &Laid) -> String {
    let mut out = String::new();
    dump_into(tree, laid, 0, &mut out);
    out
}

fn dump_into(n: &UiNode, l: &Laid, depth: usize, out: &mut String) {
    let _ = write!(out, "{}{}", "  ".repeat(depth), n.kind.name());
    if let Some(id) = &n.id {
        let _ = write!(out, " #{id}");
    }
    if depth > 0 {
        let _ = write!(out, " {},{}", l.x, l.y);
    }
    let _ = write!(out, " {}x{}", l.w, l.h);
    if let Some(s) = l.scroll {
        let _ = write!(out, " scrolls={s}");
    }
    match n.kind {
        NodeKind::List => {
            let cols: Vec<String> = l.parts.iter().map(f32::to_string).collect();
            let _ = write!(out, " columns={} rows={}", cols.join(","), l.shown);
        }
        NodeKind::Number | NodeKind::Choice | NodeKind::Input | NodeKind::Slider => {
            let _ = write!(out, " label={}", l.parts.first().copied().unwrap_or(0.0));
        }
        _ => {}
    }
    out.push('\n');
    for (c, cl) in n.children.iter().zip(&l.children) {
        dump_into(c, cl, depth + 1, out);
    }
}

/// A window as a host with no display lays it out ([`Metrics::reference`], each character a
/// cell): in its body of `size` cells and lines where the player gave it one, else at its natural
/// size; as text ([`dump`]).
#[must_use]
pub fn reference_dump(tree: &UiNode, size: Option<(i64, i64)>) -> String {
    let m = Metrics::reference();
    let measure = Cells(m.cell);
    let w = want(tree, &m, &measure);
    let body = size.map_or_else(
        || natural_body(&w),
        |(cw, ch)| (units(cw, m.cell), units(ch, m.line)),
    );
    dump(tree, &place_body(tree, &w, &m, body))
}

#[cfg(test)]
mod tests;
