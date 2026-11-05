//! The element tree: elements made from their descriptions, switched between states, given
//! attributes, pictures and children by the state they are in, and placed inside their parents.
//!
//! An element made from a reference has two descriptions: the reference as the layout places it
//! (its name, place and edges) and the template it stands for (its kind). Its children are the
//! template's and the reference's, of the current state and of the base state; its attributes
//! come from the reference before the template and from the current state before the base state,
//! the first value given winning; its pictures come from one state's media list.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use auberean_assets::ui::{AttributeValue, UiLayout};

use crate::desc::{DescIx, LayoutIx, Library};
use crate::source::Assets;
use crate::token::{attr, kind, state, Token};
use crate::Rect;

/// An element in a [`Ui`]. A handle to a removed element is refused, never another element.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Handle {
    index: u32,
    generation: u32,
}

impl Handle {
    /// The element's slot (a handle to a removed element may share it with a newer one).
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }
}

/// An attribute's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Id(u32),
    Int(i32),
    Float(f32),
    Bool(bool),
    Text(String),
    StringToken(u32),
    DataId(u32),
    /// Wide text a panel sets.
    Unicode(String),
    /// A game object's id.
    InstanceId(u64),
}

impl Value {
    fn from_record(v: &AttributeValue) -> Option<Self> {
        Some(match v {
            AttributeValue::Id(x) => Self::Id(*x),
            AttributeValue::Int(x) => Self::Int(*x),
            AttributeValue::Float(x) => Self::Float(*x),
            AttributeValue::Bool(x) => Self::Bool(*x),
            AttributeValue::Text(s) => Self::Text(s.clone()),
            AttributeValue::StringToken(x) => Self::StringToken(*x),
            AttributeValue::DataFile(d) => Self::DataId(d.0),
            AttributeValue::Empty(_) => return None,
        })
    }
}

/// What an element tells its listeners: a click, a press, a new selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    /// The element that sent it.
    pub from: Handle,
    /// Its name.
    pub element: Token,
    /// What happened.
    pub what: Token,
    /// A detail: the selected row of a list box.
    pub param: u32,
}

/// The picture an element shows, and how its media item draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picture {
    pub id: u32,
    /// 0 copies the picture's texels, alpha included; 1 and 2 blend it over what is beneath by
    /// its alpha; 3 copies it.
    pub mode: u32,
}

pub use crate::types::button::ButtonState;

/// A list box's rows and which is selected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListState {
    pub items: Vec<Handle>,
    pub selected: Option<usize>,
}

/// One element.
#[derive(Clone, Debug)]
pub struct Node {
    /// The description it was made from, followed through references: its kind's.
    pub desc: DescIx,
    /// The description as the layout places it: its name, place and edges.
    pub refdesc: DescIx,
    pub id: Token,
    pub kind: Token,
    pub state: Token,
    /// Attributes in a table per type, as the original keeps them: one name may have a value of
    /// each type.
    attrs: HashMap<(Token, u8), Value>,
    pub parent: Option<Handle>,
    /// In depth order, nearest first: the last is drawn first.
    pub children: Vec<Handle>,
    /// Relative to the parent's top left, or to the screen for a top-level element.
    pub rect: Rect,
    pub z: i32,
    pub(crate) sized: bool,
    pub visible: bool,
    /// Whether the pointer sees it (`mouse_invisible` turns it off).
    pub mouse_visible: bool,
    created_by_layout: bool,
    latest_update: u64,
    /// The description and index of the state whose media list plays.
    pub(crate) media: Option<(DescIx, usize)>,
    pub(crate) media_index: i32,
    pub picture: Option<Picture>,
    /// A picture not its size is drawn once in its middle, neither repeated nor stretched (a
    /// panel's own choice for a portrait or an icon).
    pub center_image: bool,
    pub button: ButtonState,
    pub list: ListState,
    /// A meter's fill, 0 to 1.
    pub meter: Option<f32>,
    /// Made: its first state set, its kind's own start done.
    pub(crate) ready: bool,
    /// The pointer is over it; it is the topmost element under the pointer.
    pub over: bool,
    pub over_top: bool,
    /// The pointer buttons that went down on it and have not come up.
    pub(crate) buttons: Vec<u32>,
    /// It has the keyboard.
    pub active: bool,
    /// Moving or resizing with the pointer, the border held, and where the pointer was.
    pub moving: bool,
    pub resizing: bool,
    pub(crate) border: crate::types::resizebar::Border,
    pub(crate) mouse_offset: (i32, i32),
    /// The line showing the new size while resizing with a resize line.
    pub resize_line: Option<Rect>,
    /// A drag bar or resize bar held down.
    pub(crate) held: bool,
    /// Its own cursor, and one kept to put back.
    pub cursor: Option<crate::manager::Cursor>,
    pub(crate) saved_cursor: Option<crate::manager::Cursor>,
    /// The state a container had before showing whether it takes the dragged item.
    pub(crate) saved_state: Option<Token>,
    // Panels, pages, viewports and dragging items.
    /// Made and told so: its kind's own finishing done (a panel listening to its pages).
    pub initialized: bool,
    /// A page tab's page: the element it shows while open.
    pub page: Option<Handle>,
    /// What a container asks before an item is dragged out of it, and before one is dropped in
    /// (the item, the container); without one, it takes and gives freely.
    pub drag_out_check: Option<crate::dragdrop::DragCheck>,
    pub drop_in_check: Option<crate::dragdrop::DragCheck>,
    /// The item a container is having dragged out of it (the copy follows the pointer).
    pub dragged_original: Option<Handle>,
    // The media and transitions.
    /// Its picture blends over what is beneath it.
    pub blends: bool,
    /// Its transparency for pictures drawn faded (`-1` until it first shows a picture).
    pub alpha: f32,
    /// A picture used as its transparency.
    pub alpha_picture: Option<u32>,
    /// A top-level element is drawn as a whole at this alpha (its fades set it).
    pub object_alpha: f32,
    /// A top-level element: drawn as a whole, and able to run transitions.
    pub owns_object: bool,
    /// Its transitions under way.
    pub(crate) transitions: crate::media::RunningList,
    /// The state it goes to once the transitions run as it leaves a state are done, and that
    /// they are.
    pub(crate) pending_state: Token,
    pub(crate) left: bool,
    /// It has a description of its own (a copy of its template, changed).
    pub(crate) own_desc: bool,
    /// The place it starts in, kept for the character.
    pub default_location: Option<crate::location::Location>,
    /// The list kinds' own state: scroll bars, lists, trees, menus, meters.
    pub(crate) lists: crate::types::listbox::ListKinds,
    // The text element's.
    /// A text element's glyphs, caret, selection, view and font.
    pub text_state: Option<Box<crate::types::text::TextState>>,
    /// The element's own tooltip text, in place of its `tooltip_entry`; or its own tip as a
    /// string reference, which the tooltip's text element writes.
    pub(crate) tooltip_text: Option<String>,
    pub(crate) tooltip_info: Option<Box<auberean_text::StringInfo>>,
    /// A top-level element's drawn object turned in the screen's plane about its middle, whole
    /// degrees counterclockwise.
    pub rotation: i32,
    /// Rectangles of one color painted over the element once it is drawn (a panel drawing on
    /// its picture): each inclusive, from the element's top left, blue, green, red, alpha.
    pub fills: Vec<(Rect, [u8; 4])>,
    /// A picture a panel drew, blended over the element once it is drawn, its top left at a point
    /// of the element (from the element's top left).
    pub overlay: Option<(i32, i32, crate::draw::Surface)>,
}

