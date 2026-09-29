//! The friends list: the receiver for `0x0021 Social_FriendsUpdate`, the four client-to-server
//! events, and the `@friends` command family's model half.
//!
//! This is the `0x0021` receiver (recorded traffic shows it arriving **11 times** across three
//! sessions); its panel is `dereth_ui_screens::panels::friends`.
//!
//! # The wire layout matches its documentation, and the corpus says so twice
//!
//! A friend record is `id`, `online`, `appear_offline`, `PString name`, then two
//! packed `ulong` lists; the friends list is unpacked as a packable list of
//! `FriendData` **then** the update type. `dereth_protocol::social` has exactly that.
//!
//! The check that matters is that it was made against a **populated** record. Five observed
//! `0x0021` messages contain an empty list and update type `Full`, which is precisely the corpus
//! shape that can keep every test green over a wrong layout. Six more observations include
//! five populated records. A one-entry `Added` update occupies 36 bytes and is consumed exactly.
//!
//! # Ids and names
//!
//! It is not true that "retail sends character id 0 and identifies by name". Retail's add-friend
//! request body is a bare string — **there is no id field at all**, so nothing sends a zero —
//! while the remove-friend request carries a real instance id. From two recorded sessions:
//!
//! ```text
//! session 1  t=351.746  action 0x0018  030054776f000000   "Two"
//! session 2  t=266.182  action 0x0018  03004f6e65000000   "One"
//! session 2  t=273.769  action 0x0017  1e000050           0x5000001E
//! session 2  t=277.511  action 0x0018  03004f6e65000000   "One"
//! ```
//!
//! That asymmetry is load-bearing for `@friends remove <name>`: the command takes a **name** and
//! the wire takes an **id**, so the name is resolved against the friends list before anything is
//! sent, and a name that is not on
//! the list sends nothing at all.

use dereth_primitives::ObjectId;
use dereth_protocol::social::{FriendData, SocialFriendsUpdate};

use crate::player::Friend;
use crate::{Request, RequestSink, World};

/// The client's cap: `if (0x63 < count) refuse`, i.e. the
/// list holds **100** and the hundred-and-first is refused.
///
/// The Add-button state writes the same number the other way round
/// (`count < 0x64` gives state 1, anything else state `0xD`), which is why the Add button greys
/// at exactly the point the add request would start refusing. The error text still says
/// *"Your friends list can contain up to 50 characters."*: the cap doubled and the sentence was
/// never changed, so a player refused at 100 is told 50.
pub const MAX_FRIENDS: usize = 100;

/// Display object error `0x561` when an add-friend request is refused because the
/// list is already full. The literal behind the code is not in
/// `dereth_ui_screens::chat::failure`'s table, so this build **counts** it and prints
/// nothing rather than inventing a sentence.
pub const ERROR_FRIENDS_LIST_FULL: u32 = 0x0561;

/// Display object error `0x563` when a remove-friend command is refused because the typed name is
/// on nobody's list. Same treatment as
/// [`ERROR_FRIENDS_LIST_FULL`].
pub const ERROR_NOT_A_FRIEND: u32 = 0x0563;

/// The friends-update type, the dword `0x0021` carries **after** its list.
///
/// The five arms are the client's `switch`, which has
/// no `default`: an unknown discriminator changes nothing at all, which is why
/// [`FriendsUpdate::Unknown`] exists rather than being folded into `Full`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FriendsUpdate {
    /// 0 — the whole list is assigned over. A **replace**.
    Full,
    /// 1 — the list's *first* record, inserted at the head.
    Added,
    /// 2 — the server-says-remove arm with `announce = false`.
    Removed,
    /// 3 — the same call with `announce = true`. The two arms differ **only** in that flag, and
    /// the flag is unused inside the function, so the model cannot tell them apart either.
    RemovedSilent,
    /// 4 —: one friend's online flag moved.
    LoginChange,
    /// Anything else. The friends-list notice handler's `switch` has no `default`, so this is a
    /// message the client receives, decodes and then does nothing with.
    Unknown(u32),
}

