//! The notice registry: which panels hear which notices, and in what order.
//!
//! A panel registers for the notices it handles as it is made, each once: a second registration
//! of the same panel for the same notice keeps the first. A notice is handed only to the panels
//! registered for it, in the order they registered; a panel that goes takes its registrations
//! with it. The play screen's panels register in the order the play screen makes them, and the
//! play screen itself registers some notices as it is made and the rest once its panels are made,
//! after them.
//!
//! Notices are named here as the panels know them: a notice the player's scripts send by the
//! native that sends it (its package, then its number), and the client's own notices between its
//! panels by their own number. [`keys`] turns a panel's registrations, written by the notices' own
//! numbers, into those names.
//!
//! Every notice a panel registers for here is the player's or no object's: the panels that hear
//! other objects' notices read them from the frame's view.

/// The handlers registered for each notice, in the order they registered.
#[derive(Clone, Debug)]
pub struct Registry<H> {
    lists: std::collections::BTreeMap<u32, Vec<H>>,
}

impl<H> Default for Registry<H> {
    fn default() -> Self {
        Self {
            lists: std::collections::BTreeMap::new(),
        }
    }
}

impl<H: Copy + PartialEq> Registry<H> {
    /// A registry with no handlers.
    // D2-03965, D2-03950, D2-00149, D2-00150
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lists: std::collections::BTreeMap::new(),
        }
    }

    /// Register `handler` for `notice`, after those registered before it; a handler registered
    /// already stays where it is.
    // D2-03967, D2-08105
    pub fn register(&mut self, notice: u32, handler: H) {
        let list = self.lists.entry(notice).or_default();
        if !list.contains(&handler) {
            list.push(handler);
        }
    }

    /// Register `handler` for each of `notices`, in order.
    pub fn register_all(&mut self, notices: &[u32], handler: H) {
        for &n in notices {
            self.register(n, handler);
        }
    }

    /// `handler` hears nothing more: it goes.
    // D2-03972, D2-08108, D2-03973, D2-01905, D2-03971, D2-03968
    pub fn unregister_all(&mut self, handler: H) {
        for list in self.lists.values_mut() {
            list.retain(|h| *h != handler);
        }
    }

    /// The handlers of `notice`, in the order they registered.
    // D2-03966, D2-08104
    #[must_use]
    pub fn handlers(&self, notice: u32) -> &[H] {
        self.lists.get(&notice).map_or(&[], Vec::as_slice)
    }

    /// Whether `handler` is registered for `notice`.
    #[must_use]
    pub fn hears(&self, notice: u32, handler: H) -> bool {
        self.handlers(notice).contains(&handler)
    }

    /// Every handler goes.
    // D2-03970, D2-08107
    pub fn clear(&mut self) {
        self.lists.clear();
    }
}