impl Node {
    /// Whether a pointer button went down on it and has not come up.
    #[must_use]
    pub fn holds(&self, button: u32) -> bool {
        self.buttons.contains(&button)
    }

    /// A size held within the element's greatest, then least, height and width.
    // D2-11393
    #[must_use]
    pub fn clamp_size(&self, w: i32, h: i32) -> (i32, i32) {
        let (mut w, mut h) = (w, h);
        if let Some(m) = self.int_attr(attr::MAX_HEIGHT) {
            h = h.min(m);
        }
        if let Some(m) = self.int_attr(attr::MAX_WIDTH) {
            w = w.min(m);
        }
        if let Some(m) = self.int_attr(attr::MIN_HEIGHT) {
            h = h.max(m);
        }
        if let Some(m) = self.int_attr(attr::MIN_WIDTH) {
            w = w.max(m);
        }
        (w, h)
    }

    /// An attribute's value of any type, if set (the first type that has one, in the order the
    /// records number them).
    #[must_use]
    pub fn attr(&self, a: Token) -> Option<&Value> {
        (1..=9).find_map(|t| self.attrs.get(&(a, t)))
    }

    /// A yes-or-no attribute: false when not set.
    // D2-11342
    #[must_use]
    pub fn bool_attr(&self, a: Token) -> bool {
        matches!(self.attrs.get(&(a, table::BOOL)), Some(Value::Bool(true)))
    }

    // D2-11340
    #[must_use]
    pub fn int_attr(&self, a: Token) -> Option<i32> {
        match self.attrs.get(&(a, table::INT)) {
            Some(Value::Int(v)) => Some(*v),
            _ => None,
        }
    }

    // D2-11341
    #[must_use]
    pub fn float_attr(&self, a: Token) -> Option<f32> {
        match self.attrs.get(&(a, table::FLOAT)) {
            Some(Value::Float(v)) => Some(*v),
            _ => None,
        }
    }

    /// An id attribute, else a data id one.
    // D2-11337
    #[must_use]
    pub fn id_attr(&self, a: Token) -> Option<u32> {
        match self.attrs.get(&(a, table::ID)) {
            Some(Value::Id(v)) => Some(*v),
            _ => self.data_id_attr(a),
        }
    }

    // D2-11343
    #[must_use]
    pub fn data_id_attr(&self, a: Token) -> Option<u32> {
        match self.attrs.get(&(a, table::DATA_ID)) {
            Some(Value::DataId(v)) => Some(*v),
            _ => None,
        }
    }

    /// A string-table token attribute.
    // D2-11339
    #[must_use]
    pub fn string_id_attr(&self, a: Token) -> Option<u32> {
        match self.attrs.get(&(a, table::STRING_ID)) {
            Some(Value::StringToken(v)) => Some(*v),
            _ => None,
        }
    }

    /// A text attribute.
    // D2-11344
    #[must_use]
    pub fn text_attr(&self, a: Token) -> Option<&str> {
        match self
            .attrs
            .get(&(a, table::TEXT))
            .or_else(|| self.attrs.get(&(a, table::UNICODE)))
        {
            Some(Value::Text(v) | Value::Unicode(v)) => Some(v),
            _ => None,
        }
    }

    /// An object attribute (a game object's id).
    // D2-11338
    #[must_use]
    pub fn instance_id_attr(&self, a: Token) -> Option<u64> {
        match self.attrs.get(&(a, table::INSTANCE_ID)) {
            Some(Value::InstanceId(v)) => Some(*v),
            _ => None,
        }
    }

    /// Its kind.
    // D2-11283
    #[must_use]
    pub const fn kind(&self) -> Token {
        self.kind
    }

    /// Its name.
    // D2-11286
    #[must_use]
    pub const fn id(&self) -> Token {
        self.id
    }

    /// Its state.
    // D2-11284
    #[must_use]
    pub const fn state(&self) -> Token {
        self.state
    }

    /// The element it is in.
    // D2-11285
    #[must_use]
    pub const fn parent(&self) -> Option<Handle> {
        self.parent
    }

    /// Whether a panel finished setting it up.
    // D2-11288
    #[must_use]
    pub const fn is_initialized(&self) -> bool {
        self.initialized
    }
}

/// The attribute tables, by the number the records give each type.
pub(crate) mod table {
    pub const ID: u8 = 1;
    pub const INT: u8 = 2;
    pub const FLOAT: u8 = 3;
    pub const BOOL: u8 = 4;
    pub const TEXT: u8 = 5;
    pub const STRING_ID: u8 = 6;
    pub const DATA_ID: u8 = 7;
    pub const UNICODE: u8 = 8;
    pub const INSTANCE_ID: u8 = 9;
}

impl Value {
    /// The table a value is kept in.
    pub(crate) const fn table(&self) -> u8 {
        match self {
            Self::Id(_) => table::ID,
            Self::Int(_) => table::INT,
            Self::Float(_) => table::FLOAT,
            Self::Bool(_) => table::BOOL,
            Self::Text(_) => table::TEXT,
            Self::StringToken(_) => table::STRING_ID,
            Self::DataId(_) => table::DATA_ID,
            Self::Unicode(_) => table::UNICODE,
            Self::InstanceId(_) => table::INSTANCE_ID,
        }
    }
}