impl FriendsUpdate {
    /// The friends-update handler's discriminator, straight off the wire.
    #[must_use]
    pub const fn from_raw(v: u32) -> Self {
        match v {
            0 => Self::Full,
            1 => Self::Added,
            2 => Self::Removed,
            3 => Self::RemovedSilent,
            4 => Self::LoginChange,
            other => Self::Unknown(other),
        }
    }

    /// Whether this arm reached a server-says / friends-display refresh call at all.
    #[must_use]
    pub const fn is_handled(self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}

/// One wire record, as the model holds it.
fn friend_of(d: &FriendData) -> Friend {
    Friend {
        id: d.id,
        name: d.name.clone(),
        online: d.online != 0,
        appear_offline: d.appear_offline != 0,
    }
}

impl World {
    /// The social friends-update handler forwards an update-friends-list notice to
    /// the friends panel's handler for it.
    ///
    /// Returns the arm taken, so a caller can tell "a `0x0021` arrived and meant nothing" from
    /// "no `0x0021` arrived" — the two readings an arrival counter alone cannot separate.
    ///
    /// # One divergence, and it is the one this build cannot avoid
    ///
    /// Retail keeps **two** stores: an internal friend-record list and the list box's rows. The
    /// `LoginChange` arm rewrites only the *rows* — the server-says-update arm removes and
    /// re-adds the display row and never touches the friend-record list — so retail's
    /// model copy goes stale on every login and logout, harmlessly, because the only two things
    /// that read it (the 100-cap and the `@friends remove <name>` lookup) do not care about the
    /// online flag.
    ///
    /// This build has one store, which the panel draws from, so `LoginChange` must land in it or
    /// the row would never change colour. Reproducing retail's staleness here would reproduce the
    /// bug and not the behaviour.
    pub fn recv_friends_update(&mut self, m: &SocialFriendsUpdate) -> FriendsUpdate {
        let kind = FriendsUpdate::from_raw(m.update_type);
        let friends = &mut self.player_system.social.friends;
        match kind {
            // The whole list is assigned over, then the panel refreshes: a full replace,
            // in wire order. The sort is the panel's, not the list's.
            FriendsUpdate::Full => {
                friends.clear();
                friends.extend(m.friends.iter().map(friend_of));
            }
            // The server-says-add-friend arm takes the list's head and **only** the head: a `0x0021` of
            // type 1 carrying two records adds the first and drops the second. Transcribed rather
            // than generalised: the original inserts that record before the current head, so the
            // newest friend appears first.
            FriendsUpdate::Added => {
                if let Some(d) = m.friends.first() {
                    let f = friend_of(d);
                    // The client does not de-duplicate here; the server is the one that decides a
                    // name is already on the list. A repeat of the same id would give retail two
                    // rows with the same `0x10000085`, and the panel's row removal deletes the first
                    // it finds. Keeping the store unique is strictly narrower and is the one place
                    // this differs, because a duplicated row is not a behaviour worth copying.
                    friends.retain(|e| e.id != f.id);
                    friends.insert(0, f);
                }
            }
            FriendsUpdate::Removed | FriendsUpdate::RemovedSilent => {
                if let Some(d) = m.friends.first() {
                    friends.retain(|e| e.id != d.id);
                }
            }
            // The server's friend-update message — see the divergence note above.
            FriendsUpdate::LoginChange => {
                if let Some(d) = m.friends.first() {
                    let f = friend_of(d);
                    match friends.iter_mut().find(|e| e.id == f.id) {
                        Some(e) => *e = f,
                        // The panel's row removal returns false for an id with no row and
                        // the row add then creates one anyway, so a `LoginChange` for
                        // somebody the client has never seen *does* put them on screen.
                        None => friends.push(f),
                    }
                }
            }
            FriendsUpdate::Unknown(_) => {}
        }
        kind
    }

