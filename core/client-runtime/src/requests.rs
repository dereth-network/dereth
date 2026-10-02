//! The one outbound seam: a [`dereth_client_model::Request`] becomes bytes on a
//! [`dereth_client_net::client_session::Session`]. It lives here rather than in
//! `dereth_client::interaction` because `objects.rs` needs it too;
//! `dereth_client::interaction::send_request` is a `pub use` of this function.

use dereth_client_model::Request;

/// The one place a [`dereth_client_model::Request`] becomes bytes.
///
/// Game-action arms use [`dereth_client_net::client_session::Session::send_action`], allocating the `OrderedActionHeader` stamp
/// from the **single global counter**. ForceObjdesc and TurbineChat use bare senders instead.
/// A failure here is
/// an encode failure, which `outbound::build` has already rolled the counter back for; a failure
/// *on the wire* is reported by the transport through `Session::report_send_failed`, and this
/// function deliberately does not second-guess it.
///
/// The match is exhaustive over the variants `dereth_client_model` emits rather than a blanket generic,
/// because `send_action` needs the concrete `Message` type — which is the same reason
/// `dereth_client_model::Request` exists at all (its crate note: "the senders take a `RequestSink` and emit
/// typed `Request`s; the session frames and stamps them").
pub fn send_request<T: dereth_primitives::Transport>(
    session: &mut dereth_client_net::client_session::Session<T>,
    r: &Request,
) -> bool {
    use Request as R;
    let s = session;
    let out = match r {
        R::TurbineChat(m) => return s.send_turbine_chat(m).is_ok(),
        // The admin server-version request allocates four
        // bytes, writes literal F7CC, and hands them to the control queue. There is no action stamp.
        R::AdminGetServerVersion(m) => {
            s.send_admin_get_server_version(m);
            return true;
        }
        R::GetAndWieldItem(m) => s.send_action(m),
        R::StackableSplitToWield(m) => s.send_action(m),
        R::PutItemInContainer(m) => s.send_action(m),
        R::DropItem(m) => s.send_action(m),
        R::StackableMerge(m) => s.send_action(m),
        R::StackableSplitToContainer(m) => s.send_action(m),
        R::StackableSplitTo3d(m) => s.send_action(m),
        R::GiveObjectRequest(m) => s.send_action(m),
        R::Appraise(m) => s.send_action(m),
        R::UseEvent(m) => s.send_action(m),
        R::UseWithTargetEvent(m) => s.send_action(m),
        // `0x019C` / `0x019D`. Both are `NetQueue::Weenie` in
        // `dereth_protocol::opcodes`, so they are ordinary game actions on the same global `OrderedActionHeader`
        // counter as every arm above.
        R::AddShortCut(m) => s.send_action(m),
        R::RemoveShortCut(m) => s.send_action(m),
        R::CreateTinkeringTool(m) => s.send_action(m),
        R::QueryItemMana(m) => s.send_action(m),
        // `0x0275`, `NetQueue::Weenie` in `dereth_protocol::opcodes` and
        // the same global `OrderedActionHeader` counter and the same
        // failed-send UI-counter rollback as every arm above.
        R::ConfirmationResponse(m) => s.send_action(m),
        // `0x00BF`, an ordinary game action on the same global `OrderedActionHeader` counter
        // and the Weenie queue as every arm above.
        R::SetInscription(m) => s.send_action(m),
        // `0x0140`, the next UI counter and
        // the Weenie queue: the ordinary ordered action path.
        R::AbuseLog(m) => s.send_action(m),
        // All five wrappers send to the Weenie queue; note that the
        // retail literals show the add-page event is `0x00AC` and the book-data one `0x00AA`.
        R::BookAddPage(m) => s.send_action(m),
        R::BookModifyPage(m) => s.send_action(m),
        R::BookDeletePage(m) => s.send_action(m),
        R::BookPageData(m) => s.send_action(m),
        R::BookData(m) => s.send_action(m),
        // Finishing the barber interaction uses the same
        // ordered Weenie-queue path as the writing requests above.
        R::FinishBarber(m) => s.send_action(m),
        R::ChangeCombatMode(m) => s.send_action(m),
        R::TargetedMeleeAttack(m) => s.send_action(m),
        R::TargetedMissileAttack(m) => s.send_action(m),
        R::CancelAttack(m) => s.send_action(m),
        R::QueryHealth(m) => s.send_action(m),
        // `0x01E9`, `NetQueue::Weenie` in `dereth_protocol::opcodes` and an ordinary
        // ordered game action on the same global `OrderedActionHeader` counter as every arm above. Its body
        // is empty; the round trip is measured over the answer, not over anything it carries.
        R::RequestPing(m) => s.send_action(m),
        // `0x021E`, `NetQueue::Weenie` in `dereth_protocol::opcodes` and an ordinary
        // ordered game action with an empty body, exactly like `RequestPing` above. Retail sends
        // its tail jump during player initialization, not from any panel.
        R::QueryHouse(m) => s.send_action(m),
        // `0x021C`, `0x0221` and `0x0258`, all three `NetQueue::Weenie` in
        // `dereth_protocol::opcodes` and all three ordinary ordered game actions. The capture
        // `fixtures/packet-captures/house-purchase-refused.jsonl` shows the retail client sending `0x021C` exactly
        // this way at `t = 144.458`: `b1f70000 <seq> 1c020000 3af0da79 03000000 c2150080 c1150080
        // 92150080`, an `0xF7B1` ordered action on queue 3.
        R::BuyHouse(m) => s.send_action(m),
        R::RentHouse(m) => s.send_action(m),
        R::QueryLord(m) => s.send_action(m),
        // The chess window's five, `0x0269`/`0x026A`/`0x026B`/`0x026D`/`0x026E`.
        // All five are `NetQueue::Weenie` in `dereth_protocol::opcodes`, and all five chess-event
        // bodies in retail end in the identical UI-counter allocation and ordered send sequence,
        // which is the definition of an ordinary ordered game action here. No capture carries
        // one -- all eleven chess opcodes are zero in all three opcode spaces over the 13,535
        // recorded blobs -- so the shapes are the five event bodies' own stores checked against ACE's five
        // `GameActionChess*.Handle` readers.
        R::GameJoin(m) => s.send_action(m),
        R::GameQuit(m) => s.send_action(m),
        R::GameMove(m) => s.send_action(m),
        R::GameMovePass(m) => s.send_action(m),
        R::GameStalemate(m) => s.send_action(m),
        // `0x0005`, `NetQueue::Weenie` in `dereth_protocol::opcodes` and an ordinary
        // ordered game action. The three `fellowship*` captures carry eleven of them from
        // the retail client, each a 20-byte blob `b1f70000 <seq> 05000000 <option> <value>` on
        // queue 3.
        R::PlayerOptionChanged(m) => s.send_action(m),
        // `0x001F`, `NetQueue::Weenie` in `dereth_protocol::opcodes` and an ordinary
        // ordered game action on the same global `OrderedActionHeader` counter as every arm above. The three
        // `fellowship*` captures show retail sending it exactly this way: `b1f70000 <seq>
        // 1f000000 01000000`, queue 3.
        R::AllegianceUpdateRequest(m) => s.send_action(m),
        // `0x001D` and `0x001E`, both four bytes of object id, both on queue 3
        // as ordinary ordered game actions. The recorded captures carry seven `1d000000 <id>`
        // and five `1e000000 <id>`, which is the byte-level oracle for both buttons -- and for
        // the Kick button too, because closing the kick-confirmation dialog sends `0x001E`
        // and not `0x0277`.
        R::SwearAllegiance(m) => s.send_action(m),
        R::BreakAllegiance(m) => s.send_action(m),
        // The twenty-four allegiance events the `@allegiance` / `@motd`
        // command family is the only producer of. Every one is `NetQueue::Weenie` in
        // `dereth_protocol::opcodes` and an ordinary ordered game action on the same global `OrderedActionHeader`
        // counter, because every allegiance event ends in the identical five calls --
        // get the next UI counter, pack the order header, allocate the buffer,
        // send on the game-action queue, report a failed send -- which holds for all
        // twenty-four and for the three above them.
        R::AllegianceQueryName(m) => s.send_action(m),
        R::AllegianceClearName(m) => s.send_action(m),
        R::AllegianceSetName(m) => s.send_action(m),
        R::AllegianceSetOfficer(m) => s.send_action(m),
        R::AllegianceSetOfficerTitle(m) => s.send_action(m),
        R::AllegianceListOfficerTitles(m) => s.send_action(m),
        R::AllegianceClearOfficerTitles(m) => s.send_action(m),
        R::AllegianceDoLockAction(m) => s.send_action(m),
        R::AllegianceSetApprovedVassal(m) => s.send_action(m),
        R::AllegianceChatGag(m) => s.send_action(m),
        R::AllegianceDoHouseAction(m) => s.send_action(m),
        R::AllegianceSetMotd(m) => s.send_action(m),
        R::AllegianceQueryMotd(m) => s.send_action(m),
        R::AllegianceClearMotd(m) => s.send_action(m),
        R::AllegianceBreakAllegianceBoot(m) => s.send_action(m),
        R::AllegianceInfoRequest(m) => s.send_action(m),
        R::AllegianceChatBoot(m) => s.send_action(m),
        R::AllegianceAddBan(m) => s.send_action(m),
        R::AllegianceRemoveBan(m) => s.send_action(m),
        R::AllegianceListBans(m) => s.send_action(m),
        R::AllegianceRemoveOfficer(m) => s.send_action(m),
        R::AllegianceListOfficers(m) => s.send_action(m),
        R::AllegianceClearOfficers(m) => s.send_action(m),
        R::AllegianceRecallHometown(m) => s.send_action(m),
        // The seven fellowship events. Every one is `NetQueue::Weenie` in
        // `dereth_protocol::opcodes` and an ordinary ordered game action on the same global `OrderedActionHeader`
        // counter as every arm above; the three `fellowship*` captures show retail sending them
        // exactly this way, e.g. `b1f70000 <seq> a6000000 01000000` at queue 3.
        R::FellowshipUpdateRequest(m) => s.send_action(m),
        R::FellowshipCreate(m) => s.send_action(m),
        R::FellowshipQuit(m) => s.send_action(m),
        R::FellowshipDismiss(m) => s.send_action(m),
        R::FellowshipRecruit(m) => s.send_action(m),
        R::FellowshipAssignNewLeader(m) => s.send_action(m),
        R::FellowshipChangeFellowOpenness(m) => s.send_action(m),
        // The three friends-list game actions. All three are
        // `NetQueue::Weenie` in `dereth_protocol::opcodes` and ordinary ordered actions on the same
        // global `OrderedActionHeader` counter as every arm above; two independent captures show
        // both observed friends-list actions using the ordered header and
        // queue 3.
        R::SocialAddFriend(m) => s.send_action(m),
        R::SocialRemoveFriend(m) => s.send_action(m),
        R::SocialClearFriends(m) => s.send_action(m),
        // `0x0316 Social_AbandonContract`, `NetQueue::Weenie` in
        // `dereth_protocol::opcodes` and an ordinary ordered game action on the same global `OrderedActionHeader`
        // counter as every arm above. No capture carries one -- the recordings contain no
        // contract traffic in either direction -- so the shape is the opcode table's and ACE's
        // `GameActionAbandonContract.cs`, which reads a bare `u32`.
        R::SocialAbandonContract(m) => s.send_action(m),
        // `0xF7CD`, and the one that is *not* a game action: the friends control
        // command uses `NetQueue::Control` with no `OrderedActionHeader` and no stamp, which is
        // `dereth_protocol::actions::outbound_kind`'s answer and why this arm is `send_message`.
        R::SocialSendFriendsCommand(m) => {
            s.send_friends_command(m);
            return true;
        }
        R::CastUntargetedSpell(m) => s.send_action(m),
        R::CastTargetedSpell(m) => s.send_action(m),
        R::TestSpellFormula(m) => s.send_action(m),
        // The last link of "text typed into the entry box reaches the server".
        R::Talk(m) => s.send_action(m),
        // The emote event uses the same ordered action counter and failure rollback.
        R::Emote(m) => s.send_action(m),
        // The other three things a line can be, once the talk-focus menu has changed where it
        // goes.
        R::TalkDirect(m) => s.send_action(m),
        R::ChannelBroadcast(m) => s.send_action(m),
        // `0x005D`, the sole message sent by tell and retell commands. Without this arm the
        // verb is dropped whole.
        R::TalkDirectByName(m) => s.send_action(m),
        // ---- training and character messages ----------------------------------------------
        //
        // All four are ordinary game actions on the same global `OrderedActionHeader` counter as
        // every arm above. `TrainAttribute` and `TrainAttribute2nd` are reached from a click on
        // the attributes page, through `run_ui_requests`'s two arms.
        R::TrainSkill(m) => s.send_action(m),
        R::TrainSkillAdvancementClass(m) => s.send_action(m),
        R::TrainAttribute(m) => s.send_action(m),
        R::TrainAttribute2nd(m) => s.send_action(m),
        R::SpellbookFilterEvent(m) => s.send_action(m),
        // `0x002C`, the Titles tab's only outbound message, and the same shape again.
        R::SetDisplayCharacterTitle(m) => s.send_action(m),
        // The three spell-panel messages, `0x01E3` and `0x01E4` among them. Without them a spell
        // could be dropped on a tab and nothing would persist.
        R::AddSpellFavorite(m) => s.send_action(m),
        R::RemoveSpellFavorite(m) => s.send_action(m),
        R::RemoveSpell(m) => s.send_action(m),
        // ---- trade opening and container closing ----------------------------------------
        //
        // `0x01F6` is the only thing a double-click on another player puts on the wire;
        // `0x0195` is the only message sent when changing the open ground container, and without it a client that
        // closes a corpse leaves the server streaming that corpse's contents.
        R::OpenTradeNegotiations(m) => s.send_action(m),
        R::NoLongerViewingContents(m) => s.send_action(m),
        // ---- vendor ------------------------------------------------------------------------
        //
        // The same shape once more: ordinary ordered game actions.
        R::VendorBuy(m) => s.send_action(m),
        R::VendorSell(m) => s.send_action(m),
        // ---- secure trade ------------------------------------------------------------------
        //
        // All five are `NetQueue::Weenie` in `dereth_protocol::opcodes`, so `outbound_kind` classifies
        // them `GameAction` and this is the right sender -- the same global `OrderedActionHeader` counter
        // every arm above uses.
        R::TradeAddToTrade(m) => s.send_action(m),
        R::TradeAcceptTrade(m) => s.send_action(m),
        R::TradeDeclineTrade(m) => s.send_action(m),
        R::TradeResetTrade(m) => s.send_action(m),
        R::TradeCloseTradeNegotiations(m) => s.send_action(m),
        // `0x0224` and `0x0058`. Both are `NetQueue::Weenie` in
        // `dereth_protocol::opcodes`, so `outbound_kind` classifies them `GameAction` and this is the
        // right sender.
        R::SetDesiredComponentLevel(m) => s.send_action(m),
        R::ModifyCharacterSquelch(m) => s.send_action(m),
        // `0x0059`, the same classification for the same reason: `dereth_protocol::
        // opcodes` marks `COMMUNICATION_MODIFY_ACCOUNT_SQUELCH` `NetQueue::Weenie`, and
        // the account-squelch modification event opens by obtaining the next UI counter
        // and packing an order header, which is the ordered game-action
        // shape exactly.
        R::ModifyAccountSquelch(m) => s.send_action(m),
        // `0x01A1`. `outbound_kind` puts `Character_CharacterOptionsEvent` on
        // the Weenie queue, so it is a game action on the same global `OrderedActionHeader` counter as every
        // arm above — which is what `dereth-protocol`'s own corpus test asserts by re-encoding all
        // five recorded blobs **including the stamp**.
        R::CharacterOptionsEvent(m) => s.send_action(m),
        // ---- forcing an object description ----------------------------------------------
        //
        // Forcing an object description is **not** a game action: `outbound_kind` puts
        // **`0xF6EA`** on the Control queue with no `OrderedActionHeader`, so `send_action` refuses it by
        // design. (It is not `0xF7C8`, which is `Login_SendEnterWorldRequest` on the **Logon**
        // queue.)
        //
        // Two producers raise it. The world tick's null-table sweep performs the 20-second
        // re-request of every dangling reference — the caller that produced all 35 `0xF6EA` in the
        // recorded corpus — and `set_weenie_desc` step 5 is the merge-path desync ask. Without
        // this arm a client that loses an object reference has **no recovery path at all**: the
        // null entry is re-swept every 20 s for ever and nothing leaves the machine.
        //
        // `send_force_objdesc` is the bare sender, verified byte for byte against those 35 blobs
        // by the client-session force-objdesc test and by `replay_every_scenario`.
        R::ForceObjdesc(m) => {
            s.send_force_objdesc(m.id);
            return true;
        }
        // ---- the twenty-two the chat entry is the only producer of ---------------------------
        //
        // Every one of these is an ordinary game action on the same global `OrderedActionHeader` counter and
        // the Weenie queue as every arm above -- `dereth_protocol::opcodes` gives all twenty-two
        // `send_queue: Some(NetQueue::Weenie)`, and each corresponding sender has the talk
        // sender's shape with a different opcode literal and a different body. See
        // `dereth_client_model::chat_cmd` for the handler each comes from.
        R::TeleToLifestone(m) => s.send_action(m),
        R::TeleToMarketplace(m) => s.send_action(m),
        R::TeleToPkArena(m) => s.send_action(m),
        R::TeleToPklArena(m) => s.send_action(m),
        R::EnterPkLite(m) => s.send_action(m),
        R::Suicide(m) => s.send_action(m),
        R::TeleToHouse(m) => s.send_action(m),
        R::TeleToMansion(m) => s.send_action(m),
        R::QueryAge(m) => s.send_action(m),
        R::QueryBirth(m) => s.send_action(m),
        R::SetAfkMode(m) => s.send_action(m),
        R::SetAfkMessage(m) => s.send_action(m),
        R::ModifyGlobalSquelch(m) => s.send_action(m),
        R::ChannelIndex(m) => s.send_action(m),
        R::ChannelList(m) => s.send_action(m),
        R::AddToChannel(m) => s.send_action(m),
        R::RemoveFromChannel(m) => s.send_action(m),
        R::DisplayPlayerConsentList(m) => s.send_action(m),
        R::ClearPlayerConsentList(m) => s.send_action(m),
        R::RemoveFromPlayerConsentList(m) => s.send_action(m),
        R::AddPlayerPermission(m) => s.send_action(m),
        R::RemovePlayerPermission(m) => s.send_action(m),
        // ---- the `@house` family -----------------------------------------------------------
        //
        // Ordinary game actions on the same `OrderedActionHeader` counter and the Weenie queue as every arm
        // above -- `dereth_protocol::opcodes` gives all fifteen `send_queue: Some(NetQueue::Weenie)`,
        // and `fixtures/packet-captures/house-purchase-and-trade.jsonl` carries thirteen of them as `0xF7B1` frames
        // from the retail client. See `dereth_client_model::chat_cmd`'s Family 5 for the ladder each comes
        // from.
        R::SetOpenHouseStatus(m) => s.send_action(m),
        R::SetHooksVisibility(m) => s.send_action(m),
        R::BootEveryone(m) => s.send_action(m),
        R::BootSpecificHouseGuest(m) => s.send_action(m),
        R::AddPermanentGuest(m) => s.send_action(m),
        R::RemovePermanentGuest(m) => s.send_action(m),
        R::RemoveAllPermanentGuests(m) => s.send_action(m),
        R::RequestFullGuestList(m) => s.send_action(m),
        R::ModifyAllegianceGuestPermission(m) => s.send_action(m),
        R::ModifyAllegianceStoragePermission(m) => s.send_action(m),
        R::ChangeStoragePermission(m) => s.send_action(m),
        R::AddAllStoragePermission(m) => s.send_action(m),
        R::RemoveAllStoragePermission(m) => s.send_action(m),
        R::ListAvailableHouses(m) => s.send_action(m),
        R::AbandonHouse(m) => s.send_action(m),
        // The soul-emote event, `0x01E1`, the other half of communication pose handling.
        R::SoulEmote(m) => s.send_action(m),
        other => {
            tracing::warn!("no sender for {other:?}");
            return false;
        }
    };
    match out {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("{r:?} would not encode: {e}");
            false
        }
    }
}