/// The interface: every element, the top-level ones in depth order, and the messages sent.
pub struct Ui {
    assets: Arc<dyn Assets + Send + Sync>,
    lib: Library,
    nodes: Vec<Option<Node>>,
    generations: Vec<u32>,
    free: Vec<u32>,
    /// Top-level elements, nearest first.
    roots: Vec<Handle>,
    update_count: u64,
    /// Attributes already given in the state change under way: the first value wins.
    given: HashSet<Token>,
    display: (i32, i32),
    pub(crate) outbox: Vec<Message>,
    /// States queued by media items, applied once the change that played them is done.
    pending_states: Vec<(Handle, Token)>,
    /// The pointer, the keyboard, listeners, the cursor, the clock.
    pub(crate) mgr: crate::manager::Manager,
    /// Counts every change to the tree: a host redraws the interface only when it moved.
    revision: u64,
    /// What the media items reached, and the sounds and videos they asked for.
    pub(crate) media_state: crate::media::MediaState,
    // The text elements'.
    /// The keyboard's modifiers and the clipboard.
    pub(crate) keyboard: crate::types::text::Keyboard,
    /// The tooltip shown, and when tooltips show.
    pub(crate) tips: crate::tooltip::Tooltips,
}

impl std::fmt::Debug for Ui {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ui")
            .field("roots", &self.roots.len())
            .finish_non_exhaustive()
    }
}

impl Ui {
    /// An empty interface on a screen `width` by `height`.
    // D2-11274
    #[must_use]
    pub fn new(assets: Arc<dyn Assets + Send + Sync>, width: i32, height: i32) -> Self {
        Self {
            assets,
            lib: Library::new(),
            nodes: Vec::new(),
            generations: Vec::new(),
            free: Vec::new(),
            roots: Vec::new(),
            update_count: 0,
            given: HashSet::new(),
            display: (width, height),
            outbox: Vec::new(),
            pending_states: Vec::new(),
            mgr: crate::manager::Manager::default(),
            revision: 0,
            media_state: crate::media::MediaState::default(),
            keyboard: crate::types::text::Keyboard::default(),
            tips: crate::tooltip::Tooltips::default(),
        }
    }

    /// A number that changes whenever anything that is drawn may have changed.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Note a change that is drawn: the screen is drawn again.
    // D2-11166, D2-11167, D2-11232, D2-11231, D2-11745
    pub(crate) fn touch(&mut self) {
        self.revision += 1;
    }

    #[must_use]
    pub fn assets(&self) -> &Arc<dyn Assets + Send + Sync> {
        &self.assets
    }

    #[must_use]
    pub fn library(&self) -> &Library {
        &self.lib
    }

    pub(crate) fn library_mut(&mut self) -> &mut Library {
        &mut self.lib
    }

    #[must_use]
    pub const fn display(&self) -> (i32, i32) {
        self.display
    }

    /// The screen's new size.
    pub(crate) fn set_display(&mut self, width: i32, height: i32) {
        self.display = (width, height);
        self.revision += 1;
    }

    /// Load a layout record by id (once); `None` if the install lacks it.
    pub fn load_layout(&mut self, id: u32) -> Option<LayoutIx> {
        if let Some(ix) = self.lib.layout_by_id(id) {
            return Some(ix);
        }
        let l: Arc<UiLayout> = self.assets.layout(id)?;
        Some(self.lib.add(id, &l))
    }

    // ---------------------------------------------------------------------------------------
    // The arena
    // ---------------------------------------------------------------------------------------

    #[must_use]
    pub fn node(&self, h: Handle) -> Option<&Node> {
        let n = self.nodes.get(h.index as usize)?.as_ref()?;
        (self.generations[h.index as usize] == h.generation).then_some(n)
    }

    pub(crate) fn node_mut(&mut self, h: Handle) -> Option<&mut Node> {
        self.revision += 1;
        if self.generations.get(h.index as usize) != Some(&h.generation) {
            return None;
        }
        self.nodes.get_mut(h.index as usize)?.as_mut()
    }

    fn alloc(&mut self, n: Node) -> Handle {
        self.revision += 1;
        if let Some(i) = self.free.pop() {
            self.nodes[i as usize] = Some(n);
            Handle {
                index: i,
                generation: self.generations[i as usize],
            }
        } else {
            let i = u32::try_from(self.nodes.len()).unwrap_or(u32::MAX);
            self.nodes.push(Some(n));
            self.generations.push(0);
            Handle {
                index: i,
                generation: 0,
            }
        }
    }

    /// The place a parent stands in now, as its children are placed in it.
    // D2-11313
    fn parent_current(&self, p: Handle) -> Rect {
        self.node(p).map(|pn| pn.rect).unwrap_or_default()
    }

    /// Remove an element and everything under it: each stops hearing anything and is forgotten
    /// by the manager.
    // D2-11377, D2-11389, D2-11177, D2-11249, D2-11245, D2-11209
    pub fn remove(&mut self, h: Handle) {
        if self
            .node(h)
            .is_some_and(|n| n.kind == crate::token::kind::PANEL)
        {
            self.panel_removing(h);
        }
        self.kind_deleting(h);
        let Some(n) = self.node(h) else { return };
        let kids = n.children.clone();
        let parent = n.parent;
        for k in kids {
            self.remove(k);
        }
        if let Some(p) = parent.and_then(|p| self.node_mut(p)) {
            p.children.retain(|c| *c != h);
            p.list.items.retain(|c| *c != h);
        }
        self.roots.retain(|r| *r != h);
        self.deleting_element(h);
        if let Some(l) = self.node(h).map(|n| self.lib.get(n.desc).layout) {
            self.lib.drop_ref(l);
        }
        self.revision += 1;
        let i = h.index as usize;
        self.nodes[i] = None;
        self.generations[i] = self.generations[i].wrapping_add(1);
        self.free.push(h.index);
    }

    /// The top-level elements, nearest first, as the pointer meets them; of equal depths the first
    /// put on the screen comes first.
    #[must_use]
    pub fn roots(&self) -> &[Handle] {
        &self.roots
    }