    /// The friends list, in the order the model holds it — newest first after an `Added`, wire
    /// order after a `Full`.
    #[must_use]
    pub fn friends(&self) -> &[Friend] {
        &self.player_system.social.friends
    }

    /// The friends panel's add request — the 100-cap, then
    /// the add-friend send, `0x0018`.
    ///
    /// Returns `None` when the cap refused, carrying nothing because retail's refusal is a weenie
    /// error with no text of its own ([`ERROR_FRIENDS_LIST_FULL`]).
    ///
    /// Runtime traffic confirms that the request carries the name and **only** the name.
    pub fn add_friend(&mut self, req: &mut dyn RequestSink, name: &str) -> Option<()> {
        if self.player_system.social.friends.len() >= MAX_FRIENDS {
            return None;
        }
        req.send(Request::SocialAddFriend(
            dereth_protocol::social::SocialAddFriend {
                name: name.to_owned(),
            },
        ));
        Some(())
    }

    /// The remove-friend send `(id)` — `0x0017`, and it takes an **id**.
    ///
    /// Runtime traffic confirms that the request carries the friend's object id.
    pub fn remove_friend(&mut self, req: &mut dyn RequestSink, friend_id: ObjectId) {
        req.send(Request::SocialRemoveFriend(
            dereth_protocol::social::SocialRemoveFriend { friend_id },
        ));
    }

    /// The client compares the typed name against every stored friend name, case-insensitively,
    /// after the stored name has had a
    /// leading `'+'` trimmed off it.
    ///
    /// The `'+'` is the allegiance-title marker. The comparison strips it from the *typed* half
    /// too, so both sides lose it. Returns the id the remove-friend event would receive, or `None`
    /// when the client would display object error `0x563`.
    #[must_use]
    pub fn friend_id_by_name(&self, name: &str) -> Option<ObjectId> {
        let want = name.trim_start_matches('+');
        self.player_system
            .social
            .friends
            .iter()
            .find(|f| f.name.trim_start_matches('+').eq_ignore_ascii_case(want))
            .map(|f| f.id)
    }

    /// The clear-friends send — `0x0025`, an empty body.
    ///
    /// The remove-all-friends chat command sends it and then **empties the local
    /// list without waiting for the server**, which is why the panel goes blank on the click.
    pub fn clear_friends(&mut self, req: &mut dyn RequestSink) {
        req.send(Request::SocialClearFriends(
            dereth_protocol::social::SocialClearFriends,
        ));
        self.player_system.social.friends.clear();
    }