/// The notices the player's scripts send, by their own number, with the native that sends each.
const SENT_BY_SCRIPTS: &[(u32, u32)] = &[
    (0x403, 0x0085 << 16 | 0x0001),
    (0x405, 0x008B << 16 | 0x0001),
    (0x406, 0x008B << 16 | 0x0002),
    (0x407, 0x0086 << 16 | 0x0001),
    (0x408, 0x0085 << 16 | 0x0003),
    (0x409, 0x008B << 16 | 0x0003),
    (0x40D, 0x0085 << 16 | 0x0005),
    (0x40F, 0x008F << 16 | 0x0001),
    (0x410, 0x0085 << 16 | 0x0002),
    (0x411, 0x0085 << 16 | 0x0008),
    (0x413, 0x0086 << 16 | 0x0004),
    (0x415, 0x0085 << 16 | 0x0006),
    (0x416, 0x0085 << 16 | 0x0007),
    (0x417, 0x0085 << 16 | 0x0009),
    (0x418, 0x008E << 16 | 0x0001),
    (0x419, 0x0087 << 16 | 0x0001),
    (0x41B, 0x008E << 16 | 0x0002),
    (0x41C, 0x008E << 16 | 0x0003),
    (0x41E, 0x008A << 16 | 0x0001),
    (0x41F, 0x008A << 16 | 0x0002),
    (0x420, 0x008A << 16 | 0x0003),
    (0x421, 0x008A << 16 | 0x0004),
    (0x422, 0x008A << 16 | 0x0005),
    (0x423, 0x008A << 16 | 0x0006),
    (0x424, 0x008A << 16 | 0x0007),
    (0x425, 0x008A << 16 | 0x0008),
    (0x426, 0x008A << 16 | 0x0009),
    (0x427, 0x008A << 16 | 0x000A),
    (0x428, 0x008A << 16 | 0x000B),
    (0x429, 0x008A << 16 | 0x000C),
    (0x42A, 0x0087 << 16 | 0x0002),
    (0x42B, 0x0092 << 16 | 0x0001),
    (0x431, 0x0084 << 16 | 0x0001),
    (0x432, 0x0084 << 16 | 0x0002),
    (0x434, 0x0084 << 16 | 0x0003),
    (0x435, 0x0084 << 16 | 0x0004),
    (0x437, 0x0090 << 16 | 0x0001),
    (0x438, 0x0090 << 16 | 0x0002),
    (0x439, 0x0090 << 16 | 0x0003),
    (0x43A, 0x0090 << 16 | 0x0004),
    (0x43B, 0x0090 << 16 | 0x0005),
    (0x43C, 0x0090 << 16 | 0x0006),
    (0x43D, 0x0090 << 16 | 0x0007),
    (0x43E, 0x0090 << 16 | 0x0008),
    (0x43F, 0x0090 << 16 | 0x0009),
    (0x441, 0x0089 << 16 | 0x0001),
    (0x443, 0x0085 << 16 | 0x000A),
    (0x446, 0x008D << 16 | 0x0001),
    (0x447, 0x0088 << 16 | 0x0001),
    (0x448, 0x0088 << 16 | 0x0005),
    (0x449, 0x0091 << 16 | 0x0001),
    (0x44A, 0x0088 << 16 | 0x0002),
    (0x44B, 0x0088 << 16 | 0x0003),
    (0x44D, 0x0085 << 16 | 0x000B),
    (0x451, 0x0085 << 16 | 0x000C),
    (0x453, 0x0085 << 16 | 0x0004),
    (0x454, 0x008B << 16 | 0x0004),
    (0x456, 0x008C << 16 | 0x0001),
    (0x457, 0x008C << 16 | 0x0002),
    (0x45D, 0x0089 << 16 | 0x0002),
    (0x45E, 0x0088 << 16 | 0x0004),
    (0x460, 0x0085 << 16 | 0x000D),
    (0x461, 0x0085 << 16 | 0x000E),
    (0x465, 0x0086 << 16 | 0x0002),
    (0x466, 0x0086 << 16 | 0x0003),
    (0x468, 0x0087 << 16 | 0x0004),
    (0x46E, 0x0085 << 16 | 0x0011),
    (0x472, 0x1AD2_0001),
    (0x473, 0x1AD2_0002),
    (0x474, 0x1AD2_0003),
    (0x475, 0x1AD2_0004),
    (0x476, 0x1AD2_0005),
    (0x477, 0x0085 << 16 | 0x0012),
    (0x478, 0x0085 << 16 | 0x0010),
    (0x479, 0x0092 << 16 | 0x0002),
    (0x47A, 0x0088 << 16 | 0x0006),
    (0x47B, 0x0085 << 16 | 0x0013),
    (0x47C, 0x0086 << 16 | 0x0005),
    (0x482, 0x1AD3_0004),
    (0x483, 0x1AD3_0005),
    (0x484, 0x1AD3_0006),
    (0x485, 0x1AD3_0007),
    (0x486, 0x1AD3_0003),
    (0x48C, 0x008A << 16 | 0x000D),
    (0x48E, 0x1AD3_0001),
    (0x48F, 0x1AD3_0002),
    (0x490, 0x1AD3_0008),
    (0x491, 0x1AD3_0009),
    (0x492, 0x1AD3_000A),
    (0x496, 0x1AD3_000B),
    (0x497, 0x008F << 16 | 0x0002),
    (0x498, 0x0091 << 16 | 0x0003),
    (0x499, 0x0085 << 16 | 0x0014),
    (0x49A, 0x0085 << 16 | 0x0015),
    (0x49B, 0x0085 << 16 | 0x0016),
    (0x49D, 0x008B << 16 | 0x0005),
    (0x49E, 0x0085 << 16 | 0x0017),
    (0x49F, 0x0085 << 16 | 0x0018),
    (0x4A0, 0x0085 << 16 | 0x0019),
    (0x4A1, 0x0085 << 16 | 0x001A),
    (0x4A2, 0x0085 << 16 | 0x001B),
    (0x4A3, 0x0085 << 16 | 0x000F),
    (0x4A4, 0x0092 << 16 | 0x0003),
    (0x4A5, 0x0085 << 16 | 0x001C),
    (0x4A6, 0x0085 << 16 | 0x001D),
];

/// The notice the scripts open the examination window with, which the client's panels send too
/// by its own number.
const OPEN_EXAMINATION_PANEL: u32 = 0x45F;

/// Registrations written by the notices' own numbers, as the panels name them: a notice the
/// scripts send by its native (the examination window's opening by both its names), the client's
/// own by its number; each once, in order.
#[must_use]
pub fn keys(numbers: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(numbers.len() + 1);
    let mut put = |k: u32| {
        if !out.contains(&k) {
            out.push(k);
        }
    };
    for &n in numbers {
        if n == OPEN_EXAMINATION_PANEL {
            put(0x0091 << 16 | 0x0002);
            put(n);
            continue;
        }
        match SENT_BY_SCRIPTS.iter().find(|(own, _)| *own == n) {
            Some(&(_, native)) => put(native),
            None => put(n),
        }
    }
    out
}

/// The notices of several panels, each once, in order: those of the first, then the second's
/// new ones, and so on.
#[must_use]
pub fn union(lists: impl IntoIterator<Item = Vec<u32>>) -> Vec<u32> {
    let mut out = Vec::new();
    for l in lists {
        for k in l {
            if !out.contains(&k) {
                out.push(k);
            }
        }
    }
    out
}