    /// Messages sent since the last call.
    pub fn take_messages(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.outbox)
    }

    /// The manager forgets an element going away: the keyboard, the pointer, its listening.
    // D2-11262
    fn deleting_element(&mut self, h: Handle) {
        let m = &mut self.mgr;
        if m.active == Some(h) {
            m.active = None;
        }
        if m.top == Some(h) {
            m.top = None;
        }
        let tip_owner = m.tooltip_owner == Some(h);
        if m.resizing == Some(h) {
            m.resizing = None;
        }
        m.activatables.retain(|x| *x != h);
        if let Some(n) = self.nodes.get(h.index() as usize).and_then(|n| n.as_ref()) {
            if let Some(l) = self.mgr.by_name.get_mut(&n.id) {
                l.retain(|x| *x != h);
            }
        }
        self.mgr.object_listeners.retain(|(r, _), _| *r != h);
        self.mgr.object_clean.retain(|(_, r, _)| *r != h);
        if self.mgr.drag == Some(h) {
            self.mgr.drag = None;
        }
        if self.mgr.drag_owner == Some(h) {
            self.mgr.drag_owner = None;
        }
        self.delete_listener_from_queues(crate::listener::Listener::Element(h));
        // Its tooltip goes with it, the pointer unmoved.
        if tip_owner {
            self.mgr.tooltip_owner = None;
            self.clear_tooltip();
        }
    }

    /// Take a root off the screen: it stays made, but nothing draws it or reaches it.
    pub(crate) fn take_root_off_screen(&mut self, h: Handle) {
        self.roots.retain(|r| *r != h);
        self.revision += 1;
    }

    /// Put a root in front of the others.
    pub(crate) fn bring_root_to_front(&mut self, root: Handle) {
        self.roots.retain(|r| *r != root);
        self.roots.insert(0, root);
        self.revision += 1;
    }

    // ---------------------------------------------------------------------------------------
    // Making elements
    // ---------------------------------------------------------------------------------------

    /// Make a layout's top-level element and put it on the screen.
    pub fn create_root(&mut self, layout: LayoutIx, name: Token) -> Option<Handle> {
        let d = self.lib.root(layout, name)?;
        let h = self.create_element_with(d, true)?;
        self.add_root(h);
        Some(h)
    }

    /// Make a layout's top-level element without placing it anywhere (a row for a list).
    pub fn create(&mut self, layout: LayoutIx, name: Token) -> Option<Handle> {
        let d = self.lib.root(layout, name)?;
        self.create_element(d)
    }

    /// Make an element from a description: its kind from what it stands for, its first state
    /// the reference's default, then its children.
    // D2-11248, D2-11321, D2-11184
    pub fn create_element(&mut self, refdesc: DescIx) -> Option<Handle> {
        self.create_element_with(refdesc, false)
    }

    /// Make an element; a top-level one drawn as a whole owns its object. Every element hears
    /// the clock.
    // D2-11321
    fn create_element_with(&mut self, refdesc: DescIx, owns_object: bool) -> Option<Handle> {
        let desc = self.lib.dereference(refdesc)?;
        let kind = self.lib.referent_kind(refdesc)?;
        let id = self.lib.get(refdesc).id;
        self.lib.add_ref(self.lib.get(desc).layout);
        let h = self.alloc(Node {
            desc,
            refdesc,
            id,
            kind,
            state: Token(0),
            attrs: HashMap::new(),
            parent: None,
            children: Vec::new(),
            rect: Rect::new(0, 0, -1, -1),
            z: 0,
            sized: false,
            visible: true,
            mouse_visible: true,
            created_by_layout: false,
            latest_update: 0,
            media: None,
            media_index: 0,
            picture: None,
            center_image: false,
            button: ButtonState::default(),
            list: ListState::default(),
            meter: None,
            ready: false,
            over: false,
            over_top: false,
            buttons: Vec::new(),
            active: false,
            moving: false,
            resizing: false,
            border: crate::types::resizebar::Border::None,
            mouse_offset: (0, 0),
            resize_line: None,
            held: false,
            cursor: None,
            saved_cursor: None,
            saved_state: None,
            initialized: false,
            page: None,
            drag_out_check: None,
            drop_in_check: None,
            dragged_original: None,
            blends: false,
            alpha: -1.0,
            alpha_picture: None,
            object_alpha: 1.0,
            owns_object,
            transitions: crate::media::RunningList::default(),
            pending_state: Token(0),
            left: false,
            own_desc: false,
            default_location: None,
            lists: crate::types::listbox::ListKinds::default(),
            text_state: (kind == crate::token::kind::TEXT)
                .then(|| Box::new(crate::types::text::TextState::default())),
            tooltip_text: None,
            tooltip_info: None,
            rotation: 0,
            fills: Vec::new(),
            overlay: None,
        });
        self.note_name(h);
        self.register_for_global_message(h, crate::manager::global::TIME);
        let first = self.lib.get(refdesc).default_state;
        self.set_state(h, first);
        self.update(h);
        // Each child made with it is told it is made.
        let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
        for c in kids {
            self.initialized(c);
        }
        self.set_visible(h, true);
        self.flush_pending();
        // What each kind does when it is made (a field is made as any element is, remembering no
        // state of its own).
        // D2-11271, D2-11263, D2-11493, D2-11497
        if kind == crate::token::kind::BUTTON {
            self.button_created(h);
        } else if kind == crate::token::kind::DRAGBAR {
            self.dragbar_created(h);
        } else if kind == crate::token::kind::RESIZEBAR {
            self.resizebar_created(h);
        } else if kind == crate::token::kind::SCROLLBAR {
            self.scrollbar_created(h);
        } else if kind == crate::token::kind::VIEWPORT {
            self.viewport_created(h);
        } else if kind == crate::token::kind::TEXT {
            self.text_created(h);
        }
        self.list_kind_created(h, kind);
        if let Some(n) = self.node_mut(h) {
            n.ready = true;
        }
        if self.is_activatable(h) {
            self.register_activatable(h);
        }
        Some(h)
    }

    /// Put an element on the screen among the top-level ones, by its depth.
    // D2-11204
    pub fn add_root(&mut self, h: Handle) {
        self.roots.retain(|r| *r != h);
        let z = self.node(h).map_or(0, |n| n.z);
        let at = self.root_at(&self.roots, z);
        self.roots.insert(at, h);
        self.place_tree(h);
    }

    /// Where a top-level element of depth `z` goes in a list kept nearest first: after every one at
    /// its depth or nearer, so of equal depths the first put on the screen stays nearest.
    // D2-10386
    fn root_at(&self, list: &[Handle], z: i32) -> usize {
        list.iter()
            .position(|c| self.node(*c).is_some_and(|n| n.z > z))
            .unwrap_or(list.len())
    }

    /// Where an element of depth `z` goes in a list kept nearest first: after every nearer one,
    /// before the first as deep or deeper (so of equal depths the newest is in front).
    // D2-11299
    fn insert_at(&self, list: &[Handle], z: i32) -> usize {
        if z == 0xFFFF {
            return list.len();
        }
        if let Some(last) = list.last().and_then(|l| self.node(*l)) {
            if last.z < z {
                return list.len();
            }
        }
        list.iter()
            .position(|c| self.node(*c).is_some_and(|n| z <= n.z))
            .unwrap_or(list.len())
    }

    /// Make `child` a child of `parent`, kept in depth order (taken out of its old parent first).
    // D2-11323, D2-11168
    pub fn set_parent(&mut self, child: Handle, parent: Handle) {
        if let Some(old) = self.node(child).and_then(|n| n.parent) {
            if let Some(o) = self.node_mut(old) {
                o.children.retain(|c| *c != child);
            }
        }
        self.roots.retain(|r| *r != child);
        let z = self.node(child).map_or(0, |n| n.z);
        let Some(p) = self.node(parent) else { return };
        let at = self.insert_at(&p.children, z);
        if let Some(p) = self.node_mut(parent) {
            p.children.insert(at, child);
        }
        if let Some(c) = self.node_mut(child) {
            c.parent = Some(parent);
        }
        self.place_tree(child);
    }

    // ---------------------------------------------------------------------------------------
    // States
    // ---------------------------------------------------------------------------------------

    /// The state an element's description puts it in when it is made.
    #[must_use]
    pub fn default_state(&self, h: Handle) -> Option<Token> {
        self.node(h).map(|n| self.lib.get(n.refdesc).default_state)
    }

    /// Put an element in a state, as its kind does.
    pub fn set_state(&mut self, h: Handle, s: Token) -> bool {
        if self.node(h).is_none() {
            return false;
        }
        self.kind_set_state(h, s)
    }

    /// The state change every element makes. Already in the state with nothing running, or
    /// already leaving for it, nothing happens. Otherwise the transitions under way stop; the
    /// transitions that run as the element leaves its state for this one start (unless they just
    /// ran), and while any runs the change waits for them. Then the new state's attributes (the
    /// reference's before the template's, the state's before the base state's), its media list
    /// from the start, its children, the state passed on to the children when the state says so,
    /// and the transitions that run as the element enters it from the state it was in.
    // D2-11397
    pub(crate) fn set_state_base(&mut self, h: Handle, s: Token) -> bool {
        let Some(n) = self.node(h) else { return false };
        if s == n.state {
            if n.transitions.len() == 0 {
                return true;
            }
        } else if n.transitions.len() != 0 && n.transitions_first_leaves() && s == n.pending_state {
            return true;
        }
        let current = n.state;
        let left = n.left;
        self.stop_all_transitions(h);
        if !left {
            self.start_transitions(h, current, crate::media::when::LEAVE, s, s);
        }
        let Some(n) = self.node_mut(h) else {
            return false;
        };
        if n.transitions.len() != 0 {
            return true;
        }
        n.left = false;
        let (desc, refdesc) = (n.desc, n.refdesc);
        self.given.clear();
        if refdesc != desc {
            self.add_attributes_for_state(h, refdesc, s);
        }
        self.add_attributes_for_state(h, desc, s);
        self.given.clear();
        if let Some(n) = self.node_mut(h) {
            n.state = s;
        }
        self.reset_media(h);
        self.update(h);
        let pass = if s == state::STATELESS {
            self.lib.state(desc, state::STATELESS)
        } else {
            self.lib.state(desc, s)
        }
        .is_some_and(|st| st.passes_state_to_children());
        if pass {
            let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
            for k in kids {
                self.set_state(k, s);
            }
        }
        self.start_transitions(h, s, crate::media::when::ENTER, current, s);
        true
    }

    /// Change an element's state once the change under way is done.
    pub(crate) fn queue_state(&mut self, h: Handle, s: Token) {
        self.pending_states.push((h, s));
    }

    /// The attributes of one description's state and its base state, each given once.
    // D2-11366, D2-11349, D2-11306
    fn add_attributes_for_state(&mut self, h: Handle, d: DescIx, s: Token) {
        let mut list = Vec::new();
        if s != state::STATELESS {
            if let Some(st) = self.lib.state(d, s) {
                list.extend(st.attributes.iter().cloned());
            }
        }
        if let Some(st) = self.lib.state(d, state::STATELESS) {
            list.extend(st.attributes.iter().cloned());
        }
        for a in list {
            let t = Token(a.id);
            if self.given.contains(&t) {
                continue;
            }
            if let Some(v) = Value::from_record(&a.value) {
                self.set_attr(h, t, v);
            }
            self.given.insert(t);
        }
    }

    pub(crate) fn flush_pending(&mut self) {
        while !self.pending_states.is_empty() {
            let p = std::mem::take(&mut self.pending_states);
            for (h, s) in p {
                self.set_state(h, s);
            }
        }
    }

    /// Bring the children in line with the state: keep those still described, make the new
    /// ones, drop the rest; then place the element.
    // D2-11395, D2-11392
    pub fn update(&mut self, h: Handle) {
        self.update_count += 1;
        let uc = self.update_count;
        let Some(n) = self.node_mut(h) else { return };
        n.latest_update = uc;
        let (desc, refdesc, s) = (n.desc, n.refdesc, n.state);
        let mut wanted = Vec::new();
        let mut lists = vec![(desc, s), (desc, state::STATELESS)];
        if refdesc != desc {
            lists.push((refdesc, s));
            lists.push((refdesc, state::STATELESS));
        }
        let mut seen_states = HashSet::new();
        for (d, st) in lists {
            if !seen_states.insert((d, st)) {
                continue;
            }
            if let Some(sd) = self.lib.state(d, st) {
                wanted.extend(sd.children.iter().copied());
            }
        }
        for cd in wanted {
            self.update_child(h, cd, uc);
        }
        let stale: Vec<Handle> = self
            .node(h)
            .map(|n| n.children.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|c| {
                self.node(*c)
                    .is_some_and(|cn| cn.created_by_layout && cn.latest_update < uc)
            })
            .collect();
        for c in stale {
            self.remove(c);
        }
        self.place_tree(h);
    }

    /// Keep, replace or make the child a description names. A child of the same kind made by the
    /// layout is kept: as it is when its descriptions are the same, else switched to the new
    /// description (its own values, such as attributes a panel gave it, stay).
    // D2-11396
    fn update_child(&mut self, h: Handle, cd: DescIx, uc: u64) {
        let cid = self.lib.get(cd).id;
        let existing = self.node(h).and_then(|n| {
            n.children
                .iter()
                .copied()
                .find(|c| self.node(*c).is_some_and(|cn| cn.id == cid))
        });
        if let Some(c) = existing {
            if self.node(c).is_some_and(|cn| cn.latest_update >= uc) {
                return;
            }
        }
        let Some(target) = self.lib.dereference(cd) else {
            return;
        };
        if let Some(c) = existing {
            let cn = self
                .node(c)
                .map(|cn| (cn.desc, cn.refdesc, cn.created_by_layout));
            if let Some((cdesc, crefdesc, by_layout)) = cn {
                let same_kind = self.lib.get(cdesc).kind == self.lib.get(target).kind;
                if by_layout && same_kind {
                    if cdesc == target && crefdesc == cd {
                        let uc_now = self.update_count;
                        if let Some(cn) = self.node_mut(c) {
                            cn.latest_update = uc_now;
                        }
                    } else {
                        self.switch_description(c, cd);
                        self.set_parent(c, h);
                    }
                    return;
                }
            }
            self.remove(c);
        }
        if let Some(c) = self.create_element(cd) {
            let uc_now = self.update_count;
            if let Some(cn) = self.node_mut(c) {
                cn.created_by_layout = true;
                cn.latest_update = uc_now;
            }
            self.set_parent(c, h);
        }
    }

    // ---------------------------------------------------------------------------------------
    // Attributes
    // ---------------------------------------------------------------------------------------

    /// Set an attribute, with what the element's kind does when it changes.
    // D2-11378, D2-11379, D2-11380, D2-11381, D2-11382, D2-11384, D2-11385, D2-11386
    pub fn set_attr(&mut self, h: Handle, a: Token, v: Value) {
        let Some(n) = self.node_mut(h) else { return };
        let key = (a, v.table());
        let changed = n.attrs.get(&key) != Some(&v);
        n.attrs.insert(key, v.clone());
        let k = n.kind;
        let ready = n.ready;
        if k == kind::TEXT {
            self.text_attr(h, a, &v, changed);
        }
        if !changed {
            if ready {
                self.kind_value_set(h, a, &v, false);
            }
            return;
        }
        match (a, &v) {
            // Before the element is made, a state's flags only show or hide it and let the
            // pointer through; once made, its kind follows them too.
            (t, Value::Bool(b)) if self.node(h).is_some_and(|n| n.ready) => {
                self.bool_changed(h, t, *b);
            }
            (t, Value::Bool(b)) if t == attr::HIDE => self.set_visible(h, !*b),
            (t, Value::Bool(b)) if t == attr::MOUSE_INVISIBLE => self.set_mouse_visible(h, !*b),
            (t, Value::Int(_))
                if t == attr::ACTIVATE_ORDER && self.node(h).is_some_and(|n| n.ready) =>
            {
                self.update_activate_order(h);
            }
            _ => {}
        }
        if ready {
            self.kind_value_set(h, a, &v, true);
        }
    }

    /// An element that takes the keyboard takes its new place in the order elements take it.
    // D2-11315
    fn update_activate_order(&mut self, h: Handle) {
        if !self.is_activatable(h) {
            return;
        }
        self.unregister_activatable(h);
        if self.is_activatable(h) {
            self.register_activatable(h);
        }
        let active = self.node(h).is_some_and(|n| n.active);
        self.activation_alert(h, active);
    }

    /// The string table an element's text tokens are read from: its layout's.
    // D2-11297
    #[must_use]
    pub fn string_table(&self, h: Handle) -> u32 {
        self.node(h).map_or(0, |n| {
            let l = self.lib.get(n.refdesc).layout;
            self.lib.layout(l).string_table
        })
    }

    /// Show or hide an element. When that changes whether it shows: an element whose place is
    /// Whether a picture not the element's size is drawn once in its middle.
    pub fn set_center_image(&mut self, h: Handle, on: bool) {
        if let Some(n) = self.node_mut(h) {
            n.center_image = on;
        }
    }

    /// kept says its place changed; it says whether it shows now; hidden, it gives up the
    /// keyboard; shown, one that takes the keyboard on showing takes it (unless the use key is
    /// held).
    // D2-11360, D2-11171
    pub fn set_visible(&mut self, h: Handle, v: bool) {
        let Some(n) = self.node(h) else { return };
        let parent_shown = n.parent.is_none_or(|p| self.shown(p));
        let before = parent_shown && n.visible;
        if let Some(n) = self.node_mut(h) {
            n.visible = v;
        }
        self.changing_element_visibility();
        let after = parent_shown && v;
        if before == after || !self.node(h).is_some_and(|n| n.ready) {
            return;
        }
        let Some(n) = self.node(h) else { return };
        if n.bool_attr(attr::SAVE_LOC) || n.bool_attr(attr::BROADCAST_LOC_CHANGE) {
            self.broadcast(h, crate::token::msg::LOCATION_CHANGED, 0);
        }
        self.broadcast(h, crate::token::msg::SET_SHOW_STATE, u32::from(after));
        let Some(n) = self.node(h) else { return };
        if n.active && !after {
            self.deactivate_element(h);
            return;
        }
        // The use key held (it may be what showed this) would press what took the keyboard.
        if after && n.bool_attr(attr::ACTIVATE_ON_SHOW) && !self.mgr.use_down {
            self.activate_element(h);
        }
    }

    /// Turn a top-level element's drawn object about its middle: the turn in the screen's plane,
    /// whole degrees, as the original's rotate values are kept.
    // D2-10440, D2-10429
    pub fn set_rotation(&mut self, h: Handle, degrees: i32) {
        let turn = degrees % 360;
        if self.node(h).is_some_and(|n| n.rotation != turn) {
            if let Some(n) = self.node_mut(h) {
                n.rotation = turn;
            }
        }
    }

    /// Paint rectangles of one color over an element once it is drawn, in place of the last
    /// ones painted.
    pub fn set_fills(&mut self, h: Handle, fills: Vec<(Rect, [u8; 4])>) {
        if self.node(h).is_some_and(|n| n.fills != fills) {
            if let Some(n) = self.node_mut(h) {
                n.fills = fills;
            }
        }
    }

    /// Blend a picture a panel drew over an element once it is drawn, its top left at `(x, y)`
    /// of the element, in place of the last one; `None` takes it away.
    pub fn set_overlay(&mut self, h: Handle, overlay: Option<(i32, i32, crate::draw::Surface)>) {
        if self.node(h).is_some_and(|n| n.overlay != overlay) {
            if let Some(n) = self.node_mut(h) {
                n.overlay = overlay;
            }
        }
    }

    /// An element hidden while its tooltip would show drops the tooltip, also when it went with
    /// an element it lies in.
    // D2-11205
    fn changing_element_visibility(&mut self) {
        if self.mgr.tooltip_owner.is_some_and(|o| !self.shown(o)) {
            self.mgr.tooltip_owner = None;
            self.clear_tooltip();
        }
    }

    /// The text entry taking typed characters, if any: the element with the keyboard, when it is
    /// an editable text element.
    #[must_use]
    pub fn focus(&self) -> Option<Handle> {
        self.active().filter(|h| {
            self.text_state(*h)
                .is_some_and(|t| t.has(crate::types::text::bits::EDITABLE))
        })
    }

    /// Give an editable text element the keyboard; a selectable one has its text selected, so the
    /// first character typed replaces it.
    pub fn activate(&mut self, h: Handle) -> bool {
        let Some(t) = self.text_state(h) else {
            return false;
        };
        if !t.has(crate::types::text::bits::EDITABLE) {
            return false;
        }
        let selectable = t.has(crate::types::text::bits::SELECTABLE);
        self.activate_element(h);
        if !self.node(h).is_some_and(|n| n.active) {
            return false;
        }
        if selectable {
            self.select_all(h);
        }
        self.revision += 1;
        true
    }

    /// Take the keyboard from whatever element has it.
    pub fn deactivate(&mut self) {
        if let Some(a) = self.active() {
            self.deactivate_element(a);
        }
        self.mgr.active = None;
        self.revision += 1;
    }

    // ---------------------------------------------------------------------------------------
    // Descriptions
    // ---------------------------------------------------------------------------------------

    /// A new element made from an element's template description (the template, not where the
    /// layout places it), top-level when the element is.
    // D2-11307
    pub fn copy_element(&mut self, h: Handle) -> Option<Handle> {
        let n = self.node(h)?;
        let (desc, owns) = (n.desc, n.owns_object);
        self.create_element_with(desc, owns)
    }

    /// Change the description an element is made from: where it stands from the new one, what it
    /// is from what the new one stands for (a description of its own given up); its media list
    /// and children follow the new description.
    // D2-11394
    pub fn switch_description(&mut self, h: Handle, refdesc: DescIx) {
        let Some(desc) = self.lib.dereference(refdesc) else {
            return;
        };
        let Some(n) = self.node_mut(h) else { return };
        n.refdesc = refdesc;
        n.own_desc = false;
        n.desc = desc;
        self.reset_media(h);
        self.update(h);
    }

    /// Give another element every attribute this one has and it lacks, table by table (text
    /// tokens, ids, objects, whole numbers, fractions, yes-or-no values, data ids, text); within
    /// a table by name. Then each child does the same for the other's first child made from the
    /// same description (a child the other lacks ends it there); a text element then gives the
    /// other its text, when theirs differ.
    pub fn match_attributes(&mut self, h: Handle, other: Handle) -> bool {
        let matched = self.element_match_attributes(h, other);
        if matched
            && self
                .node(h)
                .is_some_and(|n| n.kind == crate::token::kind::TEXT)
        {
            self.text_match_attributes(h, other);
        }
        matched
    }

    // D2-11350
    fn element_match_attributes(&mut self, h: Handle, other: Handle) -> bool {
        let Some(n) = self.node(h) else { return false };
        if self.node(other).is_none() {
            return false;
        }
        let order = [
            table::STRING_ID,
            table::ID,
            table::INSTANCE_ID,
            table::INT,
            table::FLOAT,
            table::BOOL,
            table::DATA_ID,
            table::TEXT,
        ];
        let mut give: Vec<((Token, u8), Value)> = n
            .attrs
            .iter()
            .filter(|((_, t), _)| order.contains(t))
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        give.sort_by_key(|((name, t), _)| (order.iter().position(|o| o == t), *name));
        for ((name, t), v) in give {
            let lacks = self
                .node(other)
                .is_some_and(|o| !o.attrs.contains_key(&(name, t)));
            if lacks {
                self.set_attr(other, name, v);
            }
        }
        let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
        for c in kids {
            let Some(id) = self.node(c).map(|n| n.id) else {
                continue;
            };
            let twin = self.node(other).and_then(|o| {
                o.children
                    .iter()
                    .copied()
                    .find(|k| self.node(*k).is_some_and(|kn| kn.id == id))
            });
            let Some(twin) = twin else {
                return true;
            };
            self.match_attributes(c, twin);
        }
        true
    }

    // ---------------------------------------------------------------------------------------
    // Finding elements
    // ---------------------------------------------------------------------------------------

    /// The first element named `name` under `h`, searching each child before the next one's
    /// siblings, nearest first.
    // D2-11296
    #[must_use]
    pub fn child_recursive(&self, h: Handle, name: Token) -> Option<Handle> {
        for c in &self.node(h)?.children {
            if self.node(*c)?.id == name {
                return Some(*c);
            }
            if let Some(f) = self.child_recursive(*c, name) {
                return Some(f);
            }
        }
        None
    }

    /// A direct child by name.
    // D2-11295
    #[must_use]
    pub fn child(&self, h: Handle, name: Token) -> Option<Handle> {
        self.node(h)?
            .children
            .iter()
            .copied()
            .find(|c| self.node(*c).is_some_and(|n| n.id == name))
    }

    /// The element named `name` anywhere on the screen.
    #[must_use]
    pub fn find(&self, name: Token) -> Option<Handle> {
        for r in &self.roots {
            if self.node(*r)?.id == name {
                return Some(*r);
            }
            if let Some(f) = self.child_recursive(*r, name) {
                return Some(f);
            }
        }
        None
    }

    // ---------------------------------------------------------------------------------------
    // Places
    // ---------------------------------------------------------------------------------------

    /// An element's rectangle on the screen.
    // D2-11162, D2-11319, D2-11320
    #[must_use]
    pub fn screen_rect(&self, h: Handle) -> Option<Rect> {
        let n = self.node(h)?;
        let mut r = n.rect;
        let mut p = n.parent;
        while let Some(ph) = p {
            let pn = self.node(ph)?;
            r = r.offset(pn.rect.x0, pn.rect.y0);
            p = pn.parent;
        }
        Some(r)
    }

    /// Whether an element and every element above it are shown.
    #[must_use]
    pub fn shown(&self, h: Handle) -> bool {
        let mut cur = Some(h);
        while let Some(c) = cur {
            match self.node(c) {
                Some(n) if n.visible => cur = n.parent,
                _ => return false,
            }
        }
        true
    }

    /// The rectangle and depth a description gives an element in a state: the state's own, else
    /// the base state's, else the element's.
    // D2-11357, D2-11372
    fn position_from_desc(&self, d: DescIx, s: Token) -> (Rect, i32) {
        let pick = self
            .lib
            .state(d, s)
            .and_then(|st| st.rect)
            .or_else(|| self.lib.state(d, state::STATELESS).and_then(|st| st.rect));
        pick.unwrap_or_else(|| {
            let e = self.lib.get(d);
            (e.rect, e.z)
        })
    }

    /// The parent's size as the child's description saw it: the reference's place when the
    /// child is one of the reference's own, else the template's.
    // D2-11388
    fn parent_original(&self, parent: Handle, child_id: Token) -> Rect {
        let Some(p) = self.node(parent) else {
            return Rect::default();
        };
        let in_ref = [p.state, state::STATELESS].iter().any(|s| {
            self.lib
                .state(p.refdesc, *s)
                .is_some_and(|st| st.children.iter().any(|c| self.lib.get(*c).id == child_id))
        });
        let d = if in_ref { p.refdesc } else { p.desc };
        self.position_from_desc(d, p.state).0
    }

    /// Place one element inside its parent by its edges: an edge at 0 stays where it is once
    /// placed, 1 keeps its distance from the parent's near side (right and bottom: far side),
    /// 2 keeps its distance from the far side (right and bottom: near side), 3 centers.
    // D2-11391, D2-11387
    pub(crate) fn place(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        let (refdesc, s, cur, sized, id) = (n.refdesc, n.state, n.rect, n.sized, n.id);
        let (orig_parent, cur_parent) = match n.parent {
            None => {
                let l = self.lib.layout(self.lib.get(refdesc).layout);
                (
                    Rect::sized(0, 0, i32::from(l.width), i32::from(l.height)),
                    Rect::sized(0, 0, self.display.0, self.display.1),
                )
            }
            Some(p) => (self.parent_original(p, id), self.parent_current(p)),
        };
        let (orig, z) = self.position_from_desc(refdesc, s);
        let edges = self.lib.get(refdesc).edges;
        let dx = (cur_parent.x1 - cur_parent.x0) - (orig_parent.x1 - orig_parent.x0);
        let ch = cur_parent.y1 - cur_parent.y0;
        let dy = ch - (orig_parent.y1 - orig_parent.y0);
        let cw = cur_parent.x1 - cur_parent.x0;
        let (ow, oh) = (orig.width(), orig.height());
        let mut x0 = match edges[0] {
            2 => orig.x0 + dx,
            3 => (cw + 1) / 2 - ow / 2,
            _ => orig.x0,
        };
        let mut x1 = match edges[2] {
            1 => {
                let v = orig.x1 + dx;
                if v < 1 {
                    orig.x1
                } else {
                    v
                }
            }
            3 => ow / 2 - 1 + (cw + 1) / 2,
            _ => orig.x1,
        };
        let mut y0 = match edges[1] {
            2 => orig.y0 + dy,
            3 => (ch + 1) / 2 - oh / 2,
            _ => orig.y0,
        };
        let mut y1 = match edges[3] {
            1 => {
                let v = orig.y1 + dy;
                if v < 1 {
                    orig.y1
                } else {
                    v
                }
            }
            3 => oh / 2 - 1 + (ch + 1) / 2,
            _ => orig.y1,
        };
        if sized {
            if edges[0] == 0 {
                x0 = cur.x0;
            }
            if edges[1] == 0 {
                y0 = cur.y0;
            }
            if edges[2] == 0 {
                x1 = cur.x1;
            }
            if edges[3] == 0 {
                y1 = cur.y1;
            }
        }
        // The size its edges give it is held within the element's least and greatest sizes, as
        // any resize is (a toolbar window keeps its width on a screen larger than its layout's).
        let (w, hh) = n.clamp_size(x1 - x0 + 1, y1 - y0 + 1);
        x1 = x0 + w - 1;
        y1 = y0 + hh - 1;
        // A top-level element reaching past the screen's right or bottom edge is moved back onto
        // it, when it fits.
        if n.parent.is_none() {
            let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
            if x1 >= self.display.0 && w <= self.display.0 {
                x0 = self.display.0 - w;
                x1 = self.display.0 - 1;
            }
            if y1 >= self.display.1 && h <= self.display.1 {
                y0 = self.display.1 - h;
                y1 = self.display.1 - 1;
            }
        }
        let old_z = n.z;
        let parent = n.parent;
        let resized = n.text_state.is_some()
            && (n.rect.width(), n.rect.height()) != (x1 - x0 + 1, y1 - y0 + 1);
        if let Some(n) = self.node_mut(h) {
            n.rect = Rect::new(x0, y0, x1, y1);
            n.z = z;
            n.sized = true;
        }
        if resized {
            self.text_resized(h);
        }
        if old_z != z {
            self.reorder(h, parent);
        }
    }

    /// Put an element at another depth: its place in its parent's (or the screen's) depth order
    /// follows (a toolbar window brought to the front).
    // D2-11287
    pub fn set_z_level(&mut self, h: Handle, z: i32) {
        let Some(n) = self.node_mut(h) else { return };
        if n.z == z {
            return;
        }
        n.z = z;
        let parent = n.parent;
        self.reorder(h, parent);
    }

    /// The transparency an element's faded pictures are drawn at, 0 to 1 (the element alone).
    pub fn set_alpha(&mut self, h: Handle, alpha: f32) {
        if let Some(n) = self.node_mut(h) {
            n.alpha = alpha;
        }
        self.touch();
    }

    /// Keep an element's place in its parent's (or the screen's) depth order after its depth
    /// changed.
    // D2-11287
    fn reorder(&mut self, h: Handle, parent: Option<Handle>) {
        let z = self.node(h).map_or(0, |n| n.z);
        match parent {
            Some(p) => {
                let Some(pn) = self.node(p) else { return };
                let mut list = pn.children.clone();
                list.retain(|c| *c != h);
                let at = self.insert_at(&list, z);
                list.insert(at, h);
                if let Some(pn) = self.node_mut(p) {
                    pn.children = list;
                }
            }
            None => {
                if self.roots.contains(&h) {
                    let mut list = self.roots.clone();
                    list.retain(|c| *c != h);
                    let at = self.root_at(&list, z);
                    list.insert(at, h);
                    self.roots = list;
                }
            }
        }
    }

    /// Place an element, then everything under it.
    pub fn place_tree(&mut self, h: Handle) {
        self.place(h);
        let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
        let is_list = self
            .node(h)
            .is_some_and(|n| n.kind == kind::LISTBOX || n.kind == kind::LISTBOX_MULTI_SIZE);
        for k in kids {
            if is_list && self.node(h).is_some_and(|n| n.list.items.contains(&k)) {
                continue;
            }
            self.place_tree(k);
        }
        if is_list {
            self.listbox_update_layout(h);
        }
        match self.node(h).map(|n| n.kind) {
            Some(k) if k == kind::SCROLLBAR => self.lay_out_scrollbar(h),
            Some(k) if k == kind::METER => self.lay_out_meter(h),
            _ => {}
        }
    }

    pub(crate) fn place_children(&mut self, h: Handle) {
        let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
        for k in kids {
            self.place_tree(k);
        }
    }
}