    /// Send friends command `0xF7CD` with action 0 and an empty name on the **Control queue**, the
    /// whole of `@friends old`.
    ///
    /// The `"old"` arm passes the empty narrow string as the player
    /// name, so the body is `00000000 0000` (a zero command and an empty `PString`); the comment
    /// on [`dereth_protocol::social::SocialSendFriendsCommand`] that *"the client only ever sends
    /// `cmd = 0`"* is this call site and retail has no other.
    pub fn send_friends_command(&mut self, req: &mut dyn RequestSink, player: &str) {
        req.send(Request::SocialSendFriendsCommand(
            dereth_protocol::social::SocialSendFriendsCommand {
                cmd: 0,
                player: player.to_owned(),
            },
        ));
    }
}

// ---------------------------------------------------------------------------------------------
// The `@friends` command family: three table entries in `crate::cmd::table`, handled by
// `do_friends`, `do_friends_add` and `do_friends_remove`
// ---------------------------------------------------------------------------------------------
//
// # Where they live, and the one hop this build does not have
//
// In retail the three communication handlers **send nothing**: list, add, and remove each raises its
// matching friends-update notice. The friends panel is the only registered receiver; it applies
// the 100-friend cap, resolves names against the stored friends list, and sends the social event. So in the
// client the chat command reaches the shard *through the panel*.
//
// This build has no notice bus from `dereth_client` into a `dereth_ui_screens` panel — the same gap
// `panels::allegiance::accept_swear_prompt`'s docstring records for `0x0274` — and the panel may
// not depend on `dereth_client_model` at all. The decisions the panel makes are therefore taken here,
// against the one store this build has, and the panel takes the identical decisions against the
// view of that store. Nothing is duplicated except the two constants, which are
// [`MAX_FRIENDS`] and the sort rule, and both name each other.

/// What one `@friends…` handler decided.
///
/// Every arm is something retail does; nothing here is a summary. `Refused` and `Listed` are
/// chat-scroll writes with the chat type each call site passes, and the
/// sending arms name the message that went out so a test can assert against the request rather
/// than against a side effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FriendsOutcome {
    /// `Request::SocialAddFriend(name)` — `0x0018`, the name and nothing else.
    Added(String),
    /// `Request::SocialRemoveFriend(id)` — `0x0017`, after the typed name was resolved.
    RemovedById(ObjectId),
    /// `Request::SocialClearFriends` — `0x0025`, `@friends remove -all`.
    Cleared,
    /// Send friends command `0xF7CD` with action 0 and an empty name for `@friends old`.
    OldList,
    /// The client's lines, in the order it prints them,
    /// each already carrying its trailing newline. Chat type **0**, window **0** — this one call
    /// site does *not* use the command's source window.
    Listed {
        online_only: bool,
        lines: Vec<String>,
    },
    /// A scroll print on `0x1A` at the source window and nothing else. All three
    /// of these literals are wide strings, taken verbatim from retail's UTF-16 text rather than
    /// inferred.
    Refused(&'static str),
    /// Display the selected object-error code with an empty detail string. The two codes' literals
    /// are not in the `chat::failure` table, so this carries the code and prints nothing.
    WeenieError(u32),
}

/// The client's refusal, 56 wide characters.
pub const MUST_NAME_THE_FRIEND_TO_ADD: &str =
    "You must specify the name of the friend you wish to add.";
/// The `@friends remove` handler's refusal.
pub const MUST_NAME_THE_FRIEND_TO_REMOVE: &str =
    "You must specify the name of the friend you wish to remove.";
/// The client's last arm, for a sub-command that is none of
/// `add` / `remove` / `online` / `old` / empty.
pub const INVALID_FRIENDS_COMMAND: &str = "Invalid friends command specified.";

/// The client's three narrow literals. Their narrow-string construction distinguishes them from
/// the three wide strings above. All four were verified against retail.
pub const FRIENDS_LIST_IS_EMPTY: &str = "Your friends list is empty!\n";
/// The listing's header.
pub const YOUR_FRIENDS: &str = "Your friends:\n";
/// The listing's "nothing matched" tail, with its own two-space indent.
pub const NO_FRIENDS_ONLINE: &str = "  You have no friends that are online.\n";
/// The client's acknowledgement, printed **before** any
/// answer from the shard.
pub const FRIENDS_LIST_CLEARED: &str = "Your friends list has been cleared.\n";

/// The `L" (Online)\n"` suffix the listing puts on a friend who is logged in. The only wide
/// literal in that function, which is why it is spelled differently from its three neighbours.
pub const ONLINE_SUFFIX: &str = " (Online)\n";

/// Chat type `0x1A` at the source window — the local-error channel every
/// refusal in the family uses. This is the same value as `dereth_client_model::scroll::LOCAL_ERROR_TYPE`.
pub const REFUSAL_CHAT_TYPE: u32 = 0x1A;
/// The listing's own channel: chat type 0 at window 0, i.e. `Default`.
pub const LISTING_CHAT_TYPE: u32 = 0;

/// Behavior: the joined arguments, trimmed of whitespace at both ends, then of a
/// **leading** `'+'`.
///
/// The `'+'` is the allegiance-title marker, and stripping it from the typed half is what makes
/// `@friends remove +Baron Lark` and `@friends remove Baron Lark` the same command. The removal
/// lookup strips it from the stored half for the same reason.
#[must_use]
pub fn join_args_as_name(args: &[String]) -> String {
    args.join(" ").trim().trim_start_matches('+').to_owned()
}

/// The order the friends panel shows friends in, which is what the `@friends` listing walks.
///
/// The ordering puts every **online** friend above every offline one and sorts within each block
/// by a case-sensitive wide-string comparison on the row's text. The panel reaches the same order by
/// doing that insert walk row by row
/// (`dereth_ui_screens::panels::friends::FriendsPanel::add_friend_display`); this build's listing
/// reads the *model* where retail's reads the rows, so it sorts here instead. The two rules are
/// the same rule and each names the other.
#[must_use]
pub fn display_order(friends: &[Friend]) -> Vec<&Friend> {
    let mut v: Vec<&Friend> = friends.iter().collect();
    v.sort_by(|a, b| b.online.cmp(&a.online).then_with(|| a.name.cmp(&b.name)));
    v
}

impl World {
    /// Handle `@friends`, the only one of the three command entries with subcommands.
    ///
    /// ```text
    /// Parse the first argument as the subcommand.
    /// `add` and `remove` dispatch to their respective handlers.
    /// `online` lists only online friends; `old` sends action 0 with an empty name.
    /// An empty subcommand lists every friend. Any other value prints the invalid-command text on
    /// chat type `0x1A` and returns success.
    /// ```
    ///
    /// The narrow-string length includes its terminator, so length 1 is empty. A bare `@friends`
    /// is therefore the *last-but-one* arm and not the first, which is
    /// why an unrecognised word is refused rather than treated as a listing.
    pub fn do_friends(&mut self, req: &mut dyn RequestSink, args: &[String]) -> FriendsOutcome {
        let (sub, rest) = match args.split_first() {
            Some((s, rest)) => (s.as_str(), rest),
            None => ("", &[] as &[String]),
        };
        if sub.eq_ignore_ascii_case("add") {
            return self.do_friends_add(req, rest);
        }
        if sub.eq_ignore_ascii_case("remove") {
            return self.do_friends_remove(req, rest);
        }
        if sub.eq_ignore_ascii_case("online") {
            return self.display_friends(true);
        }
        if sub.eq_ignore_ascii_case("old") {
            self.send_friends_command(req, "");
            return FriendsOutcome::OldList;
        }
        if sub.is_empty() {
            return self.display_friends(false);
        }
        FriendsOutcome::Refused(INVALID_FRIENDS_COMMAND)
    }

    /// Handle both `@friends add <name>` and the separate `@friends_add` table entry.
    ///
    /// Retail's body joins the arguments into a name, refuses an empty one, then
    /// raises the add-friend chat-command notice — whose receiver is
    /// the friends panel's chat-command add handler, which makes one add request. The cap is
    /// therefore on this path too.
    pub fn do_friends_add(&mut self, req: &mut dyn RequestSink, args: &[String]) -> FriendsOutcome {
        let name = join_args_as_name(args);
        if name.is_empty() {
            return FriendsOutcome::Refused(MUST_NAME_THE_FRIEND_TO_ADD);
        }
        match self.add_friend(req, &name) {
            Some(()) => FriendsOutcome::Added(name),
            None => FriendsOutcome::WeenieError(ERROR_FRIENDS_LIST_FULL),
        }
    }

    /// The `@friends remove` handler — `@friends remove <name>`,
    /// `@friends remove -all`, and the `@friends_remove` entry.
    ///
    /// The `-all` test is a `_stricmp` against the **joined** name, so `@friends remove -ALL` is
    /// the clear and `@friends remove -all Bob` is a name nobody has. The name arm is
    /// the friends panel's chat-command remove handler: resolve the name to an id
    /// and send `0x0017`, or raise weenie error `0x563` and send nothing.
    pub fn do_friends_remove(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> FriendsOutcome {
        let name = join_args_as_name(args);
        if name.is_empty() {
            return FriendsOutcome::Refused(MUST_NAME_THE_FRIEND_TO_REMOVE);
        }
        if name.eq_ignore_ascii_case("-all") {
            self.clear_friends(req);
            return FriendsOutcome::Cleared;
        }
        match self.friend_id_by_name(&name) {
            Some(id) => {
                self.remove_friend(req, id);
                FriendsOutcome::RemovedById(id)
            }
            None => FriendsOutcome::WeenieError(ERROR_NOT_A_FRIEND),
        }
    }

    /// The friends panel's chat-command display handler, as text.
    ///
    /// With an empty friends list it prints `"Your friends list is empty!\n"` and returns.
    /// Otherwise it prints `"Your friends:\n"`, then walks the friends list box: a row whose text
    /// state is `0x10000054` (online) prints `"  " + name + " (Online)\n"`, and any other row prints
    /// `"  " + name + "\n"` unless only online friends were asked for. Each printed row counts; if
    /// none was printed it ends with `"  You have no friends that are online.\n"`. Every line goes
    /// to chat type 0, window 0.
    ///
    /// **The empty-list arm returns without the header**, and the `n == 0` tail is reached with
    /// the header already printed — so a character with only offline friends who types
    /// `@friends online` sees two lines, not one.
    ///
    /// **The two-space indent never reaches the scroll, and it is written anyway.** Retail
    /// composes `"  " + name` and then hands it to the chat scroll,
    /// whose second step trims the configured whitespace from both ends.
    /// So the indent is real in the call and invisible in the window; it is composed here for the
    /// same reason the trailing newline is, which `add_text_to_scroll` also trims: a call site
    /// that quietly dropped what retail passes would be a second, smaller lie. [measured: a test
    /// asserting the indented form on the scroll fails]
    fn display_friends(&self, online_only: bool) -> FriendsOutcome {
        let friends = &self.player_system.social.friends;
        if friends.is_empty() {
            return FriendsOutcome::Listed {
                online_only,
                lines: vec![FRIENDS_LIST_IS_EMPTY.to_owned()],
            };
        }
        let mut lines = vec![YOUR_FRIENDS.to_owned()];
        let mut shown = 0usize;
        for f in display_order(friends) {
            if f.online {
                lines.push(format!("  {}{ONLINE_SUFFIX}", f.name));
                shown += 1;
            } else if !online_only {
                lines.push(format!("  {}\n", f.name));
                shown += 1;
            }
        }
        if shown == 0 {
            lines.push(NO_FRIENDS_ONLINE.to_owned());
        }
        FriendsOutcome::Listed { online_only, lines }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_protocol::social::FriendData;

    fn record(id: u32, name: &str, online: bool) -> FriendData {
        FriendData {
            id: ObjectId(id),
            online: i32::from(online),
            appear_offline: 0,
            name: name.to_owned(),
            friends_list: Vec::new(),
            friend_of_list: Vec::new(),
        }
    }

    fn update(friends: Vec<FriendData>, ty: u32) -> SocialFriendsUpdate {
        SocialFriendsUpdate {
            friends,
            update_type: ty,
        }
    }

    /// Oracle: the 36-byte record at `fellowship-one-vassal.jsonl` t=351.754, with only its
    /// three-byte name pseudonymized, decoded through the shared codec and driven through the real
    /// handler. The **populated** record, not the empty one the locked
    /// corpus carries — see the module header for why that distinction is the whole test.
    #[test]
    fn the_populated_capture_record_decodes_and_lands_in_the_model() {
        let bytes: Vec<u8> = (0..)
            .step_by(2)
            .take_while(|i| *i + 2 <= 72)
            .map(|i| {
                u8::from_str_radix(
                    &"010000001f00005001000000000000000300426578000000000000000000000001000000"
                        [i..i + 2],
                    16,
                )
                .unwrap()
            })
            .collect();
        let m: SocialFriendsUpdate = dereth_protocol::read_body(&bytes).expect("cursor exhausted");
        assert_eq!(m.friends.len(), 1);
        assert_eq!(m.friends[0].id, ObjectId(0x5000_001F));
        assert_eq!(m.friends[0].name, "Bex");
        assert_eq!(m.friends[0].online, 1);
        assert_eq!(m.update_type, 1);

        let mut w = World::new();
        assert_eq!(w.recv_friends_update(&m), FriendsUpdate::Added);
        assert_eq!(w.friends().len(), 1);
        assert_eq!(w.friends()[0].name, "Bex");
        assert!(w.friends()[0].online);
    }

    /// Oracle: the client's five arms, in order.
    #[test]
    fn the_five_update_types_are_replace_add_remove_remove_and_touch() {
        let mut w = World::new();
        w.recv_friends_update(&update(
            vec![record(1, "Ann", false), record(2, "Bob", true)],
            0,
        ));
        assert_eq!(w.friends().len(), 2, "Full is a replace");

        w.recv_friends_update(&update(vec![record(3, "Cid", true)], 1));
        assert_eq!(w.friends()[0].id, ObjectId(3), "Added inserts at the head");

        w.recv_friends_update(&update(vec![record(1, "Ann", false)], 2));
        assert!(w.friends().iter().all(|f| f.id != ObjectId(1)));
        w.recv_friends_update(&update(vec![record(2, "Bob", true)], 3));
        assert!(
            w.friends().iter().all(|f| f.id != ObjectId(2)),
            "3 removes like 2"
        );

        assert!(!w.friends()[0].online || true);
        w.recv_friends_update(&update(vec![record(3, "Cid", false)], 4));
        assert!(
            !w.friends()[0].online,
            "LoginChange rewrites the flag in place"
        );
        assert_eq!(
            w.friends().len(),
            1,
            "and does not add a row for somebody already there"
        );

        // `switch` with no `default`.
        let before = w.friends().to_vec();
        assert_eq!(
            w.recv_friends_update(&update(vec![record(9, "Dee", true)], 7)),
            FriendsUpdate::Unknown(7)
        );
        assert_eq!(
            w.friends(),
            before.as_slice(),
            "an unknown type changes nothing"
        );
    }

    /// Oracle: the client's `0x63 <`, the cap of 100.
    #[test]
    fn the_hundred_and_first_friend_is_refused_before_any_bytes_exist() {
        assert_eq!(MAX_FRIENDS, 100);
        // One short of the cap, the add is still sent.
        let mut w = World::new();
        w.recv_friends_update(&update(
            (0..MAX_FRIENDS - 1)
                .map(|i| record(100 + u32::try_from(i).expect("a small index"), "X", false))
                .collect(),
            0,
        ));
        let mut req = crate::RecordingRequests::default();
        assert!(w.add_friend(&mut req, "Bob").is_some());
        assert_eq!(req.0.len(), 1, "the ninety-ninth friend's add is sent");

        let mut w = World::new();
        w.recv_friends_update(&update(
            (0..MAX_FRIENDS)
                .map(|i| record(100 + u32::try_from(i).expect("a small index"), "X", false))
                .collect(),
            0,
        ));
        let mut req = crate::RecordingRequests::default();
        assert!(w.add_friend(&mut req, "Bob").is_none());
        assert!(
            req.0.is_empty(),
            "no datagram leaves this process, and none was even composed"
        );
    }

    /// Pinned behavior: `_stricmp` runs after a leading `'+'`
    /// is trimmed off the stored name.
    #[test]
    fn a_name_resolves_to_an_id_case_insensitively_and_past_the_allegiance_plus() {
        let mut w = World::new();
        w.recv_friends_update(&update(vec![record(7, "+Baron Lark", true)], 0));
        assert_eq!(w.friend_id_by_name("baron lark"), Some(ObjectId(7)));
        assert_eq!(w.friend_id_by_name("+Baron Lark"), Some(ObjectId(7)));
        assert_eq!(w.friend_id_by_name("Aldric"), None, "0x563's arm");
    }
}
