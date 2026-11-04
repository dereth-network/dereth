//! The server: what it answers a client with, as the January 2017 servers answered.
//!
//! These rows hold the server's side of the recorded sessions: what a server sent, in what
//! order, on which queue, for each thing a client asked. The server's internals (its store, its
//! settings) are tooling.

use super::{Behaviour, Evidence, Tier, RETAIL, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "server.accounts.listed-need-their-ticket",
        says: "A server that admits only listed accounts lets an account in with the ticket it \
               was made with, and refuses any other login before sending the world's name.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ACCOUNTS"),
        station: "yalain-testkit::cpu::entry::character_select::a_listed_account_logs_in_with_its_ticket_only",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.accounts.made-for-a-command-can-log-in",
        says: "A character the operator makes for an account that does not exist makes the \
               account on an open world, to log in with the first ticket it is given; a world of \
               listed accounts makes no account without a ticket, and says to make it with one.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ACCOUNTS-COMMAND"),
        station: "yalain-testkit::cpu::entry::character_select::an_account_made_for_an_operators_character_can_log_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.accounts.names-at-most-50",
        says: "An account name longer than 50 UTF-16 units is refused, and no account is made \
               for it, as Empyrean refuses one; the operator cannot make one either.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ACCOUNTS-NAME-LENGTH"),
        station: "yalain-testkit::cpu::entry::tickets::an_account_name_longer_than_fifty_is_refused_and_makes_no_account",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.accounts.older-secrets-kept-again-as-bcrypt",
        says: "A secret kept as earlier versions kept it (a salted SHA-256) admits its ticket \
               once more and is then kept as bcrypt at the world's cost, as Empyrean moves an \
               older password hash to bcrypt at its next good login; a wrong ticket is refused \
               and leaves it as it was.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ACCOUNTS-OLDER-SECRETS"),
        station: "yalain-testkit::cpu::entry::tickets::an_older_salted_sha256_secret_admits_its_ticket_and_is_kept_as_bcrypt_after",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.accounts.one-secret-in-every-form",
        says: "One account's secret is admitted from the September 2013 client's game login, our \
               client's game login and the friends ticket alike, whichever presented it first, \
               and a wrong secret is refused in each form.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ACCOUNTS-ONE-SECRET"),
        station: "yalain-testkit::cpu::entry::tickets::one_account_is_admitted_from_every_form_of_its_secret_and_a_wrong_one_from_none",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.accounts.open-made-at-first-login-then-need-their-ticket",
        says: "A server with open accounts makes an account the first time its name logs in, with \
               that ticket, and from then on refuses a login to it with any other ticket before \
               sending the world's name, as ACE checks an account's password; an account with no \
               ticket takes the first one it logs in with.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ACCOUNTS-OPEN"),
        station: "yalain-testkit::cpu::entry::character_select::an_open_account_is_made_at_its_first_login_and_needs_that_ticket_after",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.accounts.refused-logins-hold-back-their-address",
        says: "Five logins refused from one address (an IPv6 address by its /64) within a minute \
               have its logins refused unchecked for a minute after, with the same answer as any \
               refusal, as the friends service holds back refused tickets; Empyrean has no such \
               hold, and checks every password it is sent.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ACCOUNTS-LOGIN-HOLD"),
        station: "yalain-testkit::cpu::entry::tickets::five_refused_logins_from_one_address_hold_its_logins_back_for_a_minute",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.accounts.secrets-kept-again-at-the-worlds-cost",
        says: "A secret kept at a bcrypt cost other than the world's is kept again at the \
               world's cost when its ticket logs in, as Empyrean rehashes a password whose work \
               factor differs from its setting; a wrong ticket leaves it as it was.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ACCOUNTS-SECRET-COST"),
        station: "yalain-testkit::cpu::entry::tickets::a_secret_kept_at_another_cost_is_kept_again_at_the_worlds_cost_by_its_login",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.accounts.secrets-kept-as-salted-bcrypt",
        says: "An account's secret is kept as a bcrypt hash at the world's cost with a salt of \
               its own, never as the ticket, as Empyrean keeps passwords; a secret in no form the \
               server keeps matches nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ACCOUNTS-BCRYPT"),
        station: "yalain-testkit::cpu::entry::tickets::an_accounts_secret_is_kept_as_a_salted_bcrypt_hash_and_never_as_its_ticket",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.accounts.the-retail-login-presents-the-plain-secret",
        says: "The September 2013 client's game login carries the secret it was given as a \
               string in the ticket's blob (a 16-bit count, the bytes scrambled, padding to four \
               that is not read), and is admitted to an account kept with that secret as plain \
               text; a wrong secret is refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ACCOUNTS-RETAIL-TICKET"),
        station: "yalain-testkit::cpu::entry::tickets::the_retail_login_is_admitted_with_the_plain_secret_it_was_given",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.a-long-answer-is-one-message",
        says: "An answer of many lines to an admin's command, said in game or called, is one message \
               with its lines in their order, since separate messages sent in a row can overtake each \
               other on the wire.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-ONE-MESSAGE"),
        station: "yalain-testkit::cpu::operator::moderation::a_long_ban_list_said_in_game_comes_as_one_notice_in_its_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.a-restart-keeps-the-consoles-demotion",
        says: "An account the console made a player again stays a player at the next start, with the \
               configured list as it was; an admin stays one.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-DEMOTION-KEPT"),
        station: "yalain::bin.yalain::admin_tests::a_restart_does_not_undo_the_consoles_demotion",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.admin-characters-asked-for-only-by-admins",
        says: "A creation asking for the admin template from a player's account is answered with the \
               client's own refusal for it and makes nothing; from an admin's account it makes a character \
               of that template.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-CREATE"),
        station: "yalain-testkit::cpu::operator::admins::a_creation_asking_for_the_admin_template_needs_an_admins_account",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.admin-characters-enter-from-the-admin-template",
        says: "A character of an admin's account made an admin character enters from the admin template, \
               its object's entity and package and its description's record, for its own client and every \
               client that sees it; a player's account cannot have one, and its rights stop at once when it \
               is unmade.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-TEMPLATE"),
        station: "yalain-testkit::cpu::operator::admins::an_admin_character_enters_from_the_admin_template",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.attackable-hides-from-creatures",
        says: "A creature does not notice an admin set stealthy or super, and notices one set normal.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-ATTACKABLE"),
        station: "yalain-testkit::cpu::operator::admin_calls::a_stealthy_admin_is_not_noticed_by_creatures",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.calls-refused-for-anyone-else",
        says: "Every call of the admin family and every admin-only call of another family is refused from \
               anyone but an admin character of an admin's account: counted, logged and unanswered, before \
               any of it is read.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-GATE"),
        station: "yalain-testkit::cpu::operator::admin_calls::every_admin_call_is_refused_for_anyone_but_an_admin_character",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.character-and-object-calls",
        says: "An admin character's state and object calls act: health set and made full, a place saved in \
               its slot, an object made by its template's id and deleted; words that name nothing are \
               answered as such.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-STATE-CALLS"),
        station: "yalain-testkit::cpu::operator::admin_calls::the_state_and_object_calls_act",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.cloak-hides-an-admin",
        says: "A cloaked admin is told to no player's client, and those that knew it see it leave; lifting \
               it shows it again; an admin who sees what is hidden still sees it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-CLOAK"),
        station: "yalain-testkit::cpu::operator::admin_calls::a_cloaked_admin_is_seen_by_no_players_client",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.cloak-hides-from-ids-and-names",
        says: "A client may name anyone by an id or a name it guesses: selecting, examining, attacking, \
               using a skill on, using, recruiting, swearing to, trading with or telling a cloaked \
               admin is answered as if nobody had that id or name, and its allegiance shows it out of \
               the world. Only it and an admin who sees what is hidden find it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-CLOAK-LOOKUPS"),
        station: "yalain-testkit::cpu::operator::admin_calls::a_cloaked_admin_is_answered_for_as_if_nobody_had_its_id_or_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.cloak-kept-past-a-stay",
        says: "A cloak outlasts a stay in the world: the admin enters seen by no player's client and is \
               told it is still cloaked. Never-say-die, how creatures treat it, what it passes through, \
               its speed and its sight end with the stay. Chosen: nothing of retail's holds these; a \
               cloak silently ending is worse than one kept, and a kept never-say-die riskier than a \
               lost one.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-CLOAK-KEPT"),
        station: "yalain-testkit::cpu::operator::admin_calls::a_cloak_lasts_past_a_stay_and_nothing_else_an_admin_set_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.commands-do-nothing-for-anyone-else",
        says: "Real commands with harmful values do nothing from a player's account or from an ordinary \
               character of an admin's: no boot, ban, smite, extermination, money, made object, teleport, \
               broadcast or rename; from an admin character each does what it says.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-GATE-EFFECT"),
        station: "yalain-testkit::cpu::operator::admin_calls::a_players_admin_calls_do_nothing_and_an_admins_do_what_they_say",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.config-raises-only-what-this-start-makes",
        says: "The operator's configured list of admins raises a name only on the start that makes its \
               account from a ticket in the same configuration; an account already there, which on an open \
               world may be a stranger's, stays a player whatever ticket the list holds, and a name ahead \
               of its account makes nothing and grants nothing, at that start or the next.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-CONFIG"),
        station: "yalain::bin.yalain::admin_tests::the_configured_admins_do_not_raise_a_strangers_account",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.dumpadmintable-shows-the-groups",
        says: "An admin's request for the admin table answers with the account-level commands and the \
               groups each is kept to, as the operator set them.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-TABLE"),
        station: "yalain-testkit::cpu::operator::admin_calls::the_admin_table_shows_the_groups_as_set",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.effect-stamps-follow-the-object",
        says: "An effect an admin shows or ends on a player or a creature carries that object's next \
               visual stamp, of its own incarnation, to every client that knows it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-EFFECT-STAMPS"),
        station: "yalain-testkit::cpu::operator::admin_calls::an_effect_an_admin_shows_follows_the_objects_own_stamps",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.effect-typed-takes-effect",
        says: "An effect typed on oneself, and ending it, reach the admin's own client, which neither \
               drops them as old nor holds them back.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-EFFECT-TYPED"),
        station: "yalain-testkit::dat::admin::an_effect_typed_on_oneself_reaches_the_admins_own_client",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.admin.ethereality-override",
        says: "An ethereality override changes an admin's movement ethereality, told to every client that \
               knows it; it stands through a change of area until lifted, and normal puts the admin's own \
               back.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-ETHEREALITY"),
        station: "yalain-testkit::cpu::operator::admin_calls::an_ethereality_override_is_told_and_lifted",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.every-call-answers-an-admin",
        says: "Every call of the admin family answers an admin character whatever comes with it: none stops \
               the server or needs its values.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-ANSWERS"),
        station: "yalain-testkit::cpu::operator::admin_calls::every_admin_call_answers_an_admin_character_even_with_no_values",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.generate-makes-at-most-a-hundred",
        says: "An admin's generate call makes the number asked for, one if the number is not a count, \
               and at most a hundred. Our choice.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-GENERATE-MOST"),
        station: "yalain-testkit::cpu::operator::admin_calls::generate_makes_what_is_asked_up_to_a_hundred",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.library-carries-the-ethereality-words",
        says: "With the admin library goes the record of the ethereality command's words, which no \
               install holds, so a retail client finds a value for each and sends the command. The \
               values are this server's own; what the original numbered them is not held.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-ETHEREALITY-WORDS"),
        station: "yalain-testkit::cpu::operator::admin_calls::the_library_carries_the_words_of_the_ethereality_command",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.library-names-the-log-type",
        says: "With the admin library goes the record of the log type's names, which no install holds, \
               so an admin's answer shows under a name and not an empty []. A client holding the \
               iteration before is sent that record alone, not the library again. The one name is \
               chosen: what the original listed is not held.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-LOG-TYPE"),
        station: "yalain-testkit::cpu::operator::admin_calls::the_library_names_the_log_type_and_a_client_holding_the_last_gets_only_that",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.library-not-sent-to-our-client",
        says: "Our client on an admin's account is sent no library by a server that serves one: its \
               characters come straight on, with nothing downloaded.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-LIBRARY-NOT-OURS"),
        station: "yalain-testkit::dat::admin::our_client_on_an_admins_account_is_not_sent_the_library",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.admin.library-sends-admin-calls-in-the-clear",
        says: "The library an admin's retail client is served is the install's own with every encrypted \
               send made a plain one, in the same bytes otherwise, 175 in the final client's library; a \
               record with too few is refused as not the library this was made for.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-LIBRARY"),
        station: "yalain-testkit::dat::admin::the_admin_library_is_the_installs_with_every_send_in_the_clear",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.admin.library-served-only-to-admin-accounts",
        says: "The library goes to an admin's account alone, before its characters: told of it and sent the \
               record, its list held back until the client has stored it; a player's account is sent none, \
               nor an admin's whose client lists the library's iteration.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-LIBRARY-SERVED"),
        station: "yalain-testkit::cpu::operator::admin_calls::only_an_admins_login_is_served_the_library",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.moderation-calls",
        says: "An admin character's moderation calls answer who an account or character is and change the \
               world's limit, and show a player a line; a call with no counterpart here is answered that \
               this server has none.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-MOD-CALLS"),
        station: "yalain-testkit::cpu::operator::admin_calls::the_moderation_calls_answer_and_act",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.named-groups-gate-the-account-commands",
        says: "An account-level admin command the operator keeps to named groups is refused to an admin in \
               none of them, in game and as a call, logged and told so; an admin in one may give it; a \
               command kept to nothing is open to every admin; the console is never refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-GROUPS"),
        station: "yalain-testkit::cpu::operator::admin_calls::the_account_level_commands_are_kept_to_their_named_groups",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.never-say-die-keeps-an-admin-alive",
        says: "With never say die on, a killing blow leaves an admin's health at one; off, its health falls \
               as usual.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-NSD"),
        station: "yalain-testkit::cpu::operator::admin_calls::never_say_die_holds_an_admins_health_at_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.no-admin-without-a-secret",
        says: "An account with no secret cannot be made an admin's: an open world gives such an account to \
               the first ticket it sees.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-NO-SECRET"),
        station: "yalain-testkit::cpu::operator::admins::an_account_with_no_secret_cannot_be_made_an_admins",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.no-claiming-an-admins-account",
        says: "An admin's account with no secret is not claimed by the first ticket on an open world, at \
               the game's login or the friends service's; a player's account with no secret takes the first \
               ticket, as ever.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-NO-CLAIM"),
        station: "yalain-testkit::cpu::operator::admins::an_admins_account_with_no_secret_is_not_claimed_by_the_first_ticket",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.quest-removal-takes-its-subquests",
        says: "An admin's taking a quest off a player takes the subquests of its phases with it, done \
               or not, and theirs, without a word, but for one another quest the player holds at a \
               phase that lists it. Chosen: no recording holds the removal.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-QUEST-SUBS"),
        station: "yalain-testkit::cpu::operator::admin_calls::a_quest_an_admin_takes_off_a_player_takes_its_subquests_too",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.quest-removal-told-at-once",
        says: "An admin's taking a quest off a player tells its client at once with the update of the \
               removing kind, which the client's own handling takes off its list; a hidden quest goes \
               without a word. No recording holds such an update, so the kind is read from the client.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-QUEST-REMOVED"),
        station: "yalain-testkit::cpu::operator::admin_calls::a_quest_an_admin_takes_off_a_player_leaves_its_clients_list_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.recipes-told-at-once",
        says: "An admin's teaching a player a recipe or taking one away tells its client at once, by \
               the calls the world's own learning and forgetting use; one that changes nothing tells \
               nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-RECIPES-TOLD"),
        station: "yalain-testkit::cpu::operator::admin_calls::a_recipe_an_admin_adds_or_removes_reaches_the_players_client_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.refused-calls-log-one-line",
        says: "A refused admin call is counted every time, but logged only the first time from a \
               character in each stay in the world, so that a client cannot fill the log.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-REFUSED-LOG"),
        station: "yalain-testkit::cpu::operator::admin_calls::refused_admin_calls_log_one_line_a_stay_however_many_come",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.refused-lines-log-one-line",
        says: "An admin command typed by a player is counted and said as usual, but logged only \
               the first time from a character in each stay in the world, so that a client cannot \
               fill the log.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-REFUSED-LINES"),
        station: "yalain-testkit::cpu::operator::admin_calls::refused_admin_lines_log_one_line_a_stay_however_many_come",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.rename-told-at-once",
        says: "A rename reaches the renamed client and every client that knows it at once as a change \
               of its name's quality, and its own description holds the new name at its next entry. Our \
               choice: the client's help says the character was relogged, and no recording holds a \
               player's rename.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-RENAME-TOLD"),
        station: "yalain-testkit::cpu::operator::admin_calls::a_rename_reaches_every_client_at_once_and_lasts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.rights-end-at-once",
        says: "An admin character's rights end at once when its account is no longer an admin's, or it is \
               no longer an admin character: the check is on the server's record at the time of each call.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-RIGHTS-END"),
        station: "yalain-testkit::cpu::operator::admin_calls::an_admin_characters_rights_end_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.rooms-of-an-admins-own",
        says: "An admin's chat room is made by name, holds its maker and whoever joins by name, and \
               lasts while anyone is in it. A line reaches its members as an admin's answer and no one \
               else, since a client shows no room but its channels; one from outside is refused as one \
               to a room that isn't there. Our design: no recording holds one.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-ROOMS"),
        station: "yalain-testkit::cpu::operator::admin_calls::an_admins_chat_room_reaches_its_members_alone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.suffix-shown-after-the-name",
        says: "A suffix goes after a player's name, before the mark for its grammar, for every client \
               that knows it and for good; empty, it comes off. Nothing that reads as that mark or \
               breaks a line is taken. Our design: the help says only that it adds a suffix to the \
               name.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-SUFFIX"),
        station: "yalain-testkit::cpu::operator::admin_calls::a_suffix_shows_after_the_name_for_every_client_and_lasts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.taking-admin-away-ends-what-it-set",
        says: "A character the console makes a player again is at once no longer hidden, can die, is \
               noticed by creatures, has its own ethereality and speed, and no longer sees what is hidden, \
               whether its account lost the flag or the character its template.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-REVOKE"),
        station: "yalain-testkit::cpu::operator::admin_calls::taking_admin_away_ends_what_the_admins_commands_set",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.template-follows-the-accounts-flag",
        says: "An admin character whose account is no longer an admin's enters as a player: the template \
               follows the account's flag at every entry, whatever the character stores.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-FOLLOWS"),
        station: "yalain-testkit::cpu::operator::admins::an_admin_character_of_a_players_account_enters_as_a_player",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.admin.what-makes-an-admin-is-the-consoles",
        says: "What makes an admin is the operator's alone: said in game by an admin, raising an account, \
               making an admin character, changing groups, a new admin's account and another admin's \
               password are refused and change nothing; a player's password may be set as before.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-CONSOLE-ONLY"),
        station: "yalain-testkit::cpu::operator::admin_calls::an_admin_in_game_cannot_make_admins_or_join_groups",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.allegiance.a-stored-circle-has-an-end",
        says: "A store edited by hand into a circle of oaths is walked once round: every member \
               counted once, nothing hangs.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ALLEGIANCE-CIRCLE"),
        station: "yalain-testkit::cpu::play::allegiance::a_stored_circle_of_oaths_is_walked_once_round",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.allegiance.an-oath-sworn-and-taken",
        says: "Swearing to a player beside asks it; taking the oath tells the patron who swore and \
               its rank (a new monarch its allegiance's chat room), tells the vassal the chat \
               room, to whom it swore and its rank, and gives each its profile: the patron's with \
               the vassal under it, the vassal's with its patron over it and its monarch when the \
               monarch is not its patron; a monarch's own profile names neither.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ALLEG-OATH"),
        station: "yalain-testkit::cpu::play::allegiance::an_oath_sworn_and_taken_tells_both_and_gives_each_its_profile",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.allegiance.at-entry",
        says: "A member entering the world hears its allegiance's chat room twice, then its \
               profile; one in no allegiance is sent the empty profile alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ALLEG-ENTRY"),
        station: "yalain-testkit::cpu::play::allegiance::a_member_entering_again_hears_its_room_twice_then_its_profile",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.allegiance.broken",
        says: "Breaking with a patron tells the player whom it broke with and sends it its profile \
               as it now stands; the patron left with no vassal is sent the empty profile.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ALLEG-BREAK"),
        station: "yalain-testkit::cpu::play::allegiance::breaking_with_a_patron_tells_whom_and_empties_both_profiles",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.allegiance.experience-passed-up",
        says: "A quarter of what a vassal earns goes into its patron's pool; a pool reaching a \
               fifth of the patron's next level's experience is paid to the patron, up to a third \
               and a half of that level's experience for the level, with a text naming the vassal; \
               a patron away is paid as it enters and told what its vassals earned.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ALLEG-XP"),
        station: "yalain-testkit::cpu::play::allegiance::a_vassals_experience_is_pooled_and_paid_to_its_patron_up_to_the_levels_cap",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.allegiance.others-see-it-grow",
        says: "When a vassal swears, every other member of the allegiance in the world is sent its \
               profile again, with the allegiance's new size.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ALLEG-PUSH"),
        station: "yalain-testkit::cpu::play::allegiance::every_other_member_in_the_world_is_sent_its_profile_when_one_swears",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.allegiance.profiles-asked-for",
        says: "A member's profile asked for is answered with the profile centred on that member, \
               in the world or not; asking for nothing (id 0) is not answered; one in no \
               allegiance is sent the empty profile.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ALLEG-QUERY"),
        station: "yalain-testkit::cpu::play::allegiance::a_profile_asked_for_is_centred_on_the_member_and_nothing_is_not_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.allegiance.recorded-officers-and-names",
        says: "Every recorded profile flags as officers exactly the members in its officer set, the \
               monarch among them, and every recorded export of the tree files each name under \
               the name's hash, as this server does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ALLEG-RECORDED"),
        station: "yalain-testkit::corpus::social::recorded_profiles_flag_their_officers_and_exports_file_names_by_their_hash",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.allegiance.the-monarch-is-an-officer",
        says: "A profile names its allegiance's officers: the monarch, whose own data says it is \
               one; the members appointed by no one are not officers.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ALLEG-OFFICERS"),
        station: "yalain-testkit::cpu::play::allegiance::a_profile_names_the_monarch_its_allegiances_officer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.allegiance.tree-exported",
        says: "Exporting the allegiance's tree sends the asker every member's place in it, with its \
               patron and its vassals, the members by name and by id, the allegiance's chat room, \
               and the file's name for the client to write.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ALLEG-EXPORT"),
        station: "yalain-testkit::cpu::play::allegiance::the_tree_export_sends_the_whole_allegiance_and_the_files_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.areas.centered-on-the-user",
        says: "An area skill used on its user is centered on it: its playing hook names the \
               user and its area names the creatures within its radius of the user, each with \
               its own outcome; those it lands on take the skill's effects.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AREA-SELF"),
        station: "yalain-testkit::cpu::play::areas::an_area_skill_used_on_its_user_reaches_the_creatures_around_the_user",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.areas.every-creature-within-the-radius",
        says: "A skill's area hook reaches every live creature whose body is within its radius \
               of the target's: the story names each in turn with its own outcome and damage, \
               those it lands on take the skill's effects, and creatures beyond are left alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AREA"),
        station: "yalain-testkit::cpu::play::areas::an_area_hook_reaches_every_creature_within_its_radius_each_with_its_own_outcome",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.auras.damage-held-on-a-weapon",
        says: "What an aura holds on a weapon's damage counts in its blows and shows in its \
               examination: the greatest changed, rounded down, and the least following from it \
               by the weapon's span.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AURA-HELD-DAMAGE"),
        station: "yalain-testkit::cpu::play::two_blows::a_swing_does_the_damage_an_aura_holds_on_its_weapon",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.auras.held-pulses-last-while-held",
        says: "What an aura holds on a thing in hand lasts while the aura is on and the thing in \
               hand, on a weapon or a shield as the aura names them. Our choice: no record of its \
               own, so it ends with either at once and not at a pulse's three minutes; of one \
               effect from two auras, the stronger; a weapon's damage multiplied before it is \
               added to.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-AURA-ON-HELD-LASTS"),
        station: "yalain-testkit::cpu::play::auras::an_aura_on_what_is_held_keeps_its_pulses_off_the_player",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.auras.held-reach-fellows",
        says: "An aura for what is held holds on what its user's fellows within its range have in \
               hand, as it does on its user's, and on nobody else's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AURA-HELD-FELLOWS"),
        station: "yalain-testkit::cpu::play::auras::an_aura_for_weapons_holds_on_a_fellows_weapon_within_its_range",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.auras.off-in-portal-space",
        says: "Going into portal space turns every aura its player holds off: the toggle off, \
               its pulses' records and its own taken off, the appearance stopped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AURA-PORTAL"),
        station: "yalain-testkit::cpu::play::auras::going_into_portal_space_turns_auras_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.auras.pulse-missed",
        says: "A player holding a record of an earlier pulse of an aura that the next pulse \
               does not reach is told the pulse missed it, naming the record, and its client \
               shows the record running down.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AURA-MISSED"),
        station: "yalain-testkit::cpu::play::auras::a_player_an_auras_pulse_missed_is_told",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.auras.pulses",
        says: "An aura pulses within a period of its turning on and then every 121 to 124 \
               seconds; each pulse is a record of its own that runs out after three minutes, so \
               two run at once, and the appearance shows once while any runs.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AURA-PULSES"),
        station: "yalain-testkit::cpu::play::auras::an_aura_pulses_every_two_minutes_each_pulse_a_record_of_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.auras.pulses-on-what-is-held",
        says: "An aura for what its user holds puts no record of its pulses on the user: the thing in \
               hand holds them, and its examination lists them with their spellcraft under \
               Enchantment Effects and shows its vigor cost and a weapon's combat delay lowered, \
               rounded down.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AURA-ON-HELD"),
        station: "yalain-testkit::cpu::play::auras::a_weapons_examination_shows_what_an_aura_holds_on_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.auras.reach-fellows",
        says: "An aura's pulses reach its user's fellows within its range as they reach its user; a fellow \
               gone beyond it that holds an earlier pulse's record is told the next pulse missed it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AURA-FELLOWS"),
        station: "yalain-testkit::cpu::play::auras::an_auras_pulses_reach_fellows_within_its_range",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.auras.through-a-resurrection",
        says: "A resurrection's trip through portal space turns no aura off, and three seconds \
               after the player is back each aura it holds pulses once, beside its own round.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AURAS-THROUGH-RESURRECTION"),
        station: "yalain-testkit::cpu::play::auras::a_resurrection_leaves_auras_on_and_they_pulse_again_as_it_comes_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.auras.turned-off",
        says: "A toggled skill used while its aura is on turns it off at once: attacking, the \
               use over, the toggle off, the pulses' records then the aura's taken off, the \
               text that it stopped, not attacking, the appearance stopped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AURA-OFF"),
        station: "yalain-testkit::cpu::play::auras::using_a_toggled_skill_again_turns_its_aura_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.auras.turned-on",
        says: "A toggled skill's use ends by turning its aura on: the first pulse's record, the \
               aura's own record held until taken off, not attacking, the toggle on, the \
               skill's information marked toggled, the pulse's appearance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AURA-ON"),
        station: "yalain-testkit::cpu::play::auras::a_toggled_skill_turns_its_aura_on_as_its_use_ends",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.books.a-book-used-is-opened",
        says: "A book used from the pack is opened with its page table and its controls shown, \
               then the use is answered, in one tick.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BOOK"),
        station: "yalain-testkit::cpu::play::books::a_book_used_from_the_pack_is_opened_then_the_use_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.books.a-card-shows-its-picture",
        says: "A tarot card whose picture is known, used from the pack, shows its picture with no \
               page controls, then the use is answered; the card is kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BOOKS-TAROT"),
        station: "yalain-testkit::cpu::play::books::a_greater_trump_used_shows_its_picture_and_is_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.books.a-person-shows-a-book",
        says: "A person who shows a book, used in reach, answers the use and then shows the book \
               with its page controls.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BOOKS-NPC"),
        station: "yalain-testkit::cpu::play::books::a_shi_used_answers_the_use_then_shows_the_architects_book",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.characters.banned-names-from-the-operator",
        says: "A server given a list of banned words refuses a new character whose name holds \
               one, its letters matched in small letters with spaces and marks between them \
               ignored; a server given none refuses no name for its words.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NAME-BANNED"),
        station: "yalain-testkit::cpu::entry::character_select::a_name_holding_an_operators_banned_word_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.characters.created-and-listed",
        says: "A character the client creates is answered with its identity: an object id in the \
               players' range, its name and the look its choices give; it is in the account's \
               lists from then on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATE"),
        station: "yalain-testkit::cpu::entry::character_select::a_created_character_is_answered_with_its_identity_and_listed_after",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.characters.creation-refusals",
        says: "Creating a character with a name in use is answered name-in-use with an empty \
               identity; a name that is not a name, name refused; a character past the account's \
               six, no free slot.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATE-REFUSED"),
        station: "yalain-testkit::cpu::entry::character_select::a_taken_or_bad_name_or_a_full_account_is_refused_with_its_reason",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.characters.deleted-brought-back",
        says: "A deleted character asked back, while it is listed, is answered as a made \
               character with its grey gone (the January 2017 server's answer to the sealed \
               request); one not deleted, or another account's, is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHARACTER-RESTORE"),
        station: "yalain-testkit::cpu::entry::character_select::a_deleted_character_asked_back_loses_its_grey",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.characters.deleted-greyed-for-an-hour",
        says: "Deleting a character is answered with its id, then the lists again: it is gone \
               from the list by name and greyed out in the list with looks, counting down an hour, \
               after which it is gone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DELETE"),
        station: "yalain-testkit::cpu::entry::character_select::a_deleted_character_stays_greyed_out_for_an_hour_then_is_gone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.characters.look-from-the-physique",
        says: "A new character looks like its race and sex's body, scaled by build times height \
               across and height up, with each appearance its physique choices select set to the \
               value chosen, every choice among parts also shown.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LOOK"),
        station: "yalain-testkit::cpu::entry::looks::a_new_character_looks_as_the_2017_server_made_her",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.characters.name-what-it-may-hold",
        says: "A new character's name is two to sixteen English letters, spaces, apostrophes and \
               hyphens, beginning and ending with a letter, never with two spaces together; any \
               other name is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-NAME-SHAPE"),
        station: "yalain-testkit::cpu::entry::character_select::only_a_name_the_2017_servers_held_is_made",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.characters.name-written-form",
        says: "A new character's name is kept trimmed at both ends, each letter a capital after a \
               space or a mark and small after a letter, whatever case it was typed in.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-NAME-FORM"),
        station: "yalain-testkit::cpu::entry::character_select::a_name_is_kept_in_its_written_form",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.characters.outlive-a-restart",
        says: "An account's characters, with their choices, looks and deletions, are the same at \
               every login, across the server stopping and starting again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSIST"),
        station: "yalain-store::cpu::persistence::characters_and_their_deletions_outlive_reopening_the_database",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.emote-plays-its-behavior",
        says: "An emote plays the behavior the emote table gives it on the emoter, for it and every \
               client that knows it, carrying the emote's id and a context; each client writes the \
               emote's text itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EMOTE"),
        station: "yalain-testkit::cpu::entry::chat::an_emote_plays_its_behavior_on_the_emoter_for_everyone_who_sees_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.line-from-afar-reaches-the-room",
        says: "A line from elsewhere reaches every player in the world as the room's own line \
               under the name it is given, with no meta letters, except a player the rule says \
               does not hear it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ROOM-FROM-AFAR"),
        station: "yalain-testkit::cpu::entry::chat::a_line_from_afar_reaches_the_players_in_the_room_and_only_those_it_lets_hear",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.messages-as-2017-sent-them",
        says: "The chat messages the server builds are those the 2017 servers sent: the empty \
               allegiance profile, a new character's channels, each emote's behavior, each typed \
               emote, tell confirmations and refusals, and the chat service's lines and answers, \
               byte for byte apart from the ids a packed graph numbers its entries with.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHAT-2017"),
        station: "yalain-testkit::corpus::chat::chat_messages_are_built_as_the_2017_servers_sent_them",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.chat.room-line-echoed-answered-and-relayed",
        says: "A line a client sends to a chat room comes back to it as the room's line (its name \
               without the grammar mark, the words, and what its client said of it), then as the \
               answer to its request, and reaches every other player in the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHAT-ROOM"),
        station: "yalain-testkit::cpu::entry::chat::a_room_line_is_echoed_answered_and_relayed_to_the_other_players",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.room-line-kept-for-the-link",
        says: "A line a client sends to a room every player has is kept for the world's link with \
               the words as sent; one to a kingdom's room, or one refused, is not.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ROOM-KEPT-FOR-LINK"),
        station: "yalain-testkit::cpu::entry::chat::a_line_in_a_room_everyone_has_is_kept_for_the_link_and_a_kingdoms_is_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.room-line-to-a-room-not-the-speakers-is-refused",
        says: "A line to a room that is not one of the world's, or to a kingdom's, allegiance's or \
               fellowship's room the speaker is not in, or from a client not in the world, is \
               answered with a refusal (the client shows that the room is not available) and \
               reaches nobody. The 2017 servers answered every recorded line 0; none was refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ROOM-REFUSED"),
        station: "yalain-testkit::cpu::entry::chat::a_line_to_a_room_the_speaker_may_not_use_is_refused_and_reaches_nobody",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.said-from-anywhere-as-a-room-line",
        says: "A line said from anywhere comes back to its speaker as the room's own line, \
               reaches the others and is kept for the world's link, and no request is answered; a \
               gagged character is told it may not talk, and nobody hears it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ROOM-SAID-ANYWHERE"),
        station: "yalain-testkit::cpu::entry::chat::a_line_said_from_anywhere_is_the_rooms_own_line_and_no_request_is_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.say-heard-nearby",
        says: "What a player says reaches it and every player within hearing as a saying on the \
               speaker's object, carrying the words exactly as the client sent them; a player out \
               of hearing does not hear it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SAY"),
        station: "yalain-testkit::cpu::entry::chat::a_saying_reaches_the_speaker_and_the_players_within_hearing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.speaker-note-as-recorded",
        says: "The note a world attaches to a line it says is what the recorded clients attach: \
               its version, a zero word, its flags and the speaker's meta letters, in each of the \
               four forms the recorded lines carry.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ROOM-SPEAKER-NOTE"),
        station: "yalain-testkit::cpu::entry::chat::a_speakers_note_is_what_the_recorded_clients_attach",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.system-line-ends-its-line",
        says: "A system line the world writes itself ends with a line end (LF), as the client's \
               own strings do, so lines in a row do not run together; one that already ends its \
               line is not given a second.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SYSTEM-LINE-END"),
        station: "yalain-testkit::cpu::entry::chat::a_system_line_the_world_writes_ends_its_line_as_the_clients_strings_do",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.tell-delivered-or-refused",
        says: "A tell reaches the player of the name typed (with or without its grammar mark, in \
               any case) as the teller's tell with its id and world name; the teller is shown its \
               tell with the name as typed and the words; a name no player in the world has is \
               answered that it is unavailable.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TELL"),
        station: "yalain-testkit::cpu::entry::chat::a_tell_reaches_the_named_player_and_the_teller_is_shown_it_or_told_the_name_is_unavailable",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.tell-finds-a-suffixed-name",
        says: "A tell finds a character by its name alone or by its name and suffix as a client shows \
               it, with or without the grammar mark; where one character's name is another's name and \
               suffix, the first hears it. Chosen: no recording holds an admin's suffix.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TELL-SUFFIX"),
        station: "yalain-testkit::cpu::entry::chat::a_tell_finds_a_character_by_its_name_with_or_without_its_suffix",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.chat.two-players-talk-in-a-channel-over-real-sockets",
        says: "Over real sockets, a line one player of our client writes to the general channel \
               goes to the server's chat service, comes back to it as the channel's line with its \
               answer, and is written by the other player's scripts under the channel's name with \
               the speaker's name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ROOM-LIVE"),
        station: "yalain::live::rooms::two_players_talk_in_the_general_channel_over_real_sockets",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.chat.typed-emote-heard-nearby",
        says: "A typed emote reaches the emoter and every client that knows it, on the emoter's \
               object, with its id, its name alone and the words as typed; a player elsewhere does \
               not hear it, nor anyone a gagged player's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TYPED-EMOTE"),
        station: "yalain-testkit::cpu::entry::chat::a_typed_emote_reaches_the_emoter_and_the_players_who_see_it_with_its_name_and_words",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.combat.attack-run-to-the-death",
        says: "Asked once to attack, the server answers that the player is attacking and swings \
               for it, each swing a story from the player with a hit or an evade on the target, \
               until the target dies; the killer is paid the creature's death experience and \
               reaches the level it crosses.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ATTACK-RUN"),
        station: "yalain-testkit::cpu::play::fights::an_attack_run_swings_until_the_creature_dies_and_pays_its_experience",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.combat.creature-damage-through-armor",
        says: "A creature's hit does its natural damage in full to a player of level 1 or 2, who \
               wears no armor yet, and from level 3 the share 2017's players' armor let through.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-DAMAGE-LOW"),
        station: "yalain-testkit::cpu::play::rules::a_creatures_hit_does_its_natural_damage_in_full_to_a_player_of_level_one_or_two",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.combat.critical-by-weapon",
        says: "What a weapon's own effects add to the chance of a critical goes on top of the \
               attack's, measured as a share of hits that land.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRITICAL-BONUS"),
        station: "yalain-testkit::cpu::play::fights::a_weapons_critical_bonus_makes_its_hits_critical",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.combat.empty-hand-refused",
        says: "A player with nothing in its hand has no basic attack: asked to attack, the server \n               answers that it is attacking, that its basic attacks failed (not wielding the right \n               item) and that it is not attacking, and nothing swings.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EMPTY-HAND"),
        station: "yalain-testkit::cpu::play::specials::an_attack_with_an_empty_hand_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.combat.landing-chance",
        says: "Whether an attack lands follows the fits to the 2017 attacks, by kind, level \
               difference and the player's own level: a level 1 player's bow lands about half \
               the time at equal levels, a level 40 player's three times in four; a creature's \
               melee attack a little more than half on players of any level.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HIT-CHANCE-LEVEL"),
        station: "yalain-testkit::cpu::play::rules::attacks_land_with_the_measured_chance_for_their_direction_level_difference_and_player_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.combat.player-hits-by-weapon",
        says: "A player's hit with a weapon is drawn evenly across the weapon's damage, with what its \
               effects add, less the creature's armor level, doubled when critical.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PLAYER-HIT-WEAPON"),
        station: "yalain-testkit::cpu::play::rules::a_players_hit_is_its_weapons_damage_less_the_creatures_armor",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.combat.portal-space-ends-a-run",
        says: "A trip through portal space ends a player's attack run: out of it the player is \
               no longer attacking and is at peace, and nothing more is swung.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PORTAL-ENDS-RUN"),
        station: "yalain-testkit::cpu::entry::places::a_trip_through_portal_space_ends_an_attack_run",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.combat.practice-targets-are-struck",
        says: "A practice target that never fights back, as the halls' Straw Drudges, is \
               attacked like any other creature: the run is answered and swings, and it never \
               strikes back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PRACTICE-TARGET"),
        station: "yalain-testkit::cpu::play::fights::a_practice_target_is_struck_and_never_strikes_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.combat.strike-lands",
        says: "A player's swing at a creature is told by its story first; the creature loses the \
               health as the strike lands, a time its skill sets over the story's time scale, and \
               falls only then.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SWING-LANDS"),
        station: "yalain-testkit::cpu::play::fights::a_players_swing_hurts_a_creature_as_it_lands_by_its_skill",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.combinations.link-found",
        says: "A combination that finds the effect type it looks for on its target plays its \
               private effect for its caster, takes the found effect off, and adds its own \
               effect (a strike the target tells).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-COMBO-FOUND"),
        station: "yalain-testkit::cpu::play::combinations::a_combination_that_finds_its_link_takes_it_off_and_strikes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.combinations.link-missing",
        says: "A combination that finds nothing it looks for on its target adds no strike of its own, \
               and its caster is told so with an empty private effect.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-COMBO-MISSING"),
        station: "yalain-testkit::cpu::play::combinations::a_combination_without_its_link_adds_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.content.body-a-visual-description",
        says: "A template whose qualities name a visual description itself as its body is drawn as \
               that description.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DOOR-HAUNTED-LOOK"),
        station: "yalain-testkit::corpus::content::a_body_that_is_a_visual_description_is_the_look",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.content.described-from-templates",
        says: "An object of the world is described from its template in the data files as the \
               2017 servers made it: its look, its script package and its physics words, and the \
               entry effect it showed when it appeared.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CONTENT-DESCRIBED"),
        station: "yalain-testkit::corpus::content::objects_are_described_from_their_templates_as_the_2017_servers_made_them",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.content.take-gates-of-classes",
        says: "The items whose classes name a quest for taking them read with it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TAKE-GATES"),
        station: "yalain-testkit::dat::skills::an_item_whose_class_names_a_quest_for_taking_reads_with_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.craft.a-product-wants-a-place-in-the-pack",
        says: "A recipe's product with no place in the pack is refused at the craft's result with its \
               own answer, and nothing is made or used up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-NO-ROOM"),
        station: "yalain-testkit::cpu::play::crafting::a_product_with_no_place_in_the_pack_is_refused_at_the_result_and_uses_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.butchery-finds-nothing",
        says: "A cut that draws its profile's share of nothing gathers no material and pays no experience; \
               the use is answered empty at the result and the corpse may be butchered again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BUTCHERY-EMPTY"),
        station: "yalain-testkit::cpu::play::butchery::a_cut_that_finds_nothing_is_answered_empty_and_leaves_the_corpse",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.butchery-gathers-a-material",
        says: "A hunting knife used on a corpse walks the player to it, plays the butchering behavior \
               and about two seconds later pays the butchery skill's experience, plays the craft \
               effect, puts the material on the player's stack and answers with what was gathered; a \
               corpse is butchered once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BUTCHERY"),
        station: "yalain-testkit::cpu::play::butchery::a_knife_used_on_a_corpse_butchers_it_once_for_a_material_and_experience",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.butchery-refused-while-another-opens",
        says: "A corpse another player is on its way to open is refused at once with target locked; \
               an open corpse, or one whose opener has arrived, may be butchered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BUTCHERY-LOCKED"),
        station: "yalain-testkit::cpu::play::butchery::a_corpse_another_player_is_on_its_way_to_open_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.butchery-too-damaged",
        says: "A corpse whose killer was 9 or more levels above the creature is too damaged to butcher and \
               is refused at once; one killed 8 levels above is butchered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BUTCHERY-TOO-DAMAGED"),
        station: "yalain-testkit::cpu::play::butchery::a_corpse_its_killer_far_outleveled_is_too_damaged_to_butcher",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.butchery-units-by-knife",
        says: "A cut gives ten units above the knife's own yield, twice that for an expert cut; a crude knife's yield is 1, a made knife's its level and five.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BUTCHERY-UNITS"),
        station: "yalain-testkit::cpu::play::butchery::a_knife_cuts_ten_units_above_its_own_yield",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.butchery-wants-a-place-in-the-pack",
        says: "A cut with no place in the pack for its material is refused as the work ends, with the \
               corpse still to butcher.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BUTCHERY-NO-ROOM"),
        station: "yalain-testkit::cpu::play::butchery::a_cut_with_no_place_in_the_pack_is_refused_and_leaves_the_corpse_to_butcher",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.charged-recipe-marked-used",
        says: "A recipe with charges made is marked used at that moment, a charge fewer, its record told with \
               the time of the telling.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-CHARGED-USED"),
        station: "yalain-testkit::cpu::play::crafting::a_recipe_with_charges_made_is_marked_used_now_with_a_charge_fewer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.charges-come-with-a-recipe",
        says: "A recipe with charges comes with them when learned, granted again has them all again, and with \
               none left is refused with the answer for no charges.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-CHARGES-2017"),
        station: "yalain-testkit::cpu::play::crafting::a_recipe_learned_has_its_charges_and_granted_again_has_them_all_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.check-counts-what-effects-add",
        says: "A making's check reads the craft skill with what the effects on the player add (a \
               hammer's, a crucible's), never past the skill's cap.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-CHECK-EFFECTS"),
        station: "yalain-testkit::cpu::play::crafting::a_makings_check_counts_what_effects_add_to_the_craft_skill",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.day-starts-over",
        says: "When a player's day of craft experience is over, every craft skill earns nothing today \
               again, the client is told all its craft skills, and the next day ends 22 hours later.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-DAY"),
        station: "yalain-testkit::cpu::play::crafting::a_craft_day_over_starts_every_craft_skills_day_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.delving-stone-summons-a-node",
        says: "A delving stone used from the pack pays the surveying skill, loses a point of durability \
               (destroyed through overuse at nothing), is answered, and conjures the node its summoning \
               effect names two meters straight ahead of the player, facing as it faces.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SURVEY-DELVING"),
        station: "yalain-testkit::cpu::play::mining::a_delving_stone_conjures_its_node_in_front_of_the_player_and_is_spent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.dyed-armor-as-2017",
        says: "Every piece of armor dyed in the recordings holds its plant's value in the three modifiers when next made.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYED-AS-2017"),
        station: "yalain-testkit::corpus::examine::armor_dyed_holds_its_plants_value_when_next_made",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.craft.dyeing-colors-armor",
        says: "Dyeing gives a piece of armor the plant's value in three of its appearance modifiers, in the pack and on its wearer at once; the plant is used up, the armor stays.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYEING"),
        station: "yalain-testkit::cpu::play::crafting::dyeing_gives_armor_the_plants_color_in_the_pack_and_on_its_wearer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.ends-protection",
        says: "Making a recipe ends a protection first: its end's text, the protection taken away and its \
               glow stopped, before the recipe's animation.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RECALL-PROTECTION-CRAFT"),
        station: "yalain-testkit::cpu::play::crafting::making_a_recipe_ends_a_protection_before_its_animation",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.experience-shared-out",
        says: "Craft experience goes a share to the player's own pool (a fifth for a recipe skill, a \
               hundredth and at least one point for a gathering skill) and the rest to the craft skill, \
               which rises a level whenever it holds the next step; at its cap only the pool's share is \
               paid.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-XP-SHARE"),
        station: "yalain-testkit::cpu::play::crafting::craft_experience_goes_a_share_to_the_pool_and_the_rest_to_the_craft_skill",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.experience-tables-read",
        says: "Craft skills read with their advancement, day and use tables and their pool share, and a \
               spinner recipe with its experience and difficulty tables by the spinner, as the data \
               defines them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-TABLES"),
        station: "yalain-testkit::dat::effects::craft_skills_and_spinner_recipes_read_with_their_experience_tables",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.craft.hoard-time-runs-out",
        says: "A hoard's vein is mined only while the hoard's hidden quest runs; when its five \
               minutes run out the Shaper's Hoard effect is gone, the vein refuses as mysterious, \
               and ten seconds later the player is sent out through portal space.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HOARD-TIME"),
        station: "yalain-testkit::cpu::play::mining::the_hoards_time_runs_out_and_its_miner_is_sent_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.ingredient-answered",
        says: "An ingredient put into a recipe's slot is answered with the request's values and \
               why it does not fit: 0 when it does, a recipe the player does not know, an item not \
               its own, not one the slot takes, a stack too small.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-INGREDIENT"),
        station: "yalain-testkit::cpu::play::crafting::an_ingredient_is_answered_with_why_it_does_not_fit",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.ingredient-classes",
        says: "A slot that takes a class takes every template the world makes of a package deriving from it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-CLASSES"),
        station: "yalain-testkit::cpu::play::crafting::a_slot_that_takes_a_class_takes_the_templates_deriving_from_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.knife-wears-out",
        says: "Each cut lowers the knife's durability by one at even odds and tells the client; a knife worn \
               to nothing is destroyed through overuse, said so, and leaves the pack.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KNIFE-WEAR"),
        station: "yalain-testkit::cpu::play::butchery::a_knife_loses_durability_on_about_half_its_cuts_and_breaks_at_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.made",
        says: "Making a recipe plays its animation for its cycles, lets it go, and some seconds \
               after the request carries out its outcome: the product made in the pack, its \
               effects on the player (a quest moved on), its text, the answer, then its \
               ingredients used up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-EXECUTE"),
        station: "yalain-testkit::cpu::play::crafting::making_a_recipe_plays_its_animation_then_makes_its_product_and_uses_up_its_ingredients",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.made-armor-at-its-level",
        says: "A piece of armor's rating follows the item level by where it is worn: a chest piece's whole, a leg piece's half, a smaller piece's a quarter, a robe's more than a chest piece's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MADE-ARMOR"),
        station: "yalain-testkit::cpu::play::crafting::armor_made_at_a_level_is_rated_by_where_it_is_worn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.made-item-binds-on-use",
        says: "An item a recipe's outcome binds (on the product's ordinal, the spinner chosen within \
               the action's range) is made to bind when used in combat.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BINDS-ON-USE"),
        station: "yalain-testkit::cpu::play::crafting::a_made_item_its_recipe_binds_is_marked_to_bind_when_used_in_combat",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.made-item-bonus-at-the-spinner",
        says: "An item made by a recipe whose outcome gives it an effect for its wearer carries \
               that effect at the spinner the player chose, and puts it on whoever then wears or \
               wields it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-BONUS"),
        station: "yalain-testkit::cpu::play::crafting::a_made_item_carries_its_outcomes_bonus_at_the_spinner_chosen_and_puts_it_on_when_wielded",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.made-item-kept",
        says: "What a recipe's item level gave an item and its permanent effects are kept across a logout.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-MADE-KEPT"),
        station: "yalain-testkit::cpu::entry::examine::a_made_items_own_numbers_are_kept_across_a_logout",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.made-item-own-effects",
        says: "A made item's permanent effects change it as it is made: more damage, a smaller span, less vigor, the changed lines marked; they are listed among its enchantments with their amounts.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MADE-EFFECTS"),
        station: "yalain-testkit::cpu::play::crafting::a_made_items_permanent_effects_change_its_numbers_and_show_with_their_amounts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.made-item-profiles-as-2017",
        says: "Every profile recorded of an item made at a spinner is the profile built from its template, the recipe's outcome and the item level, what differs between items of a level taken from the recording.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MADE-PROFILES"),
        station: "yalain-testkit::corpus::examine::made_items_profiles_are_built_from_their_recipes_item_level_as_2017_built_them",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.craft.made-weapon-at-its-level",
        says: "A weapon made at a level takes its name, value and least level from it and its greatest damage from the outcome's quality, and its hits are reckoned from that.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MADE-WEAPON"),
        station: "yalain-testkit::cpu::play::crafting::a_weapon_made_at_a_level_hits_for_that_levels_damage_and_needs_that_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.map-find-ends-the-recipes-wait",
        says: "A surveyor's map finding its place lets its region's five map recipes be drawn at once, the \
               five records told in one call before the find's text.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-MAP-RECIPES-RESET"),
        station: "yalain-testkit::cpu::play::maps::a_maps_find_lets_its_regions_map_recipes_be_drawn_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.map-recipes-wait-together",
        says: "Drawing a surveyor's map has all five of its region's map recipes wait five minutes, told in \
               one call after the recipe's text and before the drawn recipe's own record.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-MAP-RECIPES-SET"),
        station: "yalain-testkit::cpu::play::crafting::drawing_a_map_has_its_regions_five_map_recipes_wait_five_minutes_together",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.mining-a-maps-node-may-unearth-a-find",
        says: "A use of a resource node a surveyor's map led to may unearth a find in place of its \
               material: a rough lodestone of the material's family or a Tessera, one unit, the use \
               told so by the item's name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MINING-FIND"),
        station: "yalain-testkit::cpu::play::maps::mining_a_maps_node_sometimes_unearths_a_lodestone_in_place_of_the_ore",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.mining-gathers-a-material",
        says: "The summoner uses its node, walking to within ten meters: it plays its craft skill's mining \
               behavior and lets it go, and about two seconds later the skill pays, the use is answered \
               with the units, and they go on the player's stack; after three uses the node flashes, a \
               fourth is refused, and the node goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MINING"),
        station: "yalain-testkit::cpu::play::mining::the_summoner_mines_its_node_three_times_for_ore_and_experience",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.mining-summoner-only",
        says: "A node summoned by another player is refused, nothing gathered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MINING-SUMMONER"),
        station: "yalain-testkit::cpu::play::mining::only_the_summoner_may_mine_a_node",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.mining-units-by-surveying",
        says: "A node's units are set when it is summoned: about its summoner's surveying level and a tenth \
               of its quantity, never fewer than the quantity, spread by its variance; a rich find twice \
               that.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MINING-UNITS"),
        station: "yalain-testkit::cpu::play::mining::a_nodes_units_follow_its_summoners_surveying_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.mining-wants-a-place-in-the-pack",
        says: "A node refuses a player with no place in its pack, with the client's own answer for a \
               use wanting room; no recording shows one, and every mined item is a move that may not \
               pass the room.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-MINING-NO-ROOM"),
        station: "yalain-testkit::cpu::play::mining::a_node_refuses_a_player_with_no_place_in_its_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.needs-its-workshop",
        says: "A recipe that needs a workshop's effect is refused at once without it and made with it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-WORKSHOP-REFUSED"),
        station: "yalain-testkit::cpu::play::crafting::a_recipe_that_needs_a_workshop_is_refused_away_from_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.new-character-registry",
        says: "A new character is told its craft skills, every one at level 1, then its whole \
               craft registry with the world's starting recipes; a returning one, its registry.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-REGISTRY"),
        station: "yalain-testkit::cpu::play::crafting::a_new_character_gets_every_craft_skill_and_the_starting_recipes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.no-level-mutation-no-level",
        says: "A product whose outcome names no mutation through the item level's table stays its template.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MADE-NO-LEVEL"),
        station: "yalain-testkit::cpu::play::crafting::a_product_with_no_mutation_by_the_item_level_stays_its_template",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.nothing-made-of-an-ingredient-gone",
        says: "A recipe's ingredients are looked at again when the making ends: one put \
               down, traded or sold meanwhile makes nothing, and one stack named for two \
               slots must hold enough for both.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-GONE"),
        station: "yalain-testkit::cpu::play::crafting::an_ingredient_put_down_during_the_making_makes_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.outcome-quality-decides",
        says: "An outcome's quality decides a product's numbers: a flawed making's reads two levels below the spinner, an expert one's two above; its name stays the spinner's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MADE-QUALITY"),
        station: "yalain-testkit::cpu::play::crafting::a_flawed_making_is_two_levels_worse_and_an_expert_one_two_better",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.own-product-makes-nothing",
        says: "A recipe that works out its own product names none the data holds: it answers that it failed with a line saying so, uses up nothing and pays nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CARD-MELDS"),
        station: "yalain-testkit::cpu::play::crafting::a_recipe_working_out_its_own_product_fails_and_uses_up_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.quest-mines",
        says: "A mine a quest works refuses a player without the quest under way; with it, the mine's \
               behavior plays some eight seconds, then the quest moves on and the use is answered \
               with the units extracted, no experience; a mine its recipe works refuses every use.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-MINES"),
        station: "yalain-testkit::cpu::play::mining::a_quest_mine_gives_its_material_and_moves_its_quest_on_and_a_recipe_mine_refuses",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.raised-from-the-pool",
        says: "Raising a craft skill by asking spends from the player's pool what the skill still lacks \
               for its next level: the pool, the skill's record and its new rating are told; a raise \
               the pool cannot pay for changes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-RAISE"),
        station: "yalain-testkit::cpu::play::crafting::raising_a_craft_skill_spends_from_the_pool_what_it_lacks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.recipe-binds-read",
        says: "A recipe that binds its product reads which product it binds (by its ordinal) and \
               over which spinners.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RECIPE-BINDS"),
        station: "yalain-testkit::dat::effects::a_recipe_that_binds_its_product_reads_the_bind_on_its_ordinal",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.craft.recipe-pays-experience",
        says: "A recipe made pays the craft experience its table gives the spinner chosen (else its \
               fixed amount) times its outcome's share; the pool is told, then the craft skill's \
               record, before the answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-XP-RECIPE"),
        station: "yalain-testkit::cpu::play::crafting::a_recipe_made_pays_the_experience_of_its_spinner_and_tells_the_pool_then_the_craft_skill",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.recipe-too-far-above-the-skill-refused",
        says: "A recipe whose difficulty is further above the craft skill, with what effects add, than \
               its cutoff is refused with the answer alone. Our choice: the client greys its button \
               out, so no recording holds an answer; this one is the unknown-recipe refusal.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-CUTOFF"),
        station: "yalain-testkit::cpu::play::crafting::a_recipe_past_its_cutoff_is_refused_until_effects_lift_the_skill",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.recipes-read-from-their-definitions",
        says: "Recipes and craft skills read from their saved states with the library's names: a \n               recipe's craft skill, animation and cycles, its ingredient slots, its checks and \n               their outcomes' actions (items made, ingredients used up, effects, texts), its \n               wait; a craft skill's cap.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RECIPE-DEFS"),
        station: "yalain-testkit::dat::effects::the_recipes_read_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.craft.recorded-crafts-play",
        says: "Every recorded craft of 2017 plays through the server at its spinner: the slots it sent are the ones asked for, the answer is the recorded one, and each ingredient goes down by what the recording shows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-RECORDED-PLAY"),
        station: "yalain-testkit::corpus::play::crafts::every_recorded_craft_plays_at_its_spinner_as_recorded",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.craft.refined-products-in-the-install",
        says: "The refining recipes in the install each read as yielding their product's template, its name the recipe's own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REFINED-PRODUCTS"),
        station: "yalain-testkit::dat::effects::every_refining_recipe_reads_with_its_product",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.craft.refining-grows-with-skill",
        says: "What a refining yields grows with the craft skill to all it takes at the skill's cap, and what the player's effects add to the skill counts.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-REFINING-SKILL"),
        station: "yalain-testkit::cpu::play::crafting::a_refining_yields_more_at_a_higher_craft_skill_and_with_what_effects_add_to_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.refining-yields",
        says: "Refining adds a third of what it takes to the player's stack of its product at the craft skill's lowest levels, says how many, uses up what its outcome says and pays what the skill pays a use.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REFINING-YIELD"),
        station: "yalain-testkit::cpu::play::crafting::refining_adds_a_third_of_what_it_takes_to_the_players_stack_and_says_how_many",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.refused",
        says: "Making a recipe while not at peace (or one unknown, waiting, or short of its \
               ingredients) is refused at once with the answer alone, naming why.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-REFUSED"),
        station: "yalain-testkit::cpu::play::crafting::making_a_recipe_while_attacking_is_refused_with_the_answer_alone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.required-amount-by-the-spinner",
        says: "Too few of the amount the spinner asks for is refused with why, as the ingredient goes in and at the making.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-SPINNER-AMOUNT"),
        station: "yalain-testkit::cpu::play::crafting::too_few_for_the_spinners_amount_is_refused_with_why",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.slots-ask-craft-flags",
        says: "A slot's required and refused craft flags are tested against the item's: its template's \
               and those a recipe gave it, which stay with it. A missing or barred flag 1, 2 or 4 \
               answers its own refusal, any other 40009.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-SLOT-FLAGS"),
        station: "yalain-testkit::cpu::play::crafting::an_item_a_recipe_flagged_is_refused_by_a_slot_that_bars_the_flag",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.slots-ask-level-and-lore",
        says: "A slot asks by the spinner: an item's least level at most its level table's, refused \
               2007, or from where its lore table starts an item's Arcane Lore at most that table's \
               and no level, refused 2008. An item that takes Arcane Lore is judged by that alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-SLOT-LEVEL"),
        station: "yalain-testkit::cpu::play::crafting::a_slot_refuses_an_item_above_its_level_or_lore_at_the_spinner",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.slots-must-match-the-spinner",
        says: "A making whose slots are not exactly those the spinner asks for is refused as invalid ingredients, and nothing is used up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-SPINNER-SLOTS"),
        station: "yalain-testkit::cpu::play::crafting::the_slots_filled_must_be_the_ones_the_spinner_asks_for",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.spellbinding-makes-a-totem",
        says: "Spellbinding makes a totem of the kind of the item taken apart.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPELLBINDING"),
        station: "yalain-testkit::cpu::play::crafting::spellbinding_makes_a_totem_of_the_kind_of_the_item_it_takes_apart",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.spellbinding-takes-an-enchantment",
        says: "An item with no enchantment is not taken apart into a totem, and nothing is lost. \
               Our choice.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SPELLBINDING-EMPTY"),
        station: "yalain-testkit::cpu::play::crafting::spellbinding_refuses_an_item_with_no_enchantment",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.spinner-chooses-the-ingredients",
        says: "A recipe with an item-level spinner asks only for the ingredients whose own range holds the value chosen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-SPINNER-LIST"),
        station: "yalain-testkit::cpu::play::crafting::an_item_level_recipe_takes_only_the_ingredients_its_spinner_asks_for",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.spinner-recipes-read",
        says: "A recipe with a spinner reads with its range, each ingredient's own range and amount table, each use-up action's table, the effect it needs, and each check by the value it covers: a failure makes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-SPINNER-READ"),
        station: "yalain-testkit::dat::effects::a_spinner_recipe_reads_with_its_ranges_tables_and_needed_effect",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.craft.spinner-sets-the-amounts-used",
        says: "Each ingredient a spinner asks for is used up in the amount its action's table gives at that value.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-SPINNER-USED"),
        station: "yalain-testkit::cpu::play::crafting::a_higher_item_level_takes_more_ingredients_in_its_tables_amounts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.spinner-within-the-recipes-range",
        says: "A spinner outside the recipe's range makes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-SPINNER-RANGE"),
        station: "yalain-testkit::cpu::play::crafting::a_spinner_outside_the_recipes_range_makes_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.surveyors-map-find-moves-its-quest",
        says: "A surveyor's map found at its place makes the quest step its use triggers, with the \
               find: the starter map moves its quest on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MAP-QUEST-STEP"),
        station: "yalain-testkit::cpu::play::maps::a_map_found_at_its_place_moves_on_the_quest_it_is_part_of",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.surveyors-map-leads-to-its-place",
        says: "A surveyor's map far from its place says how far (the band's text and marker) and whether the \
               player got closer since its last bearings; within its radius the find is told, the \
               markers cleared, the map thrown away, the use answered and its region's node made.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MAP-LEADS"),
        station: "yalain-testkit::cpu::play::maps::a_surveyors_map_tells_how_far_its_place_is_and_conjures_its_node_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.surveyors-map-place-kept",
        says: "A surveyor's map's hidden place is the center of one of its rectangle's cells, chosen at \
               its first use and kept with the map.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MAP-PLACE"),
        station: "yalain-testkit::cpu::play::maps::a_maps_place_is_chosen_inside_its_rectangle_and_kept_with_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.target-asked-for",
        says: "A recipe that asks for a target is made only at an object of a template it names; at nothing or \
               anything else it is refused with the answer for that, and nothing is spent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-TARGET-2017"),
        station: "yalain-testkit::cpu::play::crafting::a_recipe_asking_for_a_target_is_made_at_one_of_its_own_and_nowhere_else",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.tool-asks-its-craft-skill",
        says: "A knife that asks more of the player's craft skill than it has is refused, and cuts nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TOOL-REFUSED"),
        station: "yalain-testkit::cpu::play::butchery::a_knife_asking_more_craft_skill_than_the_player_has_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.tool-at-its-level",
        says: "A gathering tool's table gives the craft skill it asks and a yield of five above its level; its durability is 100, an expert making's 200; it takes no name.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TOOL-LEVEL"),
        station: "yalain-testkit::cpu::play::crafting::a_tool_made_at_a_level_asks_its_craft_skill_and_yields_five_above_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.tool-wear-kept-across-sessions",
        says: "A worn tool keeps its durability across a logout: it comes back as worn as it left.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TOOL-WEAR-KEPT"),
        station: "yalain-testkit::cpu::play::butchery::a_worn_knife_keeps_its_durability_across_a_logout",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.totem-enchants-an-item",
        says: "A totem used on an item of its kind that has a free slot and the level and lore it \
               asks puts its enchantment on the item and is gone; on anything else it is refused. \
               Our choice beyond its being used up, which 2 recorded uses show.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TOTEM-USE"),
        station: "yalain-testkit::cpu::play::crafting::a_totem_used_on_an_item_of_its_kind_enchants_it_and_is_gone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.trade-goods-worth-their-materials",
        says: "A recipe's trade goods take the item level's name and are worth what went into them, 4 a unit of the first material and 2 of the second; they need no level to use.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MADE-TRADE-GOODS"),
        station: "yalain-testkit::cpu::play::crafting::trade_goods_made_at_a_level_are_named_for_it_and_worth_their_materials",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.usage-pays-less-through-the-day",
        says: "A gathering use pays its craft skill's amount for the level, three quarters of it once \
               the day's experience passes the day's mark, half past twice the mark, a quarter past \
               three times it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-XP-USAGE"),
        station: "yalain-testkit::cpu::play::crafting::a_gathering_use_pays_less_as_the_days_experience_passes_its_mark",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.veins-are-anyones-and-come-back",
        says: "A mining vein standing in the world is anyone's to mine, each use's units drawn \
               for its miner; once spent it goes, and two minutes later a new vein stands in its \
               place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HOARD-VEINS"),
        station: "yalain-testkit::cpu::play::mining::a_placed_vein_is_mined_by_anyone_and_comes_back_where_it_stood",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.workshop-effect-at-entry",
        says: "A player entering the world within a workshop's land cells has its effect at once, as a new record.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORKSHOP-ENTRY"),
        station: "yalain-testkit::cpu::play::workshops::a_player_entering_beside_a_workshop_gets_its_effect",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.workshop-effect-indoors",
        says: "Indoors a workshop's effect neither comes nor goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORKSHOP-INDOORS"),
        station: "yalain-testkit::cpu::play::workshops::indoors_a_workshops_effect_neither_comes_nor_goes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.workshop-effect-near",
        says: "A workshop puts its effect on a player inside the 3 by 3 land cells around it: cast by the workshop, held until taken off, gone at logout.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORKSHOP-EFFECT"),
        station: "yalain-testkit::cpu::play::workshops::a_workshop_puts_its_effect_on_a_player_within_one_land_cell",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.workshop-effect-off-beyond",
        says: "A workshop's effect comes off a player beyond the land cells around it, its client told.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORKSHOP-OFF"),
        station: "yalain-testkit::cpu::play::workshops::a_workshops_effect_comes_off_a_player_beyond_the_land_cells_around_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.workshop-effect-off-far",
        says: "A player gone indoors in a far part of the world loses a workshop's effect.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORKSHOP-FAR"),
        station: "yalain-testkit::cpu::play::workshops::a_player_who_goes_indoors_far_away_loses_a_workshops_effect",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.workshops-from-their-templates",
        says: "A placed object whose template names an effect for players nearby is a workshop, standing where it was placed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORKSHOP-TEMPLATES"),
        station: "yalain-testkit::cpu::play::workshops::a_placed_object_whose_template_names_an_effect_is_a_workshop",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.craft.world-holds-what-crafting-needs",
        says: "The world holds what its recipes make and ask for, and the nodes surveying conjures.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-SOURCES"),
        station: "yalain-testkit::cpu::play::crafting::crafting_asks_the_world_for_its_products_ingredients_and_conjured_nodes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.armor-debuff-in-a-special",
        says: "What a landed special's effect takes from a creature's armor counts in the next hit \
               of a special, on the whole server with the install's own skills.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPECIAL-AFTER-ARMOR-TAKEN"),
        station: "yalain-testkit::dat::skills::a_special_after_rend_armor_does_what_it_took_from_the_creature",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.creatures.armor-debuffs",
        says: "A player's hits on a creature whose armor rating an effect has lowered do a point more \
               for each point taken.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-ARMOR-DEBUFF"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_players_hits_gain_what_an_armor_debuff_took_from_the_creature",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.attack-speed-effects",
        says: "An effect slowing a creature's attacks makes it strike less often and tells its strikes \
               slower, by the share its kind takes; a hold slows them to a fifth at most.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-ATTACK-PACE"),
        station: "yalain-testkit::cpu::play::creature_tactics::an_effect_slowing_a_creatures_attacks_slows_them_by_its_share",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.attacks-due-looked-for-among-those-fighting",
        says: "Between the world's steps only the creatures fighting are looked at for an attack \
               due, however many stand in the world. Our choice.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-ATTACKS-AMONG-FIGHTING"),
        station: "yalain-testkit::cpu::play::creature_tactics::only_creatures_fighting_are_looked_at_for_an_attack_due",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.attacks-from-skill-definitions",
        says: "A creature's attacks are its skills as the data defines them: the behavior each \
               plays, whether it throws a missile, its kind, the damage it adds at the creature's \
               rating; heals and buffs a creature uses on itself are not attacks.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-SKILLS"),
        station: "yalain-testkit::dat::skills::creatures_attacks_read_from_their_skills_definitions",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.creatures.attacks-when-due",
        says: "A creature attacking from where it stands attacks as soon as its attack is due, not \
               at the world's next step.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-ATTACKS-WHEN-DUE"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_creatures_strike_comes_when_it_is_due_not_at_the_worlds_next_step",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.barrels-stand",
        says: "A barrel is made by its camp and stands where it is, never going after a player.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-BARRELS"),
        station: "yalain-testkit::cpu::play::barrels_and_turrets::a_barrel_is_made_by_its_camp_and_never_moves_or_fights",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.basic-attacks-weighted",
        says: "A creature of the generic monsters' tables draws its plain, Strong and Stronger basic \
               attacks four to two to one; any other creature draws its basic attacks evenly.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-BASIC-WEIGHTS"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_generic_monster_favors_its_plain_attack_four_to_two_to_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.buff-themselves",
        says: "A creature with a buff uses it on itself in its fight now and then, most often at its \
               first turn, and the buff's appearance shows on it while it lasts.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-BUFFS"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_creature_buffs_itself_in_its_fight",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.camp-movement-settings",
        says: "A camp's movement settings stand over its creatures' template's: it may make them \
               stationary (they neither move nor fight), stop or start their wandering and give \
               them another perception; a creature that never moves or only turns keeps to its \
               place whatever route its camp names; a setting neither gives is the game's own \
               default.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-MOVE-SETTINGS"),
        station: "yalain-testkit::cpu::play::patrols::a_camps_movement_settings_stand_over_its_creatures_templates",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.camps-keep-their-population",
        says: "Each camp of the spawn table keeps its creatures alive from the start, at its \
               spots indoors or inside its radius outdoors, and a player whose view holds them is \
               told of each.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CAMPS"),
        station: "yalain-testkit::cpu::play::creatures::a_camps_creatures_are_there_from_the_start_and_made_for_a_player_in_view",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.camps-never-make-summons",
        says: "A camp never makes a summoned creature (a pet's AI or a pet's NPC table): one that \
               lists one beside wild creatures makes only the wild ones, one of summons alone \
               makes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CAMPS-SUMMONS"),
        station: "yalain-testkit::cpu::play::creatures::a_camp_makes_its_wild_creatures_and_never_a_summoned_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.chase-around-walls",
        says: "A creature's steps never pass through walls or what stands in the world: going \
               after a player in another room it runs around them to it, through the doorways.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHASE-AROUND-WALLS"),
        station: "yalain-testkit::dat::walls::a_creature_after_a_player_in_the_next_room_runs_through_the_doorway",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.creatures.could-not-reach",
        says: "A creature going after a player that nothing leads to (its search through the walls \
               finds no way, or a closed door it cannot open stands between them) counts that \
               player as one it could not reach, for the rest of the fight; one that reaches the \
               player, through a doorway or a door it opens, counts no one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-COULD-NOT-REACH"),
        station: "yalain-testkit::dat::walls::a_creature_with_no_way_to_its_player_counts_it_as_one_it_could_not_reach",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.creatures.damage-debuffs",
        says: "A debuff a player puts on a creature that takes from its damage takes that much from \
               each of its hits while it lasts.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-DAMAGE-DEBUFF"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_damage_debuff_takes_from_a_creatures_hits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.free-range-no-home-leash",
        says: "A creature free of its home goes after a player however far from home the fight \
               takes it; one kept near home gives up past its distance from home.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FREE-RANGE"),
        station: "yalain-testkit::cpu::play::patrols::a_free_range_creature_is_never_called_home_by_its_distance_from_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.give-up-a-second-beyond-perception",
        says: "A creature gives up on a player once the player has been beyond its perception \
               for about a second, however far beyond it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-UP"),
        station: "yalain-testkit::cpu::play::creatures::a_creature_gives_up_a_second_after_the_player_leaves_its_perception",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.heal-themselves",
        says: "A creature with a heal, hurt to its warning level, heals itself at one of its turns: its \
               story on itself, then the heal's appearance and the health it gets back; not again \
               for forty seconds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-HEALS"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_hurt_creature_heals_itself_and_an_unhurt_one_does_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.healers-heal-kin",
        says: "A healer heals its most wounded kin once that one is down to two fifths of its health or \
               less.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-KIN-HEAL"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_healer_heals_its_badly_hurt_kin",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.held-stand-still",
        says: "A creature held by a player's skill stays where it is until the hold ends, though the \
               player it is after is out of its reach; freed, it runs at the player again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-HELD"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_held_creature_stays_where_it_is_until_the_hold_ends",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.held-up-not-put-back",
        says: "A creature held up at what stands in the world stands or slides along it and is \
               never put back along its way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HELD-UP-NOT-PUT-BACK"),
        station: "yalain-testkit::dat::walls::a_creature_held_up_at_what_a_world_places_is_not_put_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.creatures.idle-behaviors",
        says: "A creature of a class of behavior at rest and standing now and then plays one of its \
               two idle behaviors (the behavior and its context alone), about every seven and a \
               half minutes as chance spaces them; one without a class, or a summoned one, never \
               does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-IDLE-BEHAVIORS"),
        station: "yalain-testkit::cpu::play::resting::a_creature_at_rest_plays_an_idle_behavior_now_and_then",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.indoor-rest-in-place",
        says: "Indoors a creature at rest stays where it is: it never sets out on a walk.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-INDOOR-REST"),
        station: "yalain-testkit::cpu::play::resting::indoors_a_creature_at_rest_stays_where_it_is",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.kin-join-a-fight",
        says: "When a creature goes after a player, resting creatures of its own kind nearby may \
               come too, though the player is beyond their own notice; strangers stay put.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KIN-JOIN"),
        station: "yalain-testkit::cpu::play::creatures::resting_kin_join_a_fight_and_strangers_do_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.look-reads-near-players-only",
        says: "A creature's look for a player reads the state only of players near it, however many \
               are in the world. Our choice.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-LOOK-NEAR"),
        station: "yalain-testkit::cpu::play::creatures::a_creature_looking_reads_the_state_of_the_players_near_it_only",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.missiles-arrive-after-flight",
        says: "A creature's missile arrives after the creature's windup and its flight at 35 \
               m/s.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MISSILE-FLIGHT"),
        station: "yalain-testkit::cpu::play::missiles::a_missile_arrives_after_the_windup_and_its_flight",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.missiles-health-before-the-story",
        says: "A missile's hit sends the player's lowered health just before the story of its \
               arrival.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MISSILE-HEALTH"),
        station: "yalain-testkit::cpu::play::missiles::a_missiles_hit_sends_the_players_health_before_the_story",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.missiles-stand-and-shoot",
        says: "A creature with a basic missile attack that engages a player from 15 m or more \
               stands and shoots: each shot a story from the creature naming the skill and a \
               missile hook at the player, no missile object made; each arrival a story the \
               player tells, naming the creature as the attacker, with one result hook.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MISSILES"),
        station: "yalain-testkit::cpu::play::missiles::a_creature_with_missiles_stands_and_shoots_and_its_target_tells_each_arrival",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.not-wandering-never-wander",
        says: "A creature whose template says it does not wander never wanders.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WANDER-FLAG"),
        station: "yalain-testkit::cpu::play::resting::a_creature_whose_template_says_it_does_not_wander_never_wanders",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.notice-and-close-in",
        says: "A creature that attacks first notices a player within its perception radius and \
               runs at it, its places reaching the player's client each nearer than the last, \
               until it is within reach.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AI-NOTICE"),
        station: "yalain-testkit::cpu::play::creatures::a_creature_notices_a_player_within_its_perception_and_comes_at_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.pass-what-creatures-pass",
        says: "A creature's body passes through what its description says, as the client's objects \
               do: scenery marked for creatures to pass and any new mover's type, and no more.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-MASK"),
        station: "yalain-testkit::dat::walls::a_creature_goes_through_scenery_marked_for_creatures_to_pass",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.creatures.patrol-returns-to-nearest-waypoint",
        says: "A creature on a route that gives up a fight runs back to the route's nearest \
               waypoint, not to where its camp made it, and carries on along the route from there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PATROL-RETURN"),
        station: "yalain-testkit::cpu::play::patrols::a_creature_on_a_route_that_gives_up_a_fight_goes_back_to_the_nearest_waypoint",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.patrol-walks-indoors",
        says: "A route is walked indoors, where a creature at rest otherwise stands, and by a \
               creature whose template says it does not wander.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PATROL-INDOORS"),
        station: "yalain-testkit::cpu::play::patrols::a_route_is_walked_indoors_and_by_a_creature_that_does_not_wander",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.patrol-walks-its-route",
        says: "A creature whose camp names a route walks to each waypoint in turn, stands at it \
               as long as the waypoint's rest, and turns back at the end of a back-and-forth \
               route or goes on round a loop.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PATROL-ROUTES"),
        station: "yalain-testkit::cpu::play::patrols::a_creature_on_a_route_walks_to_each_waypoint_rests_there_and_turns_back_at_the_end",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.practice-dummy-at-30",
        says: "The practice dummy of level 30 sends back by the same line as the others, 0.30. Our choice: it \
               shows the effect's appearance, and no recording holds an attack on it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PRACTICE-DUMMY-30"),
        station: "yalain-testkit::cpu::play::watchers::the_weakest_practice_dummy_goes_by_the_same_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.practice-dummy-sends-back",
        says: "A practice dummy holds an effect that sends back a share of each attack from its first moment: \
               0.35 at level 40, 0.40 at 50, 0.45 at 60, 0.50 at 70. The dummy on another kind of table holds \
               none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PRACTICE-DUMMY"),
        station: "yalain-testkit::cpu::play::watchers::a_practice_dummy_sends_back_a_share_of_each_attack_by_its_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.protectors-buff-kin",
        says: "A protector buffs a kin of its in the same fight, most often at its first turn and often \
               again at its next, and a heal's or buff's hook takes a point of health one time in two.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-KIN-BUFF"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_protector_buffs_its_kin_in_a_fight",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.redraw-by-harm-and-nearness",
        says: "A creature that players have hurt lately draws its target again now and then among them \
               by the harm each did it, the nearest three times as likely.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-REDRAW"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_creature_draws_its_target_by_harm_and_nearness",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.settled-never-wander",
        says: "A creature whose homesick radius is under 5 m never wanders.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WANDER-SETTLED"),
        station: "yalain-testkit::cpu::play::resting::a_creature_with_a_homesick_radius_under_five_meters_never_wanders",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.skills-add-damage",
        says: "What a creature's skill adds to its hit goes in before the player's armor: a \
               level 1 player takes a plain hit and the skill's add in full.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-ADD"),
        station: "yalain-testkit::cpu::play::rules::a_creatures_skill_adds_its_damage_before_the_players_armor",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.slowed-run-slower",
        says: "A slowed creature runs at its own speed times the slow its effects put on it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-SLOWED"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_slowed_creature_runs_at_its_speed_times_the_slow",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.specials-barred-forty-seconds",
        says: "A creature's special is told without the basic-attack flag, and once used it \
               waits 40 seconds before the creature uses it again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPECIAL-BARRED"),
        station: "yalain-testkit::cpu::play::missiles::a_creatures_special_waits_forty_seconds_before_it_is_used_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.spots-hold-one",
        says: "A camp's next creature appears at a spot none of its living creatures holds, while one \
               is free, so a camp as many as its spots fills every one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CAMP-SPOTS-HOLD-ONE"),
        station: "yalain-testkit::cpu::play::creatures::a_camp_as_many_as_its_spots_fills_every_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.stuck-in-something-put-out",
        says: "A creature that stands in something and can move no way at all is put where it \
               fits nearest when it next steps. Our choice: 2017's creatures were placed where \
               they fit when they appeared, and a world's spots are rounded.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STUCK-PUT-OUT"),
        station: "yalain-testkit::dat::walls::a_creature_standing_in_something_is_put_where_it_fits_and_walks_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.creatures.turn-on-the-most-harm",
        says: "A creature fighting one player that has not hurt it lately turns on another that has, at \
               one of its next attacks.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-TURNS"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_creature_turns_on_the_player_that_has_hurt_it_most_lately",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.turrets-roused-by-their-guards",
        says: "A turret never shoots first; once a creature near it fights, it shoots the nearest \
               player in reach from where it stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-TURRETS"),
        station: "yalain-testkit::cpu::play::barrels_and_turrets::a_turret_shoots_from_where_it_stands_once_a_creature_near_it_fights",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.unharmed-turn-to-another",
        says: "A creature that no player has hurt lately turns now and then to another player within \
               its perception, more often to a nearer one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-UNHARMED-TURN"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_creature_no_one_hurts_turns_now_and_then_to_another_player_near_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.wander-and-stop-standing",
        says: "At rest a creature walks now and then to a point within its wandering range of \
               home; a walk, or a run home, that ends (on the ground, within half a meter) is \
               told with its place once more, standing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STOP-STANDING"),
        station: "yalain-testkit::cpu::play::resting::a_creature_that_runs_home_is_told_standing_when_it_gets_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.way-searches-share-a-budget",
        says: "However many creatures are held up, a pass of the world spends a fixed number of moves \
               on searching for ways around; the searches left over go on at later passes in turn, \
               so each creature held up is answered. Our choice.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WAY-SEARCH-BUDGET"),
        station: "yalain-testkit::dat::walls::a_pass_of_the_world_spends_no_more_than_its_budget_on_way_searches",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.creatures.weapon-drawn-slider",
        says: "A creature drawing or putting away its weapons moves its drawn slider, as a player \
               does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-WEAPON-SLIDER"),
        station: "yalain-testkit::cpu::play::creature_weapons::a_creature_moves_the_drawn_slider_as_it_draws_and_puts_away_its_weapons",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.weapon-objects",
        says: "A creature's weapons are objects of their own: made hanging from it as it goes after a \
               player, let go of as it gives up, attached again as it fights again, gone with it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-WEAPONS"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_creature_draws_its_weapons_as_objects_hung_from_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.weapon-pairs-by-share",
        says: "Some creatures asked to hold two swords are given one, at a share the world sets, \
               and fight with the one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-WEAPON-PAIRS"),
        station: "yalain-testkit::cpu::play::creature_weapons::a_creature_asking_two_swords_is_given_two_at_the_listed_share_and_fights_as_it_holds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.weapon-stance",
        says: "A creature that wields a weapon takes the weapon's combat stance as it goes after a \
               player, as a player does: with a shield, two at once, or the weapon's own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-STANCE"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_creature_takes_its_weapons_stance_as_it_goes_after_a_player",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.weapon-stance-is-the-held-weapons",
        says: "A creature fights in the stance of the weapon it holds, alone, with a shield or two \
               at once: the weapon's own stance, else its kind's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-WEAPON-STANCE"),
        station: "yalain-testkit::cpu::play::creature_weapons::a_creatures_stance_is_its_weapons_own_else_the_databases_else_its_kinds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.creatures.weapon-taken-up-again",
        says: "A creature taking its weapons up again tells a client that knew them, weapon by \
               weapon, that no one holds or wields it, that the creature wields and holds it, and \
               then that it is attached.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-WEAPONS-AGAIN"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_creature_taking_its_weapons_up_again_wields_and_holds_each_before_attaching_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.data.answers-never-hold-up-the-world",
        says: "Answering requests for data records never holds up the world: a very long \
               record is compressed once before the world runs, what else answers compress \
               has a budget for each connection and for the world, and a connection has \
               only so much waiting to be sent.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-DATA-ANSWERS"),
        station: "yalain-testkit::cpu::entry::data_download::a_very_long_record_is_compressed_once_before_the_world_runs",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.data.landblock-brings-its-information",
        says: "A landblock sent to a client that asked for it brings its landblock information \
               right after it, unasked, from the same file.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DATA-LANDBLOCK"),
        station: "yalain-testkit::cpu::entry::data_download::a_landblock_sent_brings_its_landblock_information_right_after_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.data.recorded-requests-answered-as-2017",
        says: "Every data record the January 2017 clients asked for, asked of the server with the \
               install's own data files, is answered as the 2017 server answered it: the same \
               record from the same file, or the same refusal; with the landblock information \
               it sent unasked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DATA-RECORDED"),
        station: "yalain-testkit::corpus::replay::every_recorded_2017_data_request_is_answered_as_the_recorded_server_answered_it",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.data.records-served",
        says: "A data record the server's files hold is sent to the client that asked for it, \
               from the file its kind lives in, with that file's name, the kind and the id, \
               compressed when that is smaller and as it is otherwise.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DATA-SERVED"),
        station: "yalain-testkit::cpu::entry::data_download::a_held_record_is_sent_from_its_kinds_file_compressed_only_when_that_is_smaller",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.data.requests-refused",
        says: "A request for a data record the server's files do not hold is refused with the \
               record's kind, its id and error 1.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DATA"),
        station: "yalain-testkit::cpu::entry::data_download::a_record_the_server_does_not_hold_is_refused_with_its_kind_id_and_error_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.back-at-entry",
        says: "A character that left the world dead, by logging off, its client going away or the \
               server stopping, comes back by itself as it enters: at the lifestone it is bound \
               to, described at no health, then with five vitae less, three quarters of its health \
               and vigor, protected, set standing and playing its return where it stands. It is \
               told of no death, shown no box, and asks for nothing. Our choice: the one recording behind \
               this is a death during a character's log-off, and logging off with the box up is \
               unrecorded.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-DEAD-AT-ENTRY"),
        station: "yalain-testkit::cpu::play::dying::a_character_that_logged_off_dead_comes_back_alive_at_its_lifestone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.blow-before-kill-text",
        says: "A creature's strike is told by its story first; the player loses the health as the \
               blow reaches it, most of a second later for a basic attack, a special by its own \
               time, and the kill text follows the health in that instant. A player gone into \
               portal space or dead by then is not hurt by it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BLOW-LANDS"),
        station: "yalain-testkit::cpu::play::dying::a_creatures_killing_blow_is_told_before_the_health_and_the_kill_text",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.corpse-and-leaving",
        says: "Two seconds after its death a creature's corpse appears where it fell, holding \
               what its loot profile leaves; three seconds later the creature leaves the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CORPSE"),
        station: "yalain-testkit::cpu::play::fights::a_dead_creature_leaves_a_corpse_with_its_loot_and_then_leaves",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.corpse-settles",
        says: "A creature falls in one of six whole death poses; its corpse's object lies in it \
               (the pose's slider left out for the first pose), with visual stamp 10, and in the \
               same tick come its scale, its two effects, its pose, its mode, nobody's claim, its \
               killer, the creature it was and its name, their stamps 2 to 10.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CORPSE-SETTLES"),
        station: "yalain-testkit::cpu::play::fights::a_corpse_lies_in_the_death_pose_and_is_followed_by_its_settling",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.corpse-settles-for-newcomers",
        says: "A corpse made for a client whose view it comes into after it appeared is followed by \
               the same changes it was made up of, stamps 2 to 10, its name last.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CORPSE-BLOCK-LATER"),
        station: "yalain-testkit::cpu::play::fights::a_corpse_seen_later_is_followed_by_its_settling_too",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.experience-in-parts",
        says: "A kill's experience is paid in parts in the order its damagers first hurt it, each \
               the creature's experience times the health that damager took over all the health \
               taken (none past its death), rounded to the nearest: a player's and its pet's to \
               the player, each with \"You have gained %1 experience points!\" and the totals; \
               another creature's to no one; then \"You have killed the %1.\".",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-XP-PARTS"),
        station: "yalain-testkit::cpu::play::experience::a_kills_experience_is_paid_in_parts_by_the_health_each_damager_took_a_pets_to_its_owner",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.lifestone-protection",
        says: "A resurrected player is protected for three minutes: the protection added on its \
               client (its record: the protection's saved state, three minutes) as it is \
               resurrected; it ends with \"You are no longer protected by the Lifestone's \
               Magic.\", the protection taken away and its glow stopped, when its time is up, \
               when the player attacks, or when it comes out of portal space elsewhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LIFESTONE-PROTECTION"),
        station: "yalain-testkit::cpu::play::dying::a_resurrected_player_is_protected_for_three_minutes_and_creatures_leave_it_be",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.made-again-lying",
        says: "A corpse made again for a client (asked for again, or come into its view) lies as \
               it was first made; a dead creature made again before it leaves is in its dead mode \
               and pose.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CORPSE-AGAIN"),
        station: "yalain-testkit::cpu::play::fights::a_corpse_or_dead_creature_made_again_lies_in_its_pose",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.no-fair-fight",
        says: "A kill by a player the creature could not reach is no fair fight: the killer hears \
               \"You receive no experience, since that wasn't a fair fight.\" and then that it \
               killed the creature; its part is paid to no one, the killer or its fellows, and the \
               kill moves no quest on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-NO-FAIR-FIGHT"),
        station: "yalain-testkit::cpu::play::fellowships::a_kill_by_a_player_the_creature_could_not_reach_pays_no_one_and_moves_no_quest_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.nothing-worth-sends-nothing",
        says: "A part of a kill worth nothing (the creature worth no experience, or the part \
               rounding to nothing) sends no text and no totals.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-XP-NOTHING"),
        station: "yalain-testkit::cpu::play::experience::a_part_worth_nothing_sends_no_text_and_no_totals",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.player-corpse-settles",
        says: "A player's corpse is followed, as a creature's is, by its scale, its two effects, its \
               pose, its mode, nobody's claim, its killer, the player it was and its name, stamps 2 \
               to 10.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PLAYER-CORPSE-BLOCK"),
        station: "yalain-testkit::cpu::play::dying::a_players_corpse_is_followed_by_its_settling_as_a_creatures_is",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.player-dies",
        says: "A player killed by a creature: in one tick its death message and, for all who see \
               it, dead, its killer announced, unselectable, posed, playing its death; ten \
               seconds later its client is told it is dead and an empty corpse that looks like \
               it is made where it fell.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PLAYER-DEATH"),
        station: "yalain-testkit::cpu::play::dying::a_player_killed_by_a_creature_dies_and_its_corpse_is_made_ten_seconds_later",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.resurrection",
        says: "A dead player that asks to be resurrected goes through portal space to its \
               lifestone and is alive again a second and a half later, with three quarters of \
               its health and vigor.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RESURRECT"),
        station: "yalain-testkit::cpu::play::dying::a_dead_player_is_resurrected_with_three_quarters_of_its_health",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.resurrection-ends-effects",
        says: "As a resurrection is asked for, or a character that left the world dead comes back, \
               the records of effects a death ends are taken off, before the protection is added. \
               A worn item's record and those of other effects stay. A death itself takes nothing \
               off. Everyone who sees the player sees it set standing, what was taken off stop \
               showing, and it glow.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RESURRECTION-ENDS-EFFECTS"),
        station: "yalain-testkit::cpu::play::dying::a_resurrection_takes_off_what_a_death_ends_and_keeps_the_rest",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.resurrection-look",
        says: "A resurrected player is set standing at once as it goes, not played into it, then \
               shows the resurrection's glow and the arrival's effect, for everyone who sees it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RESURRECTION-LOOK"),
        station: "yalain-testkit::cpu::play::dying::a_resurrected_player_is_set_standing_then_glows_as_it_goes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.sent-back-after-ten-minutes",
        says: "A player that sits dead with its box up and asks for nothing is resurrected ten \
               minutes after it was told it was dead, by the usual trip. This rests on one \
               recording: the others all asked sooner.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SENT-BACK"),
        station: "yalain-testkit::cpu::play::dying::a_player_left_dead_ten_minutes_is_sent_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.stops-where-it-falls",
        says: "A creature that dies is told once more where it fell, standing still, after its \
               death behavior.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DEATH-STOP"),
        station: "yalain-testkit::cpu::play::fights::a_dying_creature_is_told_standing_where_it_fell_after_its_death",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.unfair-part-held-back",
        says: "The part of a kill of a damager the creature could not reach is held back alone, \
               and the player it was for (the damager, or its pet's owner) told once \"You receive \
               no experience, since that wasn't a fair fight.\" in its place among the parts; the \
               other parts are paid.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-UNFAIR-PART"),
        station: "yalain-testkit::cpu::play::experience::the_part_of_a_damager_the_creature_could_not_reach_is_held_back_and_the_others_paid",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.death.vitae",
        says: "Vitae: none lost below level 6, else at least 100 less 3 a level between 60 and \
               99; the experience to raise it by one is the level's cost times the vitae's \
               share, rounded, at least 1.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-VITAE"),
        station: "yalain-testkit::cpu::play::dying::vitae_has_its_floor_by_level_and_its_cost_to_raise",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.buff-record-runs-out",
        says: "A buff puts a record on its target at the skill's hook, with its own id, its \
               timeout and spellcraft at the skill's rating, timed; the player's speed sent when \
               it changes it, its appearance shown; at its cast time plus its timeout it is taken \
               off, the speed sent again, the appearance stopped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EFFECT-RECORDS"),
        station: "yalain-testkit::cpu::play::effects::a_buff_is_a_record_that_runs_out_on_its_own_clock",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.counter-answers-an-attack",
        says: "A counter a player holds answers an attack that reaches it with its effect on the \
               attacker, creature or player, by its chance at its spellcraft; harm over time is no \
               attack, and an attack evaded goes unanswered. Our choice.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-COUNTER"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_counter_answers_a_strike_that_lands_on_its_holder_and_not_one_evaded",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.counters-and-wards-read",
        says: "The counters read with their chance and the effect they answer with; the wards read with \
               the classes of effect they keep off.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-COUNTER-READ"),
        station: "yalain-testkit::dat::effects::the_counters_and_wards_read_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.effects.creature-debuff-shows-until-it-runs-out",
        says: "A debuff a player's skill lands on a creature shows its appearance to those who \
               see it from the add until its duration ends, and changes the creature's numbers \
               while it runs.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-DEBUFF"),
        station: "yalain-testkit::cpu::play::creature_effects::a_debuff_on_a_creature_shows_until_it_runs_out_and_changes_its_numbers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.creature-debuffs-stack",
        says: "The same effect landed on a creature while it runs is a second record: its \
               appearance is not shown again and stops only as the later record runs out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-STACK"),
        station: "yalain-testkit::cpu::play::creature_effects::a_second_debuff_keeps_the_appearance_until_the_later_one_runs_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.creature-effects-read-from-their-definitions",
        says: "The effects skills put on creatures read from their saved states: debuffs, \
               holds, damage over time, links, combinations with what they look for and add, \
               and auras with their pulsed effects, whom they reach and their range.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-EFFECT-DEFS"),
        station: "yalain-testkit::dat::effects::the_effects_on_creatures_combinations_and_auras_read_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.effects.creature-over-time",
        says: "A damage over time on a creature is told by the creature as a story of its \
               caster's with one hook and the health lost, the first within ten seconds or so \
               of the add, then every 5.6 to 11.3 seconds, the last as it runs out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-OVER-TIME"),
        station: "yalain-testkit::cpu::play::creature_effects::a_damage_over_time_on_a_creature_ticks_as_the_creature_tells_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.creature-over-time-after-death",
        says: "A creature killed while a damage over time runs on it still tells its ticks until it runs \
               out, and they change nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-DEATH"),
        station: "yalain-testkit::cpu::play::creature_effects::a_dead_creatures_damage_over_time_runs_on_and_changes_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.creature-speed",
        says: "A hold on a creature sends it a speed of nothing as it lands and its speed again \
               as it ends, before the appearance stops; a scale the effect does not change is \
               not sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CREATURE-SPEED"),
        station: "yalain-testkit::cpu::play::creature_effects::a_held_creature_is_sent_no_speed_and_its_own_again_when_the_hold_ends",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.heal-at-the-hook",
        says: "A heal aimed at a creature or nothing heals its user, with no movement order; its \
               story is the skill's behavior on its user; at the skill's hook a health heal is \
               told by the healed one, then its health; a vigor heal sends the vigor alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HEALS"),
        station: "yalain-testkit::cpu::play::effects::a_heal_aimed_at_nothing_heals_its_user_at_the_skills_hook",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.jump-scale-told",
        says: "A record that changes a player's jump tells its jump scale to all who see the \
               player as it comes and again as it goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EFFECT-JUMP"),
        station: "yalain-testkit::cpu::play::effects::a_record_changing_the_jump_tells_the_jump_scale_as_it_comes_and_goes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.kept-across-sessions",
        says: "Effects still running when a character leaves come back in its next session start, \
               in its effect registry with the id counter, and run out on the game's clock; one \
               that ran out while it was away does not come back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EFFECTS-KEPT"),
        station: "yalain-testkit::cpu::play::effects::an_effect_still_running_comes_back_in_the_session_start_and_runs_out_on_the_games_clock",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.over-time",
        says: "An effect that changes health over time does so every few seconds, by its rate \
               times the seconds since its last change, each change told by the healed one before \
               its health.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EFFECTS-OVER-TIME"),
        station: "yalain-testkit::cpu::play::effects::a_change_over_time_heals_every_few_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.protection-keeps-off-its-classes",
        says: "A protection in the data keeps off effects of the classes its list names, silently, by its chance \
               at its record's spellcraft, the top of its class drawing; effects of other classes come on, \
               and an aura's pulse is asked the same.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PROTECTION"),
        station: "yalain-testkit::cpu::play::watchers::a_protection_keeps_off_what_it_names_by_its_chance_at_its_spellcraft",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.protections-read",
        says: "The protections read with their lists and chances, and no skill or aura lists a protection before \
               an effect it would keep off.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PROTECTION-READ"),
        station: "yalain-testkit::dat::effects::the_protections_read_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.effects.quest-rewards",
        says: "A quest step's effects are put on the player after its line and gifts, cast by the \
               player at spellcraft 1, then its text, then the update; a blessing that raises a \
               quality raises it for the server's rules too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-EFFECTS"),
        station: "yalain-testkit::cpu::play::effects::a_quest_steps_effects_come_after_its_gifts_and_before_its_update",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.read-from-their-definitions",
        says: "Effects read from their saved states with the library's names, by the package each \n               derives from: an instant change to vitals and its amount by spellcraft, changes to \n               qualities (added or multiplying), chains of them, a quest moved on, a recipe given \n               or taken; each with its duration, class, appearance and whether a death ends it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EFFECT-DEFS"),
        station: "yalain-testkit::dat::effects::the_effects_read_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.effects.recalls-read",
        says: "The recalls' effects read where they go: the lifestone's and Portal Recall's to the places a \
               player keeps, each city's to its place by name (told, in its own table), Arramora's \
               untold, Allegiance Recall's to the allegiance's hall; a city recall's protection lasts \
               30 s with the lifestone's glow.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RECALL-EFFECTS"),
        station: "yalain-testkit::dat::effects::the_recalls_read_where_they_go",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.effects.records-stack",
        says: "The same effect put on twice is two records with ids counting on; neither replaces \
               nor refreshes the other, and each runs out at its own time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EFFECTS-STACK"),
        station: "yalain-testkit::cpu::play::effects::the_same_effect_twice_is_two_records_each_running_out_on_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.replies-read",
        says: "Plan B, Immolation and Taunt Force read as answering what comes at them, of any class and always; \
               Plan B and Immolation with fire over time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REPLIES-READ"),
        station: "yalain-testkit::dat::effects::the_effects_that_answer_what_comes_at_them_read_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.effects.reply-to-an-effect",
        says: "What holds a reply answers a harmful effect from another with its effect on the sender, at the \
               reply's spellcraft; a help, its own effect and an answer are not answered, so two holders can't \
               go back and forth. Our choice: no recording holds one.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-REPLY-TO-AN-EFFECT"),
        station: "yalain-testkit::cpu::play::watchers::a_holder_answers_a_harmful_effect_on_its_sender_and_an_answer_is_not_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.self-cast-lists",
        says: "A skill used on oneself names its effects in its list for its user: Immolation's protection and \
               Bladeskin come on by using them. The plain Bladeskin's aura pulses no Blocker.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SELF-CAST-LISTS"),
        station: "yalain-testkit::dat::effects::immolation_and_bladeskin_put_their_effects_on_their_user",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.effects.speed-told-at-each-speed-record",
        says: "A player and those who see it are told its speed and acceleration whenever an \
               effect that changes them comes or goes, even when the speed stays the same.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPEED-EACH-RECORD"),
        station: "yalain-testkit::cpu::play::effects::speed_is_told_at_every_speed_records_coming_and_going_even_unchanged",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.ward-keeps-off-its-classes",
        says: "A ward a player holds keeps off effects of its classes by its chance, and no others. Our \
               choice.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WARD"),
        station: "yalain-testkit::cpu::play::creature_tactics::a_ward_keeps_off_the_effects_of_its_classes_by_its_chance",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.watcher-absorbs",
        says: "A record that takes from attacks on its holder lessens what the attack does, told so: each of \
               its class's top records, the higher classes first, draws its chance and takes its share \
               of the harm rounded up, or its amount. All of it taken tells a hit and no change; more \
               than all heals. Harm an effect does is no attack.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ABSORBS"),
        station: "yalain-testkit::cpu::play::watchers::a_swing_at_a_creature_holding_one_is_lessened_and_sent_back_at_the_swinger",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.watcher-sends-back",
        says: "A record that sends harm back tells the attacker's health change in the hook of the attack it \
               answers, its share of the harm rounded up, by its chance; the attacker loses it as the \
               attack lands, creature or player, and may die of it, the holder then its killer. An \
               attack evaded, or an effect's harm, sends nothing back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SENDS-BACK"),
        station: "yalain-testkit::cpu::play::watchers::a_strike_on_a_holder_tells_what_goes_back_and_the_creature_loses_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.watcher-sends-back-at-a-player",
        says: "What goes back at a player's attack on another player is multiplied by the record's figure \
               for that, and told as PK damage too. Our choice for the telling: no recording holds one.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SENDS-BACK-PK"),
        station: "yalain-testkit::cpu::play::watchers::a_players_attack_sends_back_its_share_for_players_told_as_pk_damage",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.effects.watchers-read",
        says: "The records that watch attacks read with what they take and send back: chances, amounts along \
               a line or the same at any spellcraft, shares or amounts, and the figure against a player.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WATCHERS-READ"),
        station: "yalain-testkit::dat::effects::the_watchers_read_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.equip.own-requirements-refuse",
        says: "A treasure item whose template names no requirement is refused by its own: below \
               its own least level 10009, then below its own Arcane Lore (the player's Arcane \
               Lore skill's rating) 10050; with both met it is put on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EQUIP-OWN-REQUIREMENTS"),
        station: "yalain-testkit::cpu::play::wearing::a_treasure_chest_piece_is_refused_by_its_own_least_level_then_its_own_arcane_lore",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.equip.requirements-refuse",
        says: "An item used to put it on is refused, its answer alone with the use's flags 316 and \
               nothing put on, when its template names a race the player is not of (10015), a skill \
               the player does not hold (10013), a least level above the player's (10009), or a \
               quest not where it needs it (a hidden one's as mysterious, 10043), in that order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EQUIP-REQUIREMENTS"),
        station: "yalain-testkit::cpu::play::wearing::a_chest_piece_whose_race_skill_level_or_quest_the_wearer_lacks_is_refused_in_that_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.equip.requirements-refuse-directives",
        says: "An item dragged onto the paper doll that the player may not put on is answered as \
               the directive it asked for, at the place named and the item's slot, with the refusal \
               for its status, and stays in the pack.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EQUIP-REQUIREMENTS-DIRECTIVE"),
        station: "yalain-testkit::cpu::play::paper_doll::a_ring_for_another_race_dragged_to_the_doll_is_refused_and_stays_in_the_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.equip.wielded-effects-come-on-and-off",
        says: "A weapon whose template names effects when wielded puts them on as it is wielded and \
               takes them off as it is put away, by its use or to make room for another.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WIELDED-EFFECTS"),
        station: "yalain-testkit::cpu::play::worn_effects::a_wielded_weapon_puts_its_effect_on_and_putting_it_away_takes_it_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.equip.worn-effects-come-on-and-off",
        says: "An item worn puts its effects on its wearer just before the answer, each a record \
               held until it comes off, with a line saying the effect applies; taken off, its \
               effects go just before that answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORN-EFFECTS"),
        station: "yalain-testkit::cpu::play::worn_effects::a_worn_item_puts_its_effects_on_before_the_answer_and_takes_them_off_before_the_take_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.equip.worn-effects-read-from-their-names",
        says: "The effects items put on whoever wears or wields them read from the names their \
               templates give, at the template's spellcraft, as the data defines them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORN-EFFECT-NAMES"),
        station: "yalain-testkit::dat::effects::the_effects_items_put_on_their_wearers_read_from_their_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.equip.worn-effects-stay-through-a-death",
        says: "An item's effects stay on its wearer through a death, even those a death ends when \
               cast as buffs.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORN-DEATH"),
        station: "yalain-testkit::cpu::play::worn_effects::an_items_effects_stay_on_through_a_death_that_ends_a_buff_of_the_same_effect",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.an-inscription-is-shown",
        says: "An inscribed item's profile shows, on a tab of its own, who wrote on it by its \
               name in the world, and the words.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-INSCRIPTION-SHOWN"),
        station: "yalain-testkit::cpu::entry::examine::an_inscribed_item_shows_who_wrote_it_and_the_words_when_examined",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.an-items-icon-takes-its-appearance",
        says: "An item's icon is its look's with what its appearance values give it through their \
               appearance tables, base tables first: a dye's color, a potion's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ICON-APPEARANCE"),
        station: "yalain-testkit::cpu::entry::examine::a_dyed_items_icon_takes_its_dyes_color",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.destroyed-on-use-by-kind",
        says: "Keys, glyphs, potions and the other kinds whose class makes them so say they are \
               destroyed on use, even when their template does not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-DESTROYED-KIND"),
        station: "yalain-testkit::cpu::entry::examine::a_potions_profile_says_it_is_destroyed_on_use_by_its_kind",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.item-effect-spellcraft-shown",
        says: "An item's profile lists only the effects a player is ever shown, and shows the \
               spellcraft one is cast with unless it happens at once or changes the same \
               whatever the spellcraft.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-EFFECT-SPELLCRAFT"),
        station: "yalain-testkit::cpu::entry::examine::an_effects_spellcraft_shows_unless_it_happens_at_once_or_never_varies",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.item-maker-durability-effects",
        says: "An item's profile names who made it after its description, gives its durability \
               first among its stats, and lists its effects by when they apply, each with its \
               name and description and the spellcraft it is cast with; a made item's name is \
               drawn as a made item's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-LINES"),
        station: "yalain-testkit::cpu::entry::examine::an_item_names_its_maker_its_durability_and_its_effects",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.item-maker-kept-across-sessions",
        says: "A made item kept in the pack still names its maker after its holder logs out and \
               back in.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-MAKER-KEPT"),
        station: "yalain-testkit::cpu::entry::examine::a_made_items_maker_is_named_again_after_a_logout",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.item-profiles-as-2017",
        says: "Examining an item, wherever it is, is answered with its profile: its icon (its look's icon layers) or picture, its name (a stack's with its count), its value, its description; a weapon's damage, combat delay and vigor cost, armor's rating, delay and places; what using it does to it and the level and races it needs. 2017's profiles of items made from templates are built from the templates byte for byte.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EXAMINE-ITEM"),
        station: "yalain-testkit::corpus::examine::item_profiles_are_built_from_their_templates_as_2017_built_them",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.examination.lines-by-tab",
        says: "A profile keeps its lines in tab order (in the order added within a tab), only for \
               the tabs the client asked for and the general ones, and notes every tab any line \
               belongs to.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EXAMINE-TABS"),
        station: "yalain-testkit::cpu::entry::examine::a_profile_keeps_lines_in_tab_order_for_the_tabs_asked_and_notes_every_tab",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.looked-at-again-while-examined",
        says: "While a player examines something, it is looked at again every 3.5 seconds and \
               its profile sent again only when it changed; a request about something else or \
               a stop sent later ends that, a stop sent with the request does not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EXAMINE-AGAIN"),
        station: "yalain-testkit::cpu::entry::examine::an_examined_item_is_sent_again_when_it_changes_until_a_later_stop",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.monarch-only-object",
        says: "A placed object for monarchs alone says so on its requirements tab.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MONARCH-PROFILE"),
        station: "yalain-testkit::cpu::entry::examine::an_object_for_monarchs_alone_says_so_when_examined",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.monster-profiles-as-2017",
        says: "A monster's profile, for a player who can attack it, is the one the 2017 servers \
               sent: its name and description, level and kingdom from its template, its health and \
               vigor, and the chance to hit it, apart from the names and levels the server set at \
               run time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EXAMINE-MONSTER"),
        station: "yalain-testkit::corpus::examine::monster_profiles_are_built_from_their_templates_as_2017_built_them",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.examination.player-profile",
        says: "Examining a player is answered with its profile: the request repeated, then its \
               name, its first craft title, its race and sex, its level, its kingdom, its health \
               and vigor, its kingdom points and its quest rating; a request to stop examining is \
               not answered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EXAMINE-PLAYER"),
        station: "yalain-testkit::cpu::entry::examine::examining_a_player_is_answered_with_its_profile_and_stopping_is_not_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.player-quest-rating",
        says: "A player's profile names the quest rating the quests it has completed added.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-RATING-SHOWN"),
        station: "yalain-testkit::cpu::entry::examine::examining_a_player_names_its_quest_rating",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.portal-recall-line",
        says: "A portal examined says what may be recalled to it, as its flags say: recall and summon, \
               recall alone, or neither.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PORTAL-RECALL-LINE"),
        station: "yalain-testkit::cpu::play::recalls::a_portals_recall_line_is_what_its_flags_allow",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.examination.skills-needed-listed",
        says: "An item that takes a skill or Arcane Lore to use lists them under its requirements: \
               each skill by name, then the Arcane Lore needed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-SKILLS-NEEDED"),
        station: "yalain-testkit::cpu::entry::examine::an_item_lists_the_skills_and_arcane_lore_it_takes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.experience.from-level-50-the-fall-comes-sooner",
        says: "From level 50 what is below a player pays in full only to four levels below it, then 20% less a \
               level, at least 1%, for a kill and for a completion by its challenge; at level 49 it pays as at any \
               lower level. Vitae has no part in it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-XP-FROM-LEVEL-50"),
        station: "yalain-testkit::cpu::play::rules::from_level_50_a_kill_pays_in_full_only_to_four_levels_below_the_killer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.experience.solo-kill-award",
        says: "A solo kill by a killer under level 50 pays the creature's death experience in full from ten \
               levels below the killer to nine above, 15% less a level further below (at least 1%), 20% less a \
               level further above, rounded down.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-XP"),
        station: "yalain-testkit::cpu::play::rules::a_solo_kill_pays_the_death_experience_weighed_by_the_level_difference",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.asking-runs-out",
        says: "A recruit's asking unanswered for twenty seconds runs out: the recruit is told, and \
               the place can no longer be taken.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-EXPIRE"),
        station: "yalain-testkit::cpu::play::fellowships::an_unanswered_asking_runs_out_after_twenty_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.killing-blow-told",
        says: "A creature's kill goes to the player that did it the most damage, a pet's damage \
               counted as its owner's; a fellow of the killer whose blow killed the creature is \
               told only that its blow did, naming the creature, and a player outside the \
               killer's fellowship is told nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-KILLING-BLOW"),
        station: "yalain-testkit::cpu::play::fellowships::a_fellows_killing_blow_on_a_kill_credited_to_another_fellow_is_told_to_it_alone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.left-and-disbanded",
        says: "A fellow leaving (or leaving the world) is cleared of the fellowship, told it left \
               and leaves the channel; the others are told it left; a leader leaving leaves the \
               oldest fellow leading; disbanding clears every fellow and tells each.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-LEAVE"),
        station: "yalain-testkit::cpu::play::fellowships::leaving_and_disbanding_clear_the_fellows_and_tell_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.made",
        says: "Making a fellowship enters its chat channel, sends the fellowship with its maker as \
               leader and only fellow, and says it was made; a name with anything but letters, \
               spaces, apostrophes and hyphens is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-MADE"),
        station: "yalain-testkit::cpu::play::fellowships::a_fellowship_made_is_sent_to_its_maker_and_a_bad_name_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.recruited",
        says: "A recruit is walked to when out of reach, then the leader is told it waits and the \
               recruit asked; taking the place tells the recruit it joined (the whole fellowship \
               sent, the channel entered) and every fellow already in it of the new fellow; one in \
               a fellowship already is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-RECRUIT"),
        station: "yalain-testkit::cpu::play::fellowships::a_recruit_asked_and_accepting_joins_and_every_fellow_is_told",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.share-counts-far-fellows",
        says: "The share of a kill's experience goes by how many fellows are of a level to share, \
               near or not; only those within 160 m of the fellow whose part it is are paid.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-SHARE-COUNT"),
        station: "yalain-testkit::cpu::play::fellowships::the_share_counts_every_fellow_of_a_level_to_share_near_or_not_and_pays_those_within_160_m",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.shared-experience",
        says: "A kill in a fellowship that shares experience pays each fellow near whose level is \
               within five of the leader's the creature's experience by its own level times a \
               share by how many are of a level to share; a group creature's takes a factor by the \
               fellowship's size.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-XP"),
        station: "yalain-testkit::cpu::play::fellowships::a_kill_is_shared_by_the_fellows_of_a_level_to_share",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.shares-each-part",
        says: "In a fellowship that shares experience, each part of a kill a fellow earns (its own \
               or its pet's) is shared out in turn: every fellow of a level to share is paid the \
               share of the experience by its level times the part's share of the health taken, \
               rounded to the nearest, without a text.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-XP-PARTS"),
        station: "yalain-testkit::cpu::play::fellowships::each_part_of_a_kill_is_shared_out_among_the_fellows_without_a_text",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.shares-evenly",
        says: "Every fellow of a level to share gets the same share of a kill's experience by its own \
               level, whatever its level against the others'.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-EVEN"),
        station: "yalain-testkit::cpu::play::fellowships::shares_go_evenly_whatever_the_fellows_levels",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.unlevelled-fellow-paid-alone",
        says: "A fellow not of a level to share is paid its own part of a kill alone, whole by its own \
               level and without a text, and no other part reaches it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-UNLEVELLED"),
        station: "yalain-testkit::cpu::play::fellowships::a_fellow_not_of_a_level_to_share_is_paid_its_own_part_whole_and_none_of_the_others",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.updates",
        says: "A fellow's changed health, vigor, their greatest, level and cell go to every \
               fellow, itself included, at a look every three and a half to seven and a half \
               seconds; nothing unchanged is sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-UPDATES"),
        station: "yalain-testkit::cpu::play::fellowships::a_fellows_changed_health_goes_to_every_fellow_and_nothing_unchanged_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.fellowship.updates-in-order",
        says: "What changed of a fellow goes out in one order: its greatest health, health, \
               greatest vigor, vigor, then where it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FELLOW-ORDER"),
        station: "yalain-testkit::cpu::play::fellowships::a_fellows_changes_go_out_in_order_health_before_vigor_before_place",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.focus.fellowship-shares",
        says: "In a fellowship that shares experience, each part of a kill's focus is shared out \
               without a text: every fellow of a level to share within 100 m of the one whose \
               part it is gets the death focus times the share for how many share times the \
               part's share of the health taken, rounded; a group creature's factor is the \
               experience's alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FOCUS-FELLOWS"),
        station: "yalain-testkit::cpu::play::focus::a_fellowship_shares_a_kills_focus_within_100_m_without_the_group_factor",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.focus.focus-alone",
        says: "A part of a kill worth focus and no experience says \"You have gained %2 Focus \
               points!\" and sends the new focus, and no totals.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FOCUS-ALONE"),
        station: "yalain-testkit::cpu::play::focus::a_part_worth_focus_and_no_experience_names_the_focus_alone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.focus.greatest",
        says: "Focus stops at its greatest (100 and the character's own focus quality): an award \
               past it sends the greatest; an award at it sends no focus, its text still naming \
               the whole part.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FOCUS-GREATEST"),
        station: "yalain-testkit::cpu::play::focus::focus_stops_at_its_greatest_and_an_award_at_it_sends_no_focus",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.focus.heroes-only",
        says: "Only a hero gains focus: anyone else's part says \"You have gained %1 experience \
               points!\", naming the focus the part was worth, and no focus is sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FOCUS-HEROES"),
        station: "yalain-testkit::cpu::play::focus::a_player_who_is_no_hero_gains_no_focus",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.focus.kill-pays-in-parts",
        says: "A hero's kill pays the creature's death focus in parts as its experience: each part \
               the death focus times the health the damager took over all taken, rounded to the \
               nearest, named in \"You have gained %1 experience points and %2 Focus points!\", \
               the new focus sent next, then the totals.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FOCUS-PARTS"),
        station: "yalain-testkit::cpu::play::focus::a_heros_kill_pays_its_death_focus_in_parts_with_the_experience",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.focus.made-a-hero",
        says: "The operator makes a character a hero by name: in the world it is sent its hero \
               quality and its focus at its greatest; out of it, its stored qualities are \
               changed and it enters as a hero; an unknown name is refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FOCUS-HERO-MADE"),
        station: "yalain-testkit::cpu::play::focus::a_character_made_a_hero_is_told_and_has_its_greatest_focus",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.focus.resurrection",
        says: "Back from a resurrection, a player with more than ten focus has ten, sent after its \
               health and vigor and before it is told it is alive; ten or less is kept, nothing \
               sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FOCUS-RESURRECT"),
        station: "yalain-testkit::cpu::play::dying::a_resurrected_player_with_more_than_ten_focus_comes_back_with_ten",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.focus.unlevelled-fellow-keeps-focus",
        says: "The focus of a part earned by a fellow not of a level to share is its own, whole, and no \
               one else's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FOCUS-UNLEVELLED"),
        station: "yalain-testkit::cpu::play::fellowships::a_fellow_not_of_a_level_to_share_keeps_its_parts_focus_whole",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.focus.whatever-the-levels",
        says: "A kill's focus is not weighed by the levels: a creature far above the hero pays its \
               whole death focus while its experience is cut.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FOCUS-LEVELS"),
        station: "yalain-testkit::cpu::play::focus::a_kills_focus_is_the_same_whatever_the_levels",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.dn-channel-said",
        says: "Said @dn channel asks the hub, given rightly and on the network, and the hub's \
               answer comes as a line; given wrongly it is its help line, a bad name the rule for \
               names, a gagged character the gag's line. A tell to a channel goes through the link; \
               squelching a channel's address leaves it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-DN-CHANNEL-SAID"),
        station: "yalain-friends::cpu::commands::dn_channel_asks_the_hub_and_a_squelch_of_a_channel_leaves_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.dn-friends-said",
        says: "Said @dn friends lists the character's friends on other worlds with their state; add \
               and remove do what the client's own add and delete do, with no request to answer, \
               and say so.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-DN-FRIENDS-SAID"),
        station: "yalain-friends::cpu::commands::dn_friends_lists_adds_and_removes_friends_on_other_worlds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.dn-global-said",
        says: "Said @dn global on a world that carries no Global room says so, with nothing to say \
               its help, and off the network that it is off; otherwise the line is kept for the \
               world to say in the room, and nothing is answered.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-DN-GLOBAL"),
        station: "yalain-friends::cpu::commands::dn_global_says_a_line_in_the_room_that_carries_it_or_says_why_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.dn-link-said",
        says: "Said @dn link asks the hub and its answer comes as a line; @dn status reads as @dn, \
               which says a linked character is linked and names its directory page; @dn off says a \
               linked character's link is paused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-DN-LINK-SAID"),
        station: "yalain-friends::cpu::commands::dn_link_is_answered_by_the_hub_and_status_and_off_say_a_character_is_linked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.dn-said-in-chat",
        says: "A line said as @dn, aloud or into a room, is heard by nobody and answered with the \
               world's lines: the character's place on the network, on and off at once (kept), \
               the commands, and with the world off the network, that it is not on it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-DN"),
        station: "yalain-friends::cpu::commands::dn_said_in_chat_shows_puts_on_and_takes_off_the_network",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.dn-spelled-out-and-dng",
        says: "@derethnetwork is @dn spelled out, and @dng with its text is @dn global with its \
               text made short: heard by nobody and kept with their words.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-DN-NAMES"),
        station: "yalain-testkit::cpu::entry::chat::derethnetwork_and_dng_are_the_networks_command_under_other_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.dn-squelch-a-world",
        says: "Said @dn squelch ignores every character of a world for the one character, kept \
               across a restart and never shown in the client's own squelch list, at most fifty \
               worlds.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-DN-SQUELCH-WORLD"),
        station: "yalain-friends::cpu::commands::dn_squelch_ignores_a_whole_world_without_showing_it_in_the_clients_list",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.entry-line-only-without-a-stream",
        says: "The line that the character could not be logged into the authorization server, \
               its friends list out of reach, is shown as it enters only when the world serves no \
               friends service or the account has no stream on it; an account with a stream \
               enters without it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-ENTRY-LINE"),
        station: "yalain-testkit::cpu::entry::world::the_line_that_friends_are_out_of_reach_comes_at_entry_only_without_a_friends_stream",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.first-list-waits-for-the-view",
        says: "A client's ask for its friends while the first person view of a character that asked \
               for its link is due waits for it, at most three seconds, so the page holds its \
               person's friends from the start: the page rebuilds only from a list it asks for. Any \
               other ask waits for nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-FIRST-LIST"),
        station: "yalain-friends::cpu::network::the_clients_first_ask_for_its_friends_waits_for_the_first_person_view",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.first-state-offline-quiet",
        says: "A friend on another world first reported offline shows nothing: the client was told \
               so with its friends and prints a line for every state. A first report of online, and \
               a later change either way, are shown.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-FIRST-OFFLINE"),
        station: "yalain-friends::cpu::network::a_friend_first_reported_offline_is_not_announced_as_having_left",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.friend-added-and-removed",
        says: "A friend of this world must be a character of it, named with no world, the world's \
               name in any case or its slug, and is written back with the world's name exactly; \
               added, its state goes first and then the answer; one listed already, not listed, \
               or no address is answered with the code that says so.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-FRIEND-ADD"),
        station: "yalain-friends::cpu::friends::a_friend_of_this_world_is_added_with_its_state_first_and_removed_by_any_of_its_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.gagged-tells-go-nowhere",
        says: "A character the world has gagged tells nobody through the friends service, on this world or \
               another: nothing is shown or sent to the hub, its client is answered that the tell \
               could not be sent, and it is told it is gagged, as Empyrean refuses a gagged \
               player's tells.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-GAG-FRIENDS"),
        station: "yalain-friends::cpu::tells::a_gagged_characters_tells_through_the_service_go_nowhere",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.hand-over-keeps-presence",
        says: "A character whose friends stream came while it was already in the world (a new \
               client taking over from one gone) stays present for nineteen seconds after it \
               leaves the world, counted from its body's leaving at the end of its linger, not \
               from its old login's going; entering again within them, no friend in the world or \
               on another world sees it go and come back, and its stream is sent its friends \
               again. Not entered again, or its stream ended, or another character of the account \
               entering, it is seen to leave then.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-HAND-OVER"),
        station: "yalain-friends::cpu::presence::a_character_handed_over_and_entered_again_in_time_is_never_seen_offline",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.hidden-is-not-on-the-network",
        says: "To other worlds a cloaked admin is not there: its watchers see it offline, a watch is \
               answered offline, a tell is answered as one to a character offline. It stops watching \
               its friends, so what it shows of them goes offline; entering and leaving cloaked says \
               nothing. Our design.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NETWORK-HIDDEN"),
        station: "yalain-friends::cpu::presence::a_hidden_character_is_not_there_to_other_worlds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.hidden-is-offline-to-friends",
        says: "To its friends in the world a cloaked admin is offline: it goes offline once, its state, \
               leaving and coming back are not told, added it shows offline, a tell to it is answered \
               as one to a character offline and shows nothing. Lifted, it is back in the state it \
               chose. Our design: no recording holds an admin.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-HIDDEN"),
        station: "yalain-friends::cpu::presence::a_hidden_character_is_offline_to_its_friends_and_tells",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.ignore-list-kept",
        says: "The ignore list keeps characters of this world, which must exist, and other \
               worlds' addresses, lists them back as the client knows them, and answers a name \
               already listed or not listed with the code that says so.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-IGNORE"),
        station: "yalain-friends::cpu::friends::the_ignore_list_keeps_names_of_this_world_and_addresses_of_others",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.ignored-tells-not-shown",
        says: "A character never hears a tell from a character it ignores, through the friends \
               service or the game's own tell; the teller is shown what a delivered tell shows.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-IGNORED-TELL"),
        station: "yalain-friends::cpu::tells::an_ignored_tellers_tell_is_answered_done_and_shown_to_nobody",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.lists-capped",
        says: "A character keeps at most a hundred friends; the next is refused with the world's \
               line and a code the client shows as its list's error.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-CAP"),
        station: "yalain-friends::cpu::friends::a_friends_list_holds_a_hundred_and_the_next_is_refused_with_a_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.lists-follow-a-rename",
        says: "A rename changes every character's friend and ignore entries for it, in the store with \
               the name and in the service, so they hold for good, whether the lister is in the world \
               or not. An entry for another world's character of that name stays; a list holding both \
               names holds it once. Chosen: no recording holds a rename.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-RENAME-LISTS"),
        station: "yalain-friends::cpu::friends::other_characters_lists_hold_a_renamed_character_by_its_new_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.lists-kept",
        says: "Each character's friends and ignored names are kept across a restart, a name once \
               in any case, and forgotten with the character; a database kept before the friends \
               service opens with every list empty.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-KEPT"),
        station: "yalain-store::cpu::persistence::an_earlier_database_gains_the_friends_lists_and_keeps_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.presence-within-the-world",
        says: "A character with a friends stream entering the world is shown online to every \
               present character that lists it, and offline when it leaves (unless it was handed \
               over to a new client) or its stream ends; away is mode 3 and not to be disturbed 5, \
               and the state lasts until it leaves.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-PRESENCE"),
        station: "yalain-friends::cpu::presence::friends_in_the_world_see_a_character_come_go_and_change_its_state",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.remote-add-shown-once",
        says: "A friend added on another world is shown once, with the state its world reports, \
               then the add is answered; offline at once when the link is down or its world not \
               linked, and offline after three seconds without an answer (a later answer shown \
               only if online); never offline first and then its real state.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-REMOTE-ADD"),
        station: "yalain-friends::cpu::network::a_friend_added_on_another_world_is_shown_once_with_the_state_its_world_reports",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.remote-friend-watched",
        says: "Another world's friend needs the world and the character on the network, else the \
               world's line; kept, it is watched over the link, and shows each state its world \
               reports, offline again when its world goes; another world sees a \
               character online only while it has a stream.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-REMOTE-FRIEND"),
        station: "yalain-friends::cpu::network::another_worlds_friend_is_watched_and_its_state_shown_once_the_character_is_on_the_network",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.remote-tell",
        says: "A tell to another world's character goes over the link and is answered once, when \
               it ends: done for delivered, away (with the away line) and squelched, not logged \
               in for offline, could not be sent with the world's line for the rest and for no \
               answer in time; a tell from another world reaches the stream with the teller's \
               address.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-REMOTE-TELL"),
        station: "yalain-friends::cpu::network::a_tell_to_another_world_is_answered_once_by_how_it_ended",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.rename-seen-at-once",
        says: "A renamed character stays present with its stream and state under the new name alone, \
               without a relog: a friend present sees the old name go and the new come, tells find the \
               new name and not the old, and a hidden one tells nobody until it comes back. Chosen over \
               the relog the client's help describes.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-RENAME"),
        station: "yalain-friends::cpu::presence::a_renamed_character_stays_present_under_its_new_name_alone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.second-stream-replaces-first",
        says: "A second friends stream for an account replaces the first, which is told so and \
               closed; the character stays present, its friends told nothing, and the new stream \
               is sent its friends.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-REPLACED"),
        station: "yalain-friends::cpu::streams::a_second_stream_for_an_account_replaces_the_first_and_the_character_stays_present",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.served-beside-the-login",
        says: "The server serves the friends stream on TCP at its login port's number: a ticket \
               admits a stream before the game login, friends and tells work within the world, \
               and after @dn on said in chat another world's friend is watched and tells go both \
               ways over the link.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-LIVE"),
        station: "yalain::live::friends::the_server_serves_friends_and_tells_within_the_world_and_across_the_link",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.friends.state-told-only-when-it-changes",
        says: "Every state a character asks for (away, not to be disturbed, available) is \
               answered, and its friends in the world are told only when the state changes: the \
               client asks for away twice for one away, and prints a line for each state it is \
               sent.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-STATE-ONCE"),
        station: "yalain-friends::cpu::presence::the_same_state_asked_for_again_is_answered_but_not_told_to_friends_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.stream-back-sent-its-friends",
        says: "A friends stream that comes back while its character is in the world, after the \
               old one dropped or in place of one still open, is sent the whole friends list with \
               each friend's state at its ticket and answers the list asked for after; a \
               replacing stream leaves the character present, so no friend in the world or on \
               another world sees it go and come back.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-STREAM-BACK"),
        station: "yalain-friends::cpu::streams::a_stream_that_drops_and_comes_back_is_sent_the_whole_friends_list_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.stream-bound-to-account-and-character",
        says: "A friends stream is bound to the account its ticket names, as the login's rule \
               admits it, and acts for the account's character while it is in the world: every \
               request is answered not connected before, the friends list is sent as it enters \
               (or as the stream comes back), and nothing as it leaves.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-BOUND"),
        station: "yalain-friends::cpu::streams::a_ticketed_stream_acts_for_its_accounts_character_while_it_is_in_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.streams-bounded",
        says: "One address holds at most eight friends streams, an IPv6 address counted by its \
               /64; the next is told the world is full and closed.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-BOUNDS"),
        station: "yalain-friends::cpu::streams::an_address_holds_at_most_eight_streams_an_ipv6_one_by_its_prefix",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.suffixed-name",
        says: "A name on the friends stream may be one in the world as a client shows it, with its \
               suffix, as the client's add button and a tell by the shown name send it: a friend, \
               a squelch and a tell find the character and keep its own name; its own name wins; \
               a cloaked admin's shown name is no one.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-SUFFIXED"),
        station: "yalain-friends::cpu::friends::a_friend_or_a_squelch_by_a_name_with_its_suffix_keeps_the_character",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.tell-within-the-world",
        says: "A tell to a present character of this world reaches its stream from the teller as \
               Name@ and the world's name, with its extra data, answered done (with the world's \
               away line for one away); one not present is answered not logged in, an unknown \
               name doesn't exist, and a character's tells are held to the network's pace.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-TELL"),
        station: "yalain-friends::cpu::tells::a_tell_within_the_world_is_shown_to_a_present_character_and_answered_by_its_outcome",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.friends.ticket-refused-and-guessing-held-back",
        says: "A ticket the login's rule refuses, or one not of the ticket's form, is answered \
               refused and the stream may try again; after five refusals from one address in a \
               minute, its next minute's tickets are refused unchecked.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-FRIENDS-TICKET"),
        station: "yalain-friends::cpu::streams::refused_tickets_are_answered_so_and_five_in_a_minute_lock_the_address_for_a_minute",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.a-gift-is-answered-before-its-object",
        says: "An item made in a player's pack is answered before its object is sent, so the \
               client's own line for it calls it the item.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIFT-ORDER"),
        station: "yalain-testkit::dat::session::a_gift_from_a_person_is_called_the_item_by_the_clients_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.gives.a-line-for-an-item",
        says: "A line a person says at a give naming an item is said of that item and no other, before \
               its plain refusal, and a line for taking an item is not said as one is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-LINE-FOR-ITEM"),
        station: "yalain-testkit::cpu::play::gives::a_line_naming_an_item_is_said_of_that_item_and_no_other",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.a-step-keeps-the-item",
        says: "An item given to a person that a quest step takes is kept: it leaves the pack and the \
               world, destroyed in the person, the person says its line with the update and the quest \
               moves on; an item the step gives comes from the person, and nothing is handed back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-KEEP"),
        station: "yalain-testkit::cpu::play::gives::an_item_a_step_takes_is_kept_and_moves_the_quest",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.a-step-under-way-runs-as-its-player-leaves",
        says: "A player leaving with a give's step still to come (the gift already kept, \
               the transformation under way) has the step run as it leaves: it is not out \
               of its gift for nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-LEFT"),
        station: "yalain-testkit::cpu::play::gives::a_hero_step_under_way_runs_as_its_player_leaves",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.any-count-taken-whole",
        says: "A give's step that names no count runs for whatever is given and takes all of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-ANY-COUNT"),
        station: "yalain-testkit::cpu::play::gives::a_step_wanting_no_count_takes_whatever_is_given",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.cannot-get-there",
        says: "A walk to give that ends at rest out of reach is refused as one that could not get \
               there: nothing handed over, the item never leaving the pack.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-CANT-GET-THERE"),
        station: "yalain-testkit::cpu::play::gives::a_give_whose_walk_ends_out_of_reach_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.counted-hand-ins",
        says: "Every hand-in whose quest or person names a count wants exactly that count, and a person \
               with a line for the wrong number says it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WDB-HAND-IN-COUNTS-2017"),
        station: "yalain-testkit::corpus::world_db::hand_ins_naming_a_count_want_it_and_the_wrong_number_has_its_line",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.gives.hero-transformation",
        says: "A give whose step makes a hero runs that step when the transformation is through, 25.59 \
               s after the answer: the player a hero, the quest completed, the hero's text; meanwhile \
               only the transformation's appearances, the gift kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-HERO"),
        station: "yalain-testkit::cpu::play::gives::a_give_that_makes_a_hero_does_so_when_the_transformation_is_through",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.kept-in-2017",
        says: "The gives January 2017 kept with no step here have their rows: the offerings' chain, the \
               shrine's recipe, the mirras, the bucket, the repeat dips, the donation, the bribe, the \
               carcass, the Warders' donations, the gem hand-in and the periapt.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WDB-GIVES-KEPT-2017"),
        station: "yalain-testkit::corpus::world_db::gives_2017_kept_with_no_quest_moved_have_their_rows",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.gives.map-named-by-the-persons-line",
        says: "A person whose table names no land sends players only with the maps its lines name (the \
               miller's starter map); another map is handed back with its line for a map it does not take.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-MAP-NAMED"),
        station: "yalain-testkit::cpu::play::map_trips::a_person_with_no_land_sends_players_only_with_the_maps_its_lines_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.map-of-another-land",
        says: "A cartomancer given a map of another land hands it back with its own line for such a map \
               and sends no one; an item that is no map gets its refusal of a give.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-MAP-OTHER-LAND"),
        station: "yalain-testkit::cpu::play::map_trips::a_map_of_another_land_is_handed_back_with_the_persons_line_for_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.map-sends-the-giver",
        says: "A cartomancer given a map of its own land says its line and hands the map back; as the \
               map is back, 1.30 s later, the giver goes into portal space and comes out at the place \
               the world names for the map. The map is kept; no portal is made.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-MAP-TRIP"),
        station: "yalain-testkit::cpu::play::map_trips::a_cartomancer_given_a_map_of_its_land_sends_the_giver_where_it_leads",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.map-with-no-place",
        says: "A map whose place the world does not hold is refused as one of another land: handed back, \
               no one sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-MAP-NO-PLACE"),
        station: "yalain-testkit::cpu::play::map_trips::a_map_whose_place_the_world_does_not_hold_sends_no_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.maps-named-by-the-release",
        says: "The pinned world database names a place for each surveyor's map whose trip 2017 \
               recorded: 116 maps.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-MAPS-NO-ROWS"),
        station: "yalain-testkit::corpus::world_db::the_pinned_release_names_a_place_for_the_maps_2017_sent_players_by",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.gives.mine-tolls",
        says: "A miner's toll is a hidden quest: the fee starts it, the miner's portals ask for it under \
               way, and a player who has paid hears the miner's line for a toll still good.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WDB-MINE-TOLLS-2017"),
        station: "yalain-testkit::corpus::world_db::a_miners_toll_is_a_hidden_quest_its_portals_ask_for",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.gives.no-step-hands-back",
        says: "An item no step wants is answered as given, the person says its refusal of a give and \
               hands it back: a person plays its handing-over and the item is back 1.30 s later, an \
               object hands it back at once; no quest moves.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-HAND-BACK"),
        station: "yalain-testkit::cpu::play::gives::an_item_no_step_wants_is_handed_back_with_the_persons_refusal",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.one-give-at-a-time",
        says: "A person takes one give at a time: another player's give to it while one is under way is \
               refused at once as the target busy, with no walk; an object takes both; a giver's second \
               give while its first is under way is refused as its actions locked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-BUSY"),
        station: "yalain-testkit::cpu::play::gives::a_person_takes_one_give_at_a_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.part-of-a-stack",
        says: "A part of a stack given to a step that keeps it is taken, the rest staying in the pack \
               with its quantity told.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-STACK"),
        station: "yalain-testkit::cpu::play::gives::a_part_of_a_stack_given_is_taken_and_the_rest_stays",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.part-stays-in-the-pack",
        says: "A part of a stack given stays in the pack: its container is not cleared at the handing \
               over, and handed back it merges, its container not told again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-PART-STAYS"),
        station: "yalain-testkit::cpu::play::gives::a_part_of_a_stack_refused_comes_back_merged_with_its_container_untouched",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.received-moves-on",
        says: "An item a give's step hands the player that moves the same quest on when received moves \
               it after the step's own update: two records, in that order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-RECEIVED"),
        station: "yalain-testkit::cpu::play::gives::an_item_a_give_brings_moves_its_quest_on_after_the_step",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.row-any-item",
        says: "A give that moves no quest may name no item: the person takes anything, with one of its \
               lines for taking any item, and a row naming the item given comes first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-ROW-ANY"),
        station: "yalain-testkit::cpu::play::gives::a_give_naming_no_item_takes_anything_and_a_row_for_the_item_comes_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.row-keeps-and-gives",
        says: "A give that moves no quest keeps the item whole, says the person's lines for taking it in \
               their order, plays what the row says and gives what it gives from the person, and puts \
               its effects and its text on the player; no quest moves and nothing is handed back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-ROW"),
        station: "yalain-testkit::cpu::play::gives::a_give_moving_no_quest_keeps_the_item_says_its_lines_and_gives_its_items",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.row-requires",
        says: "A give that moves no quest and requires a quest at a phase runs at that phase alone, and a \
               quest's step for the same give comes first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-ROW-REQUIRES"),
        station: "yalain-testkit::cpu::play::gives::a_give_requiring_a_phase_runs_at_it_and_a_step_comes_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.row-wants-a-count",
        says: "A give that moves no quest and wants a count refuses any other with the person's line for \
               the item, the stack whole; that count is taken and the rest stays.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-ROW-WANTS"),
        station: "yalain-testkit::cpu::play::gives::a_give_moving_no_quest_may_want_a_count_and_refuses_any_other",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.subquests",
        says: "Items given that close a phase's subquests each tell the quest's record with its phase \
               kept, and the last, in any order, moves it on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-SUBQUESTS"),
        station: "yalain-testkit::cpu::play::gives::items_given_close_a_phases_subquests_and_the_last_moves_it_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.the-last-subquest-says-the-next-phase",
        says: "The give that closes a phase's subquests is said as the quest's next phase where the \
               person has a line for that; the rest, and a person with no such line, as the \
               subquest's own.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-LAST-SUB-LINE"),
        station: "yalain-testkit::cpu::play::gives::the_give_closing_a_phase_is_said_as_the_next_phase_where_a_line_says_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.vitae-tap",
        says: "A step's tap of vitae leaves the player nine tenths of its vitae, told with what is \
               still to earn, its appearance played on the player.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-VITAE-TAP"),
        station: "yalain-testkit::cpu::play::gives::a_steps_vitae_tap_takes_a_tenth_of_the_vitae",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.walked-handed-over-and-answered",
        says: "A give is walked to 2 m of the person, the order sent even when the player is there; the \
               player plays its handing-over with the item's look in its hand in the walk's context, \
               seen by those who see it, the item's container cleared; the give is answered as done, \
               the animation played, 1.31 s later.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-WALK"),
        station: "yalain-testkit::cpu::play::gives::an_item_given_is_walked_to_handed_over_and_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.wants-a-count",
        says: "A give's step may want an exact count: it takes that many, whole or split off a stack, \
               and any other count is refused and handed back, the quest not moved.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-WANTS"),
        station: "yalain-testkit::cpu::play::gives::a_step_wanting_a_count_takes_that_many_and_refuses_any_other",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.gives.wrong-count-line",
        says: "A wrong count hears the person's own line for it where it has one, for a quest waiting \
               for that item, and its plain refusal otherwise.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-WRONG-COUNT"),
        station: "yalain-testkit::cpu::play::gives::the_wrong_number_line_is_said_only_with_the_step_waiting_for_the_item",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.interactions.an-item-gone-while-it-plays-does-nothing",
        says: "An item put down while its use on an object plays: nothing comes of the use, \
               as for a use from the pack.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-INTERACTION-GONE"),
        station: "yalain-testkit::cpu::play::interactions::an_item_put_down_while_its_use_plays_does_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.a-gift-runs-past-a-full-pack",
        says: "An item given to a pack holding all its room is held all the same, and its move says \
               it took a place past the room; an item from a person may always take one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PACK-OVERFLOW"),
        station: "yalain-testkit::cpu::play::pack::a_gift_past_a_full_pack_is_held_and_its_move_says_so",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.a-part-split-off-holds-the-stacks-numbers",
        says: "Part of a stack split off, in the pack, to a trade or to a broker, is a new item \
               holding all the stack holds of its own.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SPLIT-NUMBERS"),
        station: "yalain-testkit::cpu::play::stack_numbers::a_part_split_off_in_the_pack_keeps_the_stacks_numbers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.a-picked-up-item-goes-to-the-packs-front",
        says: "An item picked up goes to the front of the pack, every item there moving up one, \
               and the move names that slot as both where it was sent and where it went.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PICK-UP-FRONT"),
        station: "yalain-testkit::cpu::play::drops::a_picked_up_item_goes_to_the_front_of_the_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.a-picked-up-item-joins-the-stack-it-matches",
        says: "An item picked up of a template the pack holds a stack of joins that stack: the \
               move is answered as merged, the stack shows its new quantity, and the item is \
               destroyed and leaves the world; another use meanwhile is refused at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PICK-UP-MERGE"),
        station: "yalain-testkit::cpu::play::drops::a_picked_up_item_joins_the_stack_the_pack_holds_of_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.a-pickup-the-pack-has-no-room-for-is-put-back",
        says: "An item picked up into a full pack is refused once the picking-up behavior is \
               through: the move is answered with the pack full and the player puts the item back \
               down where it lay.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PICK-UP-FULL"),
        station: "yalain-testkit::cpu::play::drops::a_pickup_into_a_full_pack_is_refused_and_the_item_put_back_down",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.a-pickup-the-player-cannot-get-to-is-refused",
        says: "A player walked toward an item to pick up that stands still twice at the same \
               place out of reach could not get there: the move is answered so, with no \
               animation, and the walk is stopped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PICK-UP-REACH"),
        station: "yalain-testkit::cpu::play::drops::a_pickup_the_player_cannot_get_to_is_refused_and_the_walk_stopped",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.a-save-at-entry-keeps-what-items-carry",
        says: "A save made as a character enters, before its items are back in the world, \
               keeps what they carry of their own (who made them, their wear and bonuses) \
               as it was.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SAVE-AT-ENTRY"),
        station: "yalain-testkit::cpu::operator::item_ids::a_save_made_as_a_character_enters_keeps_what_its_items_carry",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.a-stack-pays-for-each-piece",
        says: "Each piece of a stack with a value of its own brings that value, sold whole or \
               piece by piece, and its profile shows the value of them all.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STACK-WORTH"),
        station: "yalain-testkit::cpu::play::stack_numbers::a_stack_with_its_own_value_pays_the_same_whole_or_piece_by_piece",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.a-taken-item-joins-its-stack",
        says: "An item taken from a corpse that can join a stack in the pack joins it and is gone; it \
               needs no place of its own, so a pack holding all its room takes it too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TAKE-JOINS-STACK"),
        station: "yalain-testkit::cpu::play::integrity::a_taken_item_joins_its_stack_even_in_a_full_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.a-thing-never-takes-anothers-place",
        says: "A thing never takes the place of one the world has: an item a character \
               comes back with under an id the world already has is given a fresh id, each \
               holder keeping its own.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-IDS-CLASH"),
        station: "yalain-testkit::cpu::operator::item_ids::two_characters_kept_with_one_id_each_keep_their_own_item",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.an-attunable-item-is-not-put-down",
        says: "Asked to put down an item of an attunable template, the server answers at once \
               that it is attuned, plays nothing, and the item stays in the pack.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ATTUNABLE-DROP"),
        station: "yalain-testkit::cpu::play::drops::an_attunable_item_is_refused_at_once_and_stays_in_the_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.an-item-a-camp-keeps-lies-again-after-its-taking",
        says: "A camp whose template is an item keeps it lying at its spot: once it is taken, a new \
               one lies there when the camp's period is over (quest items lying alone 20 s after \
               their taking, keys 30 s, a dungeon's mushrooms about three minutes).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LYING-ITEMS"),
        station: "yalain-testkit::cpu::play::drops::an_item_a_camp_keeps_lies_again_at_its_spot_once_its_period_after_the_taking_is_over",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.an-item-being-put-down-is-kept-at-a-stop",
        says: "An item being put down when its player leaves the world, the world stopping \
               or the player taken out of it, is back in its pack and kept; one that has \
               landed is out of the kept pack at once.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PUT-DOWN-STOP"),
        station: "yalain-testkit::cpu::play::integrity::a_world_stopped_while_an_item_is_put_down_keeps_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.an-item-lying-in-the-world-is-walked-to-and-picked-up",
        says: "Using an item lying in the world walks the player to it (the order sent even when it is \
               there), then plays the picking-up behavior; when that is through the item is in the \
               pack, the move answered, its holder told those who see it, and it leaves the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PICK-UP"),
        station: "yalain-testkit::cpu::play::drops::an_item_lying_in_the_world_is_walked_to_and_picked_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.an-item-put-down-stands-ahead-of-the-player",
        says: "Asked to put an item down, the server has the player play its putting-down \
               behavior with the item in its hand and clears the item's container at once; when \
               the behavior is through (1.75 s for a human man) the move is answered and the item \
               stands straight ahead of the player at its reach times its height, half a meter \
               up, facing as it faces, made there for those who see it, and 0.3 s later comes to \
               rest just above the player's feet; a second drop meanwhile is refused at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PUT-DOWN"),
        station: "yalain-testkit::cpu::play::drops::an_item_put_down_stands_ahead_of_the_player",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.armor-used-is-worn-and-seen",
        says: "Armor, clothing or jewelry used from the pack is worn where it belongs: the wearer \
               looks it to everyone who sees it, and used again it comes off to the pack's front.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WEAR"),
        station: "yalain-testkit::cpu::play::wearing::a_chest_piece_used_is_worn_and_its_look_seen_and_used_again_comes_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.closing-only-what-the-player-has-open",
        says: "Only the player who has a corpse open closes it; another naming it changes nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CLOSE-OPEN"),
        station: "yalain-testkit::cpu::play::integrity::a_corpse_another_player_has_open_is_not_closed_by_a_stranger",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.coins-out-of-the-bag-need-room",
        says: "Coins taken out of the money bag into a full pack, or by a player busy or \
               dead, do nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-COINS-ROOM"),
        station: "yalain-testkit::cpu::play::pack::coins_out_of_the_money_bag_into_a_full_pack_do_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.coins-used-into-money",
        says: "Coins used from the pack go into the money bag whole, one gold each, as a drag of \
               them would: the adding text, the money up, the coins gone, and the drag's answer \
               from the player at slot 0, the use itself not answered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-COINS-USED"),
        station: "yalain-testkit::cpu::play::pack::coins_used_from_the_pack_go_into_the_money_bag_whole",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.conservation",
        says: "Over random scripts of takes, trades, drops, stacks moved, money moved, \
               deaths, log-offs, stops and crashes, every item is held exactly once across \
               every record, and what players have is what they were given, less at a stop \
               only what lay on the ground or in a corpse.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CONSERVATION"),
        station: "yalain-testkit::cpu::play::conservation::every_item_is_held_once_and_nothing_is_made_or_lost_over_random_scripts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.inscribing-an-item",
        says: "A weapon, armor, a shield, jewelry or a tool takes an inscription and the player is \
               told it is inscribed; any other item is refused with a text.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-INSCRIBE"),
        station: "yalain-testkit::cpu::play::own_requests::a_weapon_takes_an_inscription_and_a_mote_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.inscribing-only-what-the-player-holds",
        says: "An item the player does not hold is refused an inscription with the text any \
               other item is, and takes no words.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-INSCRIBE-HELD"),
        station: "yalain-testkit::cpu::play::own_requests::an_item_the_player_does_not_hold_takes_no_inscription",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.inscription-kept-across-sessions",
        says: "An inscription stays with its item across a logout, with who wrote it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-INSCRIPTION-KEPT"),
        station: "yalain-testkit::cpu::play::own_requests::an_inscription_is_kept_across_a_logout",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.item-ids-never-given-twice",
        says: "An id an item holds is never given again: the world's ids come in blocks the \
               store sets aside before they are given, above every range a run before them \
               used, whatever a character coming back holds.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-IDS"),
        station: "yalain-testkit::cpu::operator::item_ids::a_restart_never_gives_a_new_thing_an_id_a_character_away_still_holds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.items-come-in-at-the-packs-front",
        says: "An item given, bought or taken goes to the front of the pack (of the first bag with \
               room when the pack is full), every item there moving up one, and the move names that \
               slot as both where it was sent and where it went.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PACK-FRONT"),
        station: "yalain-testkit::cpu::play::pack::an_item_given_comes_in_at_the_packs_front_and_the_move_names_that_slot",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.items-with-no-use-only-answer",
        says: "A mote used from the pack is refused as unusable with the use's own flags, and a dye \
               plant is answered with nothing done; neither is wielded.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-NO-USE-ITEMS"),
        station: "yalain-testkit::cpu::play::own_requests::a_mote_used_from_the_pack_is_refused_and_a_dye_plant_only_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.lodestones-only-answer",
        says: "A lodestone used from the pack is only answered, made with the flags of a use at \
               once (444), and is neither wielded nor spent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LODESTONES"),
        station: "yalain-testkit::cpu::play::own_requests::a_lodestone_used_from_the_pack_is_only_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.money-bag-gives-coins",
        says: "Coins dragged out of the money bag come into the pack as one stack of that many, the \
               money goes down by as many, and the drag is answered with the container the client \
               named.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MONEY-BAG-OUT"),
        station: "yalain-testkit::cpu::play::pack::coins_dragged_out_of_the_money_bag_come_into_the_pack_and_the_money_goes_down",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.money-bag-refused-while-busy",
        says: "An item dragged to the money bag while another action is under way is not sold: the player \
               is told it is too busy and the drag is answered with the item.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MONEY-BAG-BUSY"),
        station: "yalain-testkit::cpu::play::pack::a_drag_to_the_money_bag_while_busy_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.money-bag-sells-an-item-for-its-value",
        says: "An item of the pack dragged to the money bag is sold for its value: the player is told the \
               gold added, its money goes up by it, and the item is destroyed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MONEY-BAG"),
        station: "yalain-testkit::cpu::play::pack::an_item_dragged_to_the_money_bag_is_sold_for_its_value",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.money-from-a-corpse-open",
        says: "A corpse is turned into money only by the player who has it open, as taking \
               everything from one is.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-MONEY-OPEN"),
        station: "yalain-testkit::cpu::play::integrity::a_corpse_the_player_has_not_open_is_not_turned_into_money",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.money-from-a-corpse-past-a-full-pack",
        says: "A corpse is turned into money whatever the pack holds; each item's move may run past \
               the room, and says it did when the pack holds all its room.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TRANSMUTE-PAST-ROOM"),
        station: "yalain-testkit::cpu::play::integrity::a_corpse_turned_into_money_with_a_full_pack_says_it_ran_past_the_room",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.off-hand-listed-at-entry",
        says: "A weapon in the off hand is listed there as its player enters the world, taking that \
               hand alone, and hangs from its secondary place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-OFF-HAND-ENTRY"),
        station: "yalain-testkit::cpu::play::paper_doll::a_weapon_in_the_off_hand_stays_there_across_a_login",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.one-item-one-holder",
        says: "An item in a corpse goes to one holder: a take finds the item still in its \
               container when it is through, or is refused; a taker gone by then, or with a \
               full pack, leaves it where it was.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ONE-HOLDER"),
        station: "yalain-testkit::cpu::play::integrity::an_item_taken_by_two_from_one_corpse_goes_to_one_of_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.pack-segments-are-windows-of-the-contents",
        says: "The pack's contents go as one list with the main pack's room and count, then each \
               bag's: the counts tile the list in order, each window holding its segment's items, \
               and an item coming into a bag lengthens that window and moves the later ones on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PACK-WINDOWS"),
        station: "yalain-testkit::cpu::play::pack::the_packs_segments_are_windows_that_tile_its_contents_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.paper-doll-equips-at-the-place-named",
        says: "An item dragged to the paper doll is worn or wielded at the place the client names, \
               whatever is there taken off to where the item was in the pack, the answer flagged as a \
               directive and naming the item it replaced.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PAPER-DOLL-EQUIP"),
        station: "yalain-testkit::cpu::play::paper_doll::a_ring_dragged_to_the_doll_goes_on_the_hand_named_and_takes_off_the_one_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.paper-doll-takes-off-to-the-slot-named",
        says: "A weapon dragged to the off hand is wielded there without the weapon slider; dragged \
               off the paper doll an item goes to the pack slot the client names.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PAPER-DOLL-UNEQUIP"),
        station: "yalain-testkit::cpu::play::paper_doll::a_sword_dragged_to_the_off_hand_and_off_again_goes_to_the_slot_named",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.past-every-segment-the-pack-itself-grows",
        says: "Past every segment an item goes to the pack itself and no bag grows; a slot past the \
               last bag is the pack's own. Recorded looks show the pack over its room and never a \
               bag, and the client takes no slot past the bags. Our choice: that it goes to the \
               pack's front, as no such move is recorded.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PACK-PAST-SEGMENTS"),
        station: "yalain-testkit::cpu::play::pack::an_item_past_every_segment_goes_to_the_packs_front_and_no_bag_grows",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.putting-down-takes-the-bodys-time",
        says: "Putting an item down takes as long as the body's putting-down behavior: about 1.75 \
               s for a human man, 1.97 s for Lugians and Tumeroks, 1.56 s for an Empyrean woman \
               and 3.57 s for a Drudge.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PUT-DOWN-RACE"),
        station: "yalain-testkit::cpu::play::drops::a_drudges_drop_takes_its_bodys_longer_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.reorganize-joins-a-stack",
        says: "A stack moved onto a stack of its kind with room in the pack joins it; the moved stack is \
               destroyed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REORG-MERGE"),
        station: "yalain-testkit::cpu::play::pack::a_stack_moved_onto_a_stack_of_its_kind_joins_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.reorganize-moves-an-item",
        says: "An item moved about the pack or its bags leaves its slot and goes in at the slot named, the \
               items there moving up one; the answer names where the server held it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REORG-MOVE"),
        station: "yalain-testkit::cpu::play::pack::a_move_about_the_pack_takes_the_item_to_the_slot_named_and_closes_up_behind_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.reorganize-refused-while-busy",
        says: "A move about the pack asked while another action is under way is refused and nothing moves.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REORG-BUSY"),
        station: "yalain-testkit::cpu::play::pack::a_move_about_the_pack_while_busy_is_refused_and_nothing_moves",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.reorganize-splits-a-stack",
        says: "Part of a stack moved about the pack splits off as a new stack at the slot named.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REORG-SPLIT"),
        station: "yalain-testkit::cpu::play::pack::part_of_a_stack_moved_splits_off_as_a_new_stack_at_the_slot",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.stacks-join-only-with-like-numbers",
        says: "Items join a stack only when each piece is worth the same and carries the same of \
               its own; any other goes in as an item of its own.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STACK-JOIN"),
        station: "yalain-testkit::cpu::play::stack_numbers::a_piece_worth_little_does_not_join_a_piece_worth_much",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.take-all-needs-the-container-open",
        says: "Taking everything from a container the player does not have open is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TAKE-ALL-OPEN"),
        station: "yalain-testkit::cpu::play::pack::taking_all_from_a_container_not_open_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.take-all-takes-every-item",
        says: "Taking everything from an open corpse plays the picking-up behavior, then brings every item \
               into the pack in the corpse's order, each to the front.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TAKE-ALL"),
        station: "yalain-testkit::cpu::play::pack::taking_all_from_an_open_corpse_brings_every_item_into_the_pack_in_its_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.the-pack-has-three-bags",
        says: "A pack has 42 places and three bags of 12; an item coming into a full pack goes to the front \
               of the first bag with room.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PACK-BAGS"),
        station: "yalain-testkit::cpu::play::pack::an_item_given_to_a_full_pack_goes_to_the_front_of_the_first_bag",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.units-gathered-stay-within-a-stacks-limit",
        says: "Units gathered fill a player's plain stacks to their template's limit, and the rest \
               make new stacks, none past it, and none lost. Our choice which fills first.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STACK-LIMIT"),
        station: "yalain-testkit::cpu::play::stack_numbers::units_gathered_fill_a_stack_to_its_limit_and_the_rest_make_new_stacks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.use-goes-to-the-hand-last-held",
        says: "A use puts a one-handed weapon in the hand it was last held in, else the primary \
               hand, putting away only what is in that hand and naming it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-USE-LAST-HAND"),
        station: "yalain-testkit::cpu::play::paper_doll::a_weapon_last_in_the_off_hand_goes_back_there_by_its_use",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.use-refused-by-the-quest-for-taking",
        says: "An item in the pack that its quest keeps from the player is refused when used, as a \
               take would be; with the quest where it asks, the use goes on to what the item does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-USE-TAKE-QUEST"),
        station: "yalain-testkit::cpu::play::chests::an_item_in_the_pack_its_quest_keeps_is_refused_by_its_use",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.use-with-no-place-refused",
        says: "A use of an item in the pack that names no hand or body place, and has no use of its \
               own, is refused and the item stays. Our choice: 2017 never wielded one in 931 uses, and \
               refused motes and broken medallions this way; other items with no use weren't seen.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-USE-NO-PLACE"),
        station: "yalain-testkit::cpu::play::holds::an_item_with_no_place_is_refused_by_its_use_and_stays_in_the_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.wield-keeps-the-packs-order",
        says: "Wielding answers with the item's place in the pack as the client keeps it (the pack's order, closed up behind anything taken out), before the weapon is drawn and the item moves; whatever is in the way goes back where the item was first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WIELD-ORDER"),
        station: "yalain-testkit::cpu::play::specials::wielding_names_the_items_place_and_the_pack_closes_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.inventory.worn-item-replaces-what-is-there",
        says: "Wearing an item where another is worn takes that one off first and puts it where \
               the new one was in the pack; a second ring goes on the other hand.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WEAR-REPLACE"),
        station: "yalain-testkit::cpu::play::wearing::a_second_chest_piece_replaces_the_first_which_goes_where_it_was",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.a-jump-cuts-a-use-short",
        says: "An item's use whose behavior is playing (a potion's drink) is cut short by a jump or \
               its user's death: answered as interrupted, nothing done, the item kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-CUT-SHORT"),
        station: "yalain-testkit::cpu::play::items::a_jump_or_a_death_cuts_a_drink_short",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.a-mount-needs-empty-hands-and-no-mount",
        says: "An item that mounts its user is refused while anything is in the player's hands, and while \
               a mount is already on; nothing is done.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-MOUNT-REFUSED"),
        station: "yalain-testkit::cpu::play::items::a_mount_is_refused_with_full_hands_and_on_a_mount",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.a-potion-is-drunk-then-takes-effect",
        says: "An item with a use behavior (a potion's drink) used from the pack plays it with the \
               item in hand; when it has played out, at once its effects go on the user, the item \
               is spent and the use is answered \"You drink ...\" or \"You use ...\".",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-DRINK"),
        station: "yalain-testkit::cpu::play::items::a_potion_is_drunk_and_at_the_drinks_end_takes_effect_and_is_spent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.an-effect-without-a-duration-lasts-until-taken-off",
        says: "An item's effect with no duration (a Crystalline Tyrant Egg's mount) never runs out \
               and goes at logout; the egg, not destroyed on use, is kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-UNTIL-OFF"),
        station: "yalain-testkit::cpu::play::items::an_eggs_mount_lasts_until_taken_off_its_speed_sent_and_the_egg_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.an-eggs-mount-costs-focus",
        says: "A Crystalline Tyrant Egg's use takes five from its user's focus as the mount goes on, each \
               time it is used.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-FOCUS"),
        station: "yalain-testkit::cpu::play::items::an_eggs_use_takes_five_from_the_focus",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.an-item-without-a-use-behavior-acts-at-once",
        says: "An item with no use behavior (a charm, an egg) acts in the request's instant: its \
               effects on the user, one off its stack when it is destroyed on use, the answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-AT-ONCE"),
        station: "yalain-testkit::cpu::play::items::a_charm_without_a_use_behavior_acts_at_once_and_goes_one_off_its_stack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.binds-on-use-kept",
        says: "An item's mark to bind when used in combat is kept with it across a logout.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BINDS-ON-USE-KEPT"),
        station: "yalain-testkit::cpu::entry::keeping::an_items_bind_on_use_mark_is_kept_across_a_logout",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.binds-on-use-warning",
        says: "An item that binds when used in combat is equipped with the mark in its profile and \
               the warning \"This item will become bound to you when used in combat ...\" (not on \
               top), at every equip; once its holder fights with it on, it is bound, equipped \
               with no mark and no warning.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BINDS-WARNING"),
        station: "yalain-testkit::cpu::play::binding::an_item_that_binds_in_combat_warns_at_every_equip_until_fought_with",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.binds-when-hit",
        says: "Being hit binds what a player wears and wields that binds when used in combat; what \
               it only carries keeps its mark.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BINDS-WARNING-HIT"),
        station: "yalain-testkit::cpu::play::binding::being_hit_binds_what_is_worn_but_not_what_is_carried",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.food-heals-and-wears",
        says: "A forager's food is drunk as a potion is; at the drink's end health and vigor are each \
               healed by its effects at the spellcraft of 40, the use answered with the drink's text \
               as one that took its time, and the food worn by one; at its fifth use it is destroyed \
               through overuse.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FOOD"),
        station: "yalain-testkit::cpu::play::items::a_forager_s_food_heals_health_and_vigor_and_wears_out_at_its_fifth_use",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.food-read",
        says: "A forager's food reads as a drink no use destroys, its two effects healing health and \
               vigor by 244 at the spellcraft of 40.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FOOD-READ"),
        station: "yalain-testkit::dat::effects::a_forager_s_food_reads_as_a_drink_that_heals_health_and_vigor",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.items.hidden-quest-refused-as-mysterious",
        says: "An item or object that needs a hidden quest the player does not have where it needs \
               it is refused as mysterious, never as lacking the quest or as at the wrong status.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HIDDEN-MYSTERIOUS"),
        station: "yalain-testkit::cpu::play::items::an_item_that_needs_a_hidden_quest_is_refused_as_mysterious",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.least-level",
        says: "An item used by a player below its least level is refused and does nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-LEVEL"),
        station: "yalain-testkit::cpu::play::items::an_egg_used_below_its_least_level_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.looked-at",
        says: "An item that is only looked at, used from the pack, is shown to the player as an \
               examination, then the use is answered at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-EXAMINE"),
        station: "yalain-testkit::cpu::play::items::a_trophy_used_is_shown_then_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.monarch-only",
        says: "An item for monarchs alone, used or worn by anyone else, is refused with the monarchs' refusal and does nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MONARCH-ITEM"),
        station: "yalain-testkit::cpu::play::items::an_item_for_monarchs_alone_is_refused_to_anyone_else",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.needs-its-quest",
        says: "An item that needs a quest is refused when the player never had it, or has it but not as \
               the item needs it (under way, completed).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-QUEST"),
        station: "yalain-testkit::cpu::play::items::an_item_that_needs_a_quest_under_way_is_refused_without_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.not-while-moving",
        says: "An item that may not be used moving (a Crystalline Tyrant Egg) is refused while its user \
               moves, and does nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-MOVING"),
        station: "yalain-testkit::cpu::play::items::an_egg_used_moving_is_refused_and_does_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.nothing-else-while-drinking",
        says: "While an item's use behavior plays, another use is refused as busy and does nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-BUSY"),
        station: "yalain-testkit::cpu::play::items::another_use_while_a_potion_is_drunk_is_refused_and_does_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.place-before-its-quest-step",
        says: "An item whose use moves a quest is held to its own requirements first: away from \
               its place it is refused as any item is, told how far, and the quest is not moved.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-PLACE-BEFORE-QUEST"),
        station: "yalain-testkit::cpu::play::items::a_rod_whose_use_moves_a_quest_is_refused_away_from_its_place_and_moves_it_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.take-kept-by-a-quest",
        says: "An item its quest keeps from the player is not taken: refused at once as lacking the \
               quest, as at the wrong status, or for a hidden quest as mysterious; with the quest \
               where it needs it, the item is taken.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TAKE-QUEST"),
        station: "yalain-testkit::cpu::play::chests::an_item_its_quest_keeps_is_refused_when_taken_until_the_quest_is_where_it_needs_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.tessera-gives-a-hero-skill-credit",
        says: "A Tessera used from the pack plays its use, its user free to move, and at its \
               end gives one hero skill credit more with the skill repository, is spent, and is \
               answered \"You use {the} %1.\" with flags 309.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TESSERA"),
        station: "yalain-testkit::cpu::play::items::a_tessera_used_gives_a_hero_skill_credit_and_is_spent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.training-hall-potions",
        says: "A training hall's potion, whose template names no effect, is drunk as a potion is \
               and at the drink's end puts on its one effect (haste, healing or thorns) at the \
               spellcraft of 1, spent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TRAINING-POTIONS"),
        station: "yalain-testkit::cpu::play::items::a_training_hall_potion_is_drunk_for_its_one_effect",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.treasure-potions-drunk-for-their-own-effect",
        says: "A treasure potion carries an effect of its own (one of nine kinds at its \
               dropper's tier, a spellcraft from 0 to 100), is named for it and colored by it, \
               and is drunk for it at its spellcraft.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-POTIONS"),
        station: "yalain-testkit::cpu::play::items::a_treasure_potion_is_named_colored_and_drunk_for_its_own_effect",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.items.usage-effects-read-from-their-names",
        says: "The effects an item's use puts on are read from the names its template gives: each \
               name's effect enumeration value names its effect, else its package does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-EFFECT-NAMES"),
        station: "yalain-testkit::dat::effects::the_effects_of_items_uses_read_from_their_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.items.used-only-near-its-place",
        says: "An item used farther from its place than its radius is refused, the item telling the \
               player how far off it is; within the radius it works.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-PLACE"),
        station: "yalain-testkit::cpu::play::items::an_item_used_far_from_its_place_is_refused_and_says_so",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.kingdoms.channel-of-the-kingdom",
        says: "A player's faction channel is its kingdom's: the Shadow's room 3, the Order's 4, the \
               Dominion's 5, the neutral kingdom's (a player in none too) 6, each with its kingdom \
               and its name after the world's, as every faction channel the 2017 servers sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-CHANNEL"),
        station: "yalain-testkit::corpus::chat::every_recorded_faction_channel_is_ours_for_the_kingdom_it_names",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.kingdoms.ethereality",
        says: "A player's movement ethereality names the kingdoms of the players it may not fight \
               where it stands, by the client's rule of who may fight whom: every kingdom in peace, \
               its own and the neutral one in a controlled area, its own where kingdom fights \
               kingdom, none where all fight; a player in no kingdom counts as neutral.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-ETHEREALITY"),
        station: "yalain-testkit::cpu::play::kingdoms::a_players_ethereality_passes_through_the_kingdoms_it_may_not_fight",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.kingdoms.kept",
        says: "A player's kingdom is kept: its description and its object carry it as it enters again \
               (its faction and its physics type's kingdom bit), and so does the object other \
               players are made.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-KEPT"),
        station: "yalain-testkit::cpu::play::kingdoms::a_joined_kingdom_is_kept_and_carried_by_the_players_description_and_object",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.kingdoms.plant-shows-its-planters",
        says: "A dye plant's profile names its planter's kingdom.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-PLANT"),
        station: "yalain-testkit::cpu::play::dye_plants::a_plants_profile_shows_its_planters_kingdom",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.kingdoms.profile",
        says: "A player's profile names its kingdom; a player in none is shown in the neutral \
               kingdom.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-PROFILE"),
        station: "yalain-testkit::cpu::play::kingdoms::a_players_profile_names_its_kingdom",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.kingdoms.room-reaches-its-kingdom",
        says: "A line said into a kingdom's chat room reaches the players of that kingdom alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-ROOM"),
        station: "yalain-testkit::cpu::play::kingdoms::a_kingdoms_room_line_reaches_its_kingdoms_players_alone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.kingdoms.shrine-join",
        says: "A player on foot who uses a faction shrine of another kingdom joins the shrine's \
               kingdom 3.83 s after its rating: its kingdom and physics type change for every client \
               that knows it, it is told it joined, and its channels are sent again with its \
               kingdom's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-JOIN"),
        station: "yalain-testkit::cpu::play::kingdoms::a_player_on_foot_joins_a_shrines_kingdom_a_little_after_its_rating",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.kingdoms.shrine-kinds-read",
        says: "Each faction shrine joins the kingdom its script kind names: the placed shrines are the \
               Dominion's, the neutral kingdom's and the Order's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-SHRINE-KINDS"),
        station: "yalain-testkit::dat::interactions::each_faction_shrine_joins_the_kingdom_its_script_kind_names",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.kingdoms.shrine-of-its-own",
        says: "A player of a shrine's kingdom already (a player in none at the neutral shrine) is told \
               its rating and nothing more.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-SHRINE-OWN"),
        station: "yalain-testkit::cpu::play::kingdoms::a_player_of_the_shrines_kingdom_already_joins_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.kingdoms.status-at-entry",
        says: "The session start carries the conflict status of the landblock the character enters, \
               and nothing more is said of it; a new character is told it once more after its \
               description, and its ethereality is set after the arrival effect.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-STATUS-ENTRY"),
        station: "yalain-testkit::cpu::play::kingdoms::the_session_start_carries_the_status_of_where_the_character_enters",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.kingdoms.status-on-arrival",
        says: "A player taken by a portal into a landblock of another conflict status is told it once \
               its client has drawn the world, not before.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-STATUS-ARRIVAL"),
        station: "yalain-testkit::cpu::play::kingdoms::arriving_through_a_portal_tells_the_status_once_the_world_is_drawn",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.kingdoms.status-on-the-way",
        says: "Walking into a landblock of another conflict status tells the player the new status, \
               and its ethereality follows for every client that knows it; walking into a \
               landblock of the same status says nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KINGDOM-STATUS-WALK"),
        station: "yalain-testkit::cpu::play::kingdoms::walking_into_a_landblock_of_another_status_tells_it_and_the_ethereality_follows",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.login.a-new-session-enters-after-a-log-off",
        says: "A new client of an account whose character has logged off and left enters that \
               character at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOG-IN-A-ROW"),
        station: "yalain-testkit::cpu::entry::logging_in_again::a_new_session_after_a_log_off_enters_the_character_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.login.answers-in-the-recorded-order",
        says: "A login is answered with the world's name and a question about the client's data; \
               the client's answer with the end of the download, the account's characters by name, \
               then with their looks, each on the queue the 2017 login servers used.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LOGIN-SHAPE"),
        station: "yalain-testkit::cpu::entry::character_select::a_login_is_answered_in_the_order_and_on_the_queues_of_2017",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.login.log-off-at-the-list-unanswered",
        says: "A log-off from a client at the character list, as the client sends after a refused \
               entry, is not answered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOG-LOGOFF-AT-LIST"),
        station: "yalain-testkit::cpu::entry::logging_in_again::a_log_off_at_the_character_list_is_not_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.login.logging-in-again-logs-off-the-character-left-in-the-world",
        says: "A new login of an account whose character a dead client left in the world logs that \
               character off, about nineteen seconds as any log-off; entering it is refused until \
               it has left.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOG-NEW-LOGIN"),
        station: "yalain-testkit::cpu::entry::logging_in_again::a_new_login_logs_off_the_character_a_killed_client_left_in_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.corpse-closes-when-its-opener-walks-away",
        says: "An open corpse whose opener goes more than six meters from it closes: its opener is                told, as when it asks to close it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CORPSE-WALK-AWAY"),
        station: "yalain-testkit::cpu::play::fights::an_open_corpse_closes_when_its_opener_walks_away",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.one-viewer-at-a-time",
        says: "A corpse is open to one player at a time: another player's use of it while it is \
               open is refused and opens nothing, until the first closes it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LOOT-ONE-VIEWER"),
        station: "yalain-testkit::cpu::play::fights::a_corpse_another_player_has_open_refuses_the_use_until_it_is_closed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.open-and-take",
        says: "Using a corpse opens it: the opener is told what it holds and of each item; using \
               an item in it takes it, and once the taking is through the move is answered and \
               the item is the player's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LOOT-TAKE"),
        station: "yalain-testkit::cpu::play::fights::a_corpse_opens_to_its_killer_and_an_item_taken_from_it_is_the_players",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.quest-items-kept-from-money",
        says: "Turning a corpse's items into money keeps back a quest item and an item worth \
               nothing: it stays in the corpse, listed as not turned into money with error 1105; \
               the answer's status is 1105 too when nothing was turned into money.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TRANSMUTE-KEPT"),
        station: "yalain-testkit::cpu::play::fights::turning_a_corpses_items_into_money_keeps_an_item_worth_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-effect-on-hit",
        says: "What a treasure weapon puts on what it hits comes on by that effect's own chance as \
               its hits land. Our choice where a hit reads it; the chance is the effect's.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-ON-HIT"),
        station: "yalain-testkit::cpu::play::fights::a_weapons_effect_on_a_hit_goes_on_what_it_hits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-enchantments",
        says: "A treasure weapon, shield or piece of armor may draw enchantments: how many by its \
               sort and level, each family by how often 2017 showed it, a family of five at the tier \
               of its level. One on the item itself changes its numbers; a weapon glows as its \
               first says. Our choice, a fit to the recorded items.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-ENCHANT"),
        station: "yalain-testkit::cpu::play::fights::an_items_enchantments_follow_its_sort_and_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-jewelry-enchantments",
        says: "Treasure jewelry is plain with no least level, or carries one effect on its wearer \
               and a least level: how often by its dropper's level, which by how often 2017 showed \
               it. Our choice, a fit to the recorded items.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-JEWELRY"),
        station: "yalain-testkit::cpu::play::fights::treasure_jewelry_is_plain_or_has_one_effect_and_a_least_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-look-drawn",
        says: "A treasure item of a template seen to draw its look draws values for its look's \
               appearance keys as it drops: a weapon's glow, a piece of armor's colors. They color it, \
               change its icon and are worn with it, and a dye takes their place. Our choice, a fit \
               to the items 2017 made.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-LOOK"),
        station: "yalain-testkit::cpu::play::fights::a_weapon_a_creature_drops_draws_its_look",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-named-for-an-enchantment",
        says: "A treasure item with enchantments is named for one of them, drawn evenly, in the \
               suffix its name holds for the effect. Our choice which; its name's shape is 2017's.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-NAMED"),
        station: "yalain-testkit::cpu::entry::examine::a_treasure_item_is_named_for_its_enchantment_in_its_create_and_its_profile",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-numbers-by-level",
        says: "A treasure weapon, shield or piece of armor has its own damage, rating, delay, vigor \
               cost and slots by its level, near what a made one has there, in place of its \
               template's placeholders.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-NUMBERS"),
        station: "yalain-testkit::cpu::play::fights::a_treasure_items_numbers_follow_its_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-own-properties",
        says: "An item drawn from a creature's loot profile is given what its dropper's level \
               gives it of its own: equipment whose template names no least level its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-OWN"),
        station: "yalain-testkit::cpu::play::fights::equipment_a_creature_drops_carries_a_least_level_of_its_own_by_the_creatures_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-own-properties-by-level",
        says: "A treasure item's own least level is its dropper's loot base plus 0 to 3, at \
               most 50; its Arcane Lore comes only above a base of 51, from the game's table; a \
               potion's effect is of its dropper level's tier, at a spellcraft from 0 to 100.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-BY-LEVEL"),
        station: "yalain-testkit::cpu::play::fights::a_treasure_items_own_properties_follow_its_droppers_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-paired-enchantments",
        says: "A counter that lowers armor comes with the Armor Ward and Burning Edge's fire with its \
               edge, each pair one draw from the level that reached it, showing two lines; neither \
               half comes alone, and an item is named for neither the counter, the ward nor the \
               fire. Our choice, a fit to the recorded items.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-PAIRS"),
        station: "yalain-testkit::cpu::play::fights::a_paired_enchantment_comes_with_its_other_half_and_names_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-rarer-enchantments",
        says: "A treasure weapon may draw a slayer, an edge that narrows its span, a ward or counter \
               on who wields it and the strongest effects on a hit; a piece of armor a skill's boost \
               on who wears it. Each by how often 2017 showed it, from the level that reached it. \
               Our choice, a fit to the recorded items.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-RARER"),
        station: "yalain-testkit::cpu::play::fights::the_rarer_enchantments_are_drawn_rarely_and_from_their_levels",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-slayer",
        says: "A slayer harms what it names by its spellcraft as each hit lands, basic or special, \
               told as its own story past the creature's armor, and does nothing to any other. \
               Our choice that it goes by spellcraft: one weapon measured.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-SLAYER"),
        station: "yalain-testkit::cpu::play::fights::a_slayer_harms_the_creatures_it_names_by_its_spellcraft_and_no_others",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.treasure-word",
        says: "A treasure item's name has a word before it by its sort, its level and its combat \
               delay, in its create and its profile. Our choice, a fit to the recorded items.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-WORD"),
        station: "yalain-testkit::cpu::play::fights::a_treasure_items_word_goes_by_its_sort_level_and_combat_delay",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.loot.turned-into-money",
        says: "Turning a corpse's items into money moves each into the pack and destroys it \
               there, the money going up by its value, and the answer says what was earned; each \
               item is let go of (its holder none) before it is gone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TRANSMUTE"),
        station: "yalain-testkit::cpu::play::fights::turning_a_corpses_items_into_money_pays_their_value",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.lying-client.a-call-too-big-is-dropped",
        says: "A client's call over 64 KB is dropped: the largest of the 299,860 calls \
               recorded in 2017 was 7,676 bytes.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-LYING-BIG"),
        station: "yalain-testkit::cpu::play::lying_client::a_call_far_too_big_is_neither_kept_nor_said",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.lying-client.calls-hurt-nobody",
        says: "Every call a client may make, of the shape it reads and with every value a \
               lie, from fixed seeds, panics nothing and takes nothing of another player.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-LYING-CALLS"),
        station: "yalain-testkit::cpu::play::lying_client::every_call_of_its_shape_with_every_value_a_lie_does_no_harm",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.lying-client.messages-cut-and-bitten-do-no-harm",
        says: "Messages cut short, bitten, grown and with counts inflated, from fixed \
               seeds, through the real parser on every queue, panic nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-LYING-BYTES"),
        station: "yalain-testkit::cpu::play::lying_client::messages_cut_short_and_bitten_panic_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.lying-client.recorded-messages-bitten-do-no-harm",
        says: "Every kind of message and call the 2017 clients sent, bitten, cut short and \
               with counts inflated from a fixed seed, through the real parser and into a \
               world, panics nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-LYING-RECORDED"),
        station: "yalain-testkit::corpus::replay::every_kind_of_recorded_message_bitten_panics_nothing",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.market.a-consigned-item-keeps-what-it-holds",
        says: "An item on sale holds what it held in the pack: taken back unsold it comes as a new \
               item with the old one's value and own numbers, a restart leaves it as it was, and \
               a seller collecting its price leaves the buyer's item alone. A record from before \
               items' numbers were kept still reads, its items plain. Our choice: recordings show \
               a bought item keeping its id and a plain stack taken back under a new one, but no \
               made item taken back, no seller collecting and no restart.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-MARKET-ITEM-KEPT"),
        station: "yalain-testkit::cpu::play::market::an_item_taken_back_from_a_broker_keeps_what_it_holds_of_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.market.a-consignment-comes-back-only-unsold-with-room-and-its-template",
        says: "A consignment comes back to its seller only unsold, with a place in the pack \
               and its template in the world; anything else is answered and leaves the sale \
               up and whole. A sold item stays its buyer's, its price still to be \
               collected. A sale of a template the world no longer holds isn't bought, and \
               part of a stack of one isn't put up.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-MARKET-TAKE-BACK"),
        station: "yalain-testkit::cpu::play::market::a_sold_consignment_stays_up_with_its_price_and_its_buyers_item_stays_whole",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.market.catalog-consignments-and-player-sales",
        says: "Using a broker answers the use, then shows its catalog; an item put up for sale \
               takes a hundredth of what it would fetch, moves to the broker and shows the \
               consignment; another player sees the sale in its store and buys it (the price \
               taken, the item made in its pack, the sale's stock told as gone); the seller \
               collects the price.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MARKET"),
        station: "yalain-testkit::cpu::play::market::an_item_put_up_with_a_broker_is_bought_by_another_player_and_its_price_collected",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.market.dealings-in-reach-and-with-room",
        says: "A broker's store, consignments, sales and collecting answer only a player in reach; a \
               sale bought into a full pack is refused for want of room and neither paid \
               nor moved.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-MARKET-LIMITS"),
        station: "yalain-testkit::cpu::play::market::a_brokers_dealings_need_reach_and_room",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.mounts.dismount",
        says: "A rider asking to dismount is told it is not mounted and shown its own look and \
               speed again, the mount's glow stopped and its parting one played; a follow under way \
               stops first; a player not mounted is not answered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-DISMOUNT"),
        station: "yalain-testkit::cpu::play::mounts::a_rider_dismounting_is_shown_its_own_look_and_speed_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.mounts.egg-mounts-until-dismounted",
        says: "A Crystalline Tyrant Egg mounts its user with no glow until it dismounts, costs five \
               focus, and is kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-EGG"),
        station: "yalain-testkit::cpu::play::mounts::an_egg_mounts_its_user_until_dismounted_and_is_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.mounts.mounted-look",
        says: "A rider looks like its mount's look for its race and sex, wearing its own appearance \
               over its body's defaults.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-LOOK"),
        station: "yalain-testkit::cpu::play::mounts::the_mounted_look_carries_the_riders_appearance_over_its_bodys_defaults",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.mounts.pet-orders-refused",
        says: "A rider's order to its pets to attack is refused with its own text.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-PETS"),
        station: "yalain-testkit::cpu::play::mounts::a_riders_pet_attack_order_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.mounts.read-from-their-definitions",
        says: "Saddles' and eggs' mount effects, the mount table's looks for every type, race and \
               sex, and the bodies' own appearance read from the data.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-DEFS"),
        station: "yalain-testkit::dat::mounts::the_mounts_read_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.mounts.refused",
        says: "A saddle or an egg is refused while moving, with something in hand, when already \
               mounted, or indoors.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-REFUSED"),
        station: "yalain-testkit::cpu::play::mounts::a_mount_item_is_refused_in_hand_indoors_moving_or_mounted",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.mounts.ride-ends-at-logoff",
        says: "A rider who logs off leaves without its mount, shown in its own look.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-LOGOFF"),
        station: "yalain-testkit::cpu::play::mounts::a_rider_logging_off_leaves_without_its_mount",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.mounts.ride-ends-at-resurrection",
        says: "A resurrection ends a ride as a dismount does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-RESURRECT"),
        station: "yalain-testkit::cpu::play::mounts::a_resurrection_ends_a_ride",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.mounts.ride-ends-indoors",
        says: "A rider who goes indoors is put off its mount a few seconds later, or a moment after \
               arriving inside by portal.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-INDOORS"),
        station: "yalain-testkit::cpu::play::mounts::a_rider_indoors_is_put_off_its_mount_a_few_seconds_later",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.mounts.ride-runs-out",
        says: "A saddle's ride ends when its time is up, as a dismount ends it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-EXPIRES"),
        station: "yalain-testkit::cpu::play::mounts::a_saddles_ride_ends_when_its_time_is_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.mounts.saddle-mounts-its-user",
        says: "An Ataur saddle used from the pack mounts its user for its time: the mount's glow, \
               its speed and its look for all who see, the saddle spent.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-SADDLE"),
        station: "yalain-testkit::cpu::play::mounts::a_saddle_used_from_the_pack_mounts_its_user_and_is_spent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.mounts.seen-mounted",
        says: "A player who comes into view of a rider sees it on its mount, at its speed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MOUNT-SEEN"),
        station: "yalain-testkit::cpu::play::mounts::a_player_who_comes_into_view_sees_the_rider_mounted",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.cookies-and-seeds-from-the-hosts-source",
        says: "Every cookie and checksum seed the server sends a client, a login's handshake, a \
               referral and the world's handshake, is taken from the random source its host gave \
               it; the server program's is the system's.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-COOKIES-SOURCE"),
        station: "yalain-net::cpu::cookies::every_cookie_and_checksum_seed_sent_comes_from_the_source_the_host_gave",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.failed-random-source-makes-no-connection",
        says: "With the random source failing, a login or world login gets no handshake and makes \
               no connection, the client's resend being served once it is back and a world \
               login's cookie staying good, and a referral closes the login, telling the client. \
               No number of the server's own stands in.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-COOKIES-FAILED"),
        station: "yalain-net::cpu::cookies::a_failed_random_source_gives_no_handshake_and_no_number_of_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.game-clock-held",
        says: "A server set to hold its game clock reads one time from then on, and the clock \
               every connection sends says that time, so every client shows the same moment of \
               the game's day.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CLOCK-HELD"),
        station: "yalain-testkit::cpu::entry::transport::a_held_game_clock_stands_still_for_the_server_and_in_what_it_sends",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.large-message-holds-only-its-queue",
        says: "A message sent after a large one on another queue goes at once; one on the large \
               message's own queue goes after it, so each queue keeps its order.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PACE-QUEUES"),
        station: "yalain-net::cpu::pace::a_large_message_holds_back_only_the_messages_on_its_own_queue",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.large-message-paced",
        says: "A server sends a message longer than 32 KB at most 32 KB each half second, after \
               what it resends and its other messages, and answers each request to send again \
               at once: a seven-megabyte record arrives whole over a link that loses packets, in \
               about two minutes, and nothing sent is let go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PACE-RATE"),
        station: "yalain-net::cpu::pace::a_seven_megabyte_message_arrives_whole_over_a_lossy_link_at_the_2013_servers_pace",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.large-message-rate-setting",
        says: "A host may set the rate of large messages between 4 KiB and 256 KiB a second, the \
               2013 servers' 64 KiB unless it does: at the highest a seven-megabyte record arrives \
               whole over a link that loses packets, with never more out than the window, nothing \
               let go; at the lowest a large message still arrives; any other rate stops the server \
               at its start.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PACE-SETTING"),
        station: "yalain-net::cpu::pace::a_seven_megabyte_message_at_the_highest_rate_arrives_whole_and_loses_nothing_it_sent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.large-message-window",
        says: "A server's large message waits while 1,140 packets it sent are unacknowledged, \
               ten of the client's requests to send again, so a link that goes down never leaves \
               more lost than the client asks for again in six seconds; a request to send again \
               acknowledges what comes before it, and is answered whole.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PACE-WINDOW"),
        station: "yalain-net::cpu::pace::a_large_message_never_has_more_out_than_the_client_can_ask_for_again_before_it_is_let_go",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.large-record-served-over-real-sockets",
        says: "The server's executable serves the install's script library, seven megabytes \
               compressed, to a client on loopback that asks for it, and the client takes it \
               whole.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PACE-LIVE"),
        station: "yalain::live::session::a_client_on_loopback_takes_the_script_library_whole_from_the_server_executable",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.net.lost-packets-resent",
        says: "Over a link that loses packets both ways, a login still completes: the side that \
               misses a packet asks for it again and the other resends it under its original key.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LOSS"),
        station: "yalain-testkit::cpu::entry::transport::a_login_completes_over_a_link_that_loses_every_fifth_datagram",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.nearby-clients-referred-where-they-reached-the-world",
        says: "A client close by, on the server's machine or a private, link-local or shared \
               network, is referred to the world at the address it reached the server by: \
               loopback for one on the machine, the machine's address on the network for one on \
               it. Anyone else is referred to the address the world names for players from \
               elsewhere.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-REFER-NEARBY"),
        station: "yalain-testkit::cpu::entry::character_select::each_client_is_referred_to_the_world_at_the_address_it_reached_the_login_by",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.nearby-world-login-by-its-cookie",
        says: "A client the world referred to an address on a network close by enters the world \
               by the referral's cookie from any address on such a network, though its login came \
               from another (one through the router's public address arrives from the router); \
               from elsewhere a world login must come from its login's address, and a cookie \
               never sent lets no one in.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NEARBY-WORLD-LOGIN"),
        station: "yalain-testkit::cpu::entry::character_select::a_client_on_the_worlds_network_enters_by_its_cookie_from_another_address_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.referral-cookie-used-once",
        says: "A referral's cookie lets in one world connection. A resend while its handshake \
               waits makes no second connection; a world login with it after that makes none and \
               ends nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-COOKIE-ONCE"),
        station: "yalain-net::cpu::cookies::a_referrals_cookie_lets_in_one_world_connection_and_a_resend_changes_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.net.world-login-from-its-logins-address",
        says: "A world login from another address than its login's is refused and spends nothing \
               (but for a referral to a network close by, from that kind of network), so a cookie \
               read off the wire lets no other address in. A player whose address changed gets in \
               by logging in again.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WORLD-LOGIN-ADDRESS"),
        station: "yalain-net::cpu::cookies::a_world_login_from_another_address_than_its_login_is_refused_and_spends_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.answers-owed-capped",
        says: "A world owed a hundred answers already is not answered again, and its tell not shown, until \
               it is owed fewer; an answer the hub stopped waiting for after thirty seconds is dropped.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-ANSWERS"),
        station: "yalain-federation::cpu::tells::a_world_owed_a_hundred_answers_is_answered_no_more_until_it_is_owed_fewer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.channel-invitation-shown-as-a-tell",
        says: "An invitation to a channel is shown as a tell from the inviter would be: to a \
               present character on the network, never past an ignore of the inviter, its world or \
               the channel, or the operator's blocks.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CHANNEL-INVITE"),
        station: "yalain-federation::cpu::channels::an_invitation_is_shown_only_as_a_tell_from_the_inviter_would_be",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.channel-line-shown-as-a-tell",
        says: "A channel's line reaches a member as a tell from the channel's address, its text the \
               speaker's address and the line, never past an ignore of the speaker or the channel, \
               a block, or to one not present.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CHANNEL-LINE"),
        station: "yalain-federation::cpu::channels::a_channels_line_reaches_a_member_as_a_tell_from_the_channels_address",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.channel-requests-answered",
        says: "A channel request goes to the hub under the character's address, and each answer is \
               shown as the network's own line for it; off the network nothing goes, and a request \
               never answered ends in the network's line.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CHANNEL-REQUESTS"),
        station: "yalain-federation::cpu::channels::a_channel_request_goes_to_the_hub_and_its_answer_is_shown",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.directory-answers",
        says: "A listing refused after @dn on is shown and holds the character out until its next \
               login or @dn on; one the hub held back to its pace goes again a minute later, the \
               latest profile then, its page then shown.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-DIR-ANSWERS"),
        station: "yalain-federation::cpu::directory::a_refusal_holds_the_character_out_and_one_held_back_goes_a_minute_later",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.directory-changes-paced",
        says: "A listed character's change while it plays goes once its last listing is five \
               minutes old, the latest profile alone; an unchanged profile or one out of the \
               directory's bounds sends nothing; at logout a changed profile goes at once.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-DIR-CHANGES"),
        station: "yalain-federation::cpu::directory::a_change_waits_its_five_minutes_and_a_logout_sends_it_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.directory-listed-on",
        says: "A character on the network is listed with its Asheron's Call 2 profile at once at \
               each login and at @dn on, which asks the world for its profile and shows its page; a \
               character off the network is never listed.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-DIR-ON"),
        station: "yalain-federation::cpu::directory::a_character_on_the_network_is_listed_at_login_and_at_dn_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.directory-off-kept",
        says: "Going off the network takes the character out of the directory at once; the listing \
               that says so is owed until the hub answers it, across a lost link and a restart, and \
               goes before any profile; a profile not answered goes again after a lost link.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-DIR-OFF"),
        station: "yalain-federation::cpu::directory::going_off_takes_the_character_out_at_once_and_is_owed_until_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.directory-paced",
        says: "The directory's listings go after everything else the link carries, within the hub's \
               pace for a world's listings, and each goes once the pace allows.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-DIR-PACED"),
        station: "yalain-federation::cpu::directory::listings_go_after_the_rest_within_the_hubs_pace",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.directory-rename",
        says: "A renamed character on the network is taken out of the directory under the name it had, \
               at once, and listed under the new one; one the hub holds out stays out under the new \
               name.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NETWORK-RENAME-LISTING"),
        station: "yalain-federation::cpu::directory::a_renamed_character_leaves_the_directory_under_its_old_name_and_is_listed_under_its_new",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.directory-taken-out-by-operator",
        says: "The hub's operator taking a character out: the world says it heard at once, tells \
               the character (at its next login when away), and sends nothing for it until its next \
               login.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-DIR-OPERATOR"),
        station: "yalain-federation::cpu::directory::the_hubs_operator_holds_a_character_out_until_its_next_login",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.directory-taken-out-by-person",
        says: "A character whose person asked the network to forget it: the world says it heard \
               at once, takes the character off the network, kept, and tells it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-DIR-PERSON"),
        station: "yalain-federation::cpu::directory::a_character_its_person_forgot_goes_off_the_network",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.frames-paced-in-order",
        says: "The world's own tells go to the hub first, then the answers it owes other worlds, then \
               watches and presence answers a world at a time, each at its share of the hub's pace; \
               a newer watch takes a waiting one's place, and none waits while the link is down.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-ORDER"),
        station: "yalain-federation::cpu::presence::watches_wait_behind_tells_and_answers_and_go_a_world_at_a_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.friends-state-paced",
        says: "A character is shown at most four changes of one friend on another world at once, then \
               one every five seconds, its latest state.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-PRESENCE-PACE"),
        station: "yalain-federation::cpu::presence::a_friends_changes_are_shown_four_at_once_then_one_every_five_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.friends-state-shown",
        says: "A friend's state on another world reaches the character that lists it as its world \
               reports it, the first report a state and later ones news; friends of a world that \
               goes, or of a lost hub, show offline.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-PRESENCE"),
        station: "yalain-federation::cpu::presence::a_friends_state_is_shown_as_its_world_reports_it_and_offline_when_its_world_goes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.friends-watched",
        says: "A present character on the network watches each friend on a linked world: at the \
               welcome, when it arrives, when the friend's world comes up and when it adds one; it \
               stops when it leaves or removes one.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-WATCHING"),
        station: "yalain-federation::cpu::presence::present_characters_on_the_network_watch_their_friends_on_linked_worlds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-a-line-each-way",
        says: "Through the world's own chat service a line said in the room carrying the Global \
               room goes to the hub from a character on the network, a line from another world \
               reaches this world's players in that room, a player who squelches its world does \
               not hear it, and @dng is a line said in that room from anywhere.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-EACH-WAY"),
        station: "yalain::bin.yalain::friends::tests::a_line_goes_each_way_between_the_world_and_the_hub_and_dng_says_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-admin-word",
        says: "dn admin global says where the world carries the Global room and whence it knows, \
               asks the hub to move it with a room, tells whoever asked what the hub answered, \
               and says so for a word that is no room, a link down, a hub that keeps no room and \
               a link that goes first.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-ADMIN-WORD"),
        station: "yalain-federation::cpu::rooms::dn_admin_global_says_where_the_room_is_and_asks_the_hub_to_move_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-as-the-hub-says",
        says: "The room the hub keeps has the last word over the setting, holds at once, joining \
               or leaving the Global room as it goes on or off, and is kept across a restart.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-HUB-WORD"),
        station: "yalain-federation::cpu::rooms::the_hubs_room_has_the_last_word_and_is_kept_across_a_restart",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-carried-as-the-setting-says",
        says: "A world joins the Global room at its welcome, ahead of what was queued, and a hub \
               that holds no room for it is told the room its setting names, once on each link.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-SEED"),
        station: "yalain-federation::cpu::rooms::a_world_joins_the_global_room_and_tells_a_hub_that_holds_none_its_setting",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-empty-carries-nothing",
        says: "A world whose setting is empty joins no room, tells a hub that holds none off, \
               sends no line and shows no line, and its status says it carries the Global room in \
               no room.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-EMPTY"),
        station: "yalain-federation::cpu::rooms::a_world_whose_setting_is_empty_carries_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-line-in",
        says: "A line from another world is kept for the players in the room under its speaker's \
               address, made safe and marked with where it was said; a line of this world's own, \
               one the operator blocks, one left empty, one in another room and one in the \
               world's own room reach nobody.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-LINE-IN"),
        station: "yalain-federation::cpu::rooms::a_line_from_another_world_is_kept_for_the_room_under_its_speakers_address",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-line-in-paced",
        says: "Another world's lines are held to the room's pace by this world too, a world at a \
               time.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-LINE-PACED"),
        station: "yalain-federation::cpu::rooms::another_worlds_lines_are_held_to_the_rooms_pace_by_this_world_too",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-line-out",
        says: "A line a character on the network says in the room that carries the Global room \
               goes to the hub under its address; off the network, or in another room, it stays \
               here, and the line follows the room when the hub moves it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-LINE-OUT"),
        station: "yalain-federation::cpu::rooms::a_line_said_in_the_carrying_room_goes_to_the_hub_under_its_speakers_address",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-line-out-after-the-link",
        says: "A line waits for the link as a tell does, behind the room's joining, and one the \
               link took down with it is sent again.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-LINE-WAITS"),
        station: "yalain-federation::cpu::rooms::a_line_waits_for_the_link_and_is_sent_again_when_the_link_went",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-line-out-refused",
        says: "A speaker past its own pace is told so and its line stays here, and one the hub \
               refuses for the world's share of the room is told that.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-LINE-REFUSED"),
        station: "yalain-federation::cpu::rooms::a_line_past_its_pace_or_held_back_by_the_hub_tells_its_speaker",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-pvp-each-way",
        says: "The network's roleplay is the PvP room each way, and a world whose setting is \
               empty carries nothing either way and answers @dn global that it does not carry the \
               room.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-PVP"),
        station: "yalain::bin.yalain::friends::tests::roleplay_is_the_pvp_room_each_way_and_an_empty_setting_carries_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.global-room-words-name-this-clients-rooms",
        says: "The network's general and trade are General and Trade; this client has no room for \
               looking for company or for roleplay, so lfg and roleplay are PvP; nothing else is \
               a room.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-WORDS"),
        station: "yalain-federation::cpu::rooms::each_of_the_hubs_room_names_is_a_room_this_client_has",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.linked-to-a-hub",
        says: "A server on the Dereth Network links to its hub with the world's key and says hello \
               first; once welcomed it answers another world's tell and watch, and its console \
               shows the hub connected and blocks a world.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-LINK"),
        station: "yalain::live::network::the_server_links_to_its_hub_and_answers_another_worlds_tell_and_watch",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.network.operator-admin-words",
        says: "dn admin block is dn block as the game says it, at the console or by an admin in \
               game; a name may carry the mark a line from Discord shows; dn admin alone is the \
               help.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-ADMIN-WORDS"),
        station: "yalain-federation::cpu::operator::the_operators_words_read_the_same_after_admin_and_in_game",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.operator-block-at-once",
        says: "An operator's block takes hold at once: what it covers sees this world's characters \
               offline, is no longer watched and shows offline here; lifting it gives back what it \
               took.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-BLOCK"),
        station: "yalain-federation::cpu::operator::a_block_takes_hold_at_once_and_lifting_it_gives_back_what_it_took",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.operator-console",
        says: "The console's dn shows the world's link and its blocks, dn block and dn unblock (their \
               word in any case) block or lift a world or Name@world for everyone on this world, and \
               with the network off each says so.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-CONSOLE"),
        station: "yalain-federation::cpu::operator::the_console_shows_the_link_and_the_blocks_and_blocks_and_lifts",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.operator-words-in-game-for-admins",
        says: "The network's operator words said in game need an admin's account, as its verification does: \
               an admin's line is kept marked as an admin's; a player's is the friends service's, which \
               answers with its help, and is logged; nobody hears it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-OPERATOR-SAID"),
        station: "yalain-testkit::cpu::operator::moderation::dn_admin_said_in_game_is_only_an_admins",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.own-rooms-mirrored",
        says: "The world's own rooms the hub mirrors are joined by their names and left when they \
               stop; a line there goes to the hub under the room's name and its speaker is told \
               of Discord; a room the hub says it does not mirror keeps its lines here for a \
               minute; the room carrying the Global room is never one, and two names for one room \
               go under the first.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OWN-ROOMS-MIRRORED"),
        station: "yalain-federation::cpu::rooms::the_worlds_own_rooms_the_hub_mirrors_are_joined_and_their_lines_go_to_discord",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.person-friend-added",
        says: "A linked character's add on another world goes to the hub, within its pace, and \
               nothing is kept here: the client's own add is answered when the view names the friend \
               and its world says its state, or after three seconds, refused with the world's line; \
               `@dn friends add` is told as the hub answers. With the link down it keeps the friend \
               itself.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-FRIEND-ADDED"),
        station: "yalain-friends::cpu::network::a_linked_characters_add_is_the_hubs_and_answered_when_the_view_names_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.person-friend-removed",
        says: "Removing a row a person friend listed removes that person friend at the hub, by its \
               handle, quietly and whatever the pace. A row that went in play is answered as removed \
               while the client may still show it, until its next list.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-FRIEND-REMOVED"),
        station: "yalain-friends::cpu::network::removing_a_row_is_answered_done_while_the_client_may_show_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.person-friends-carried",
        says: "The first view saying a character is linked carries the friends it keeps on other \
               worlds to its person, once and quietly; one the view names as a handle is taken off \
               its kept list, the store too, and listed once. The first view after an unlink hands \
               them back to be kept, unless the character never asked for its link.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-FRIENDS-CARRIED"),
        station: "yalain-federation::cpu::persons::the_first_view_carries_kept_friends_and_a_handle_is_no_longer_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.person-friends-listed",
        says: "What a person view's person friends name is listed for a character while the view \
               names it and the character is here and on the network: watched, in the view's order, \
               each once, not its own world's, at most 100 with what it keeps, and never kept. A \
               character of a person friend other than its handle coming online is shown as that \
               person.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-FRIENDS-LISTED"),
        station: "yalain-federation::cpu::persons::a_view_lists_what_its_person_friends_name_while_it_names_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.person-link-and-unlink",
        says: "Said @dn link and @dn unlink ask the hub under the character's address, and each \
               answer is shown as the network's own line for it; off the network or past its pace \
               nothing goes and it is told why; a request never answered ends in the network's \
               line.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-LINK"),
        station: "yalain-federation::cpu::persons::link_and_unlink_ask_the_hub_and_show_its_answer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.person-link-holds-for-its-asker",
        says: "A link holds only for the character that asked for it, kept across a restart: one \
               that never asked is taken as unlinked and the hub told so quietly; one that asked is \
               told when it becomes linked while here, and not when it arrives linked.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-ASKED"),
        station: "yalain-federation::cpu::persons::a_link_holds_only_for_the_character_that_asked_and_is_told_when_it_comes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.person-request-outlives-a-link",
        says: "A person request handed to a link that goes before the hub answers goes again on the \
               next link.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-OUTLIVES"),
        station: "yalain-federation::cpu::persons::a_person_request_on_a_link_that_goes_goes_again_on_the_next",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.person-view-watched",
        says: "A present character on the network asks for its person view, again at each welcome, \
               and stops when it leaves or goes off the network.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-VIEW"),
        station: "yalain-federation::cpu::persons::the_person_view_is_watched_while_a_character_is_present_on_the_network",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.reach-answered",
        says: "The hub's question about a tell to a character away is true only for one of this \
               world's, away, on the network, that asked for its link, ignoring neither the teller \
               nor its world, by the list kept for it, the operator blocking neither, within the \
               pace of that world's tells to it; false for anything else, alike.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-REACH"),
        station: "yalain-federation::cpu::persons::the_hubs_question_about_a_character_away_is_true_only_as_a_shown_tell_would_be",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.rename-seen-at-once",
        says: "A rename reaches the network at once, without a restart: other worlds see the old \
               address go offline and its watchers are forgotten; the character stays on the network, \
               watches its friends and answers under the new name, and a friend's world answering again \
               shows its client nothing new.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NETWORK-RENAME"),
        station: "yalain-friends::cpu::network::a_renamed_character_goes_on_on_the_network_under_its_new_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.say-from-the-worlds-own-facts",
        says: "A line the hub asks a character to say reads the world's own facts of it: shown to \
               the room's players marked with where it was said, except one who ignores it, and \
               refused for no such character, a gag or a ban, in the world or not.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SAY-WORLD-FACTS"),
        station: "yalain::bin.yalain::friends::tests::a_say_from_the_hub_reads_the_worlds_own_facts_of_the_character",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.say-refused",
        says: "A line the hub asks for is refused with its reason, no_character, not_on_network, \
               not_linked, banned, gagged, room_off or rate_limited, or with none when nothing is \
               left of it, and nothing is shown or sent.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SAY-REFUSED"),
        station: "yalain-federation::cpu::rooms::a_line_the_hub_asks_for_is_refused_for_each_reason_and_nothing_is_shown",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.say-said-from-discord",
        says: "A line the hub asks a character to say reaches the room's players marked with \
               where it was said, goes on to the hub with the same mark, and is answered said; \
               the character is told nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SAY-SAID"),
        station: "yalain-federation::cpu::rooms::a_line_the_hub_asks_for_is_said_as_the_character_marked_with_where_it_was_said",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.setting-and-blocks-kept",
        says: "Each character's place on the network and the operator's blocks are kept across a \
               restart; a database kept before the network opens with everyone off it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-KEPT"),
        station: "yalain-store::cpu::persistence::an_earlier_database_gains_the_networks_setting_and_blocks_and_keeps_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.tell-away-linked",
        says: "A tell within the world, by address or the game's own, to a character away, on the \
               network and linked by itself goes to the hub under its own address, so it may reach \
               its player's Discord, and the teller is answered as for one not logged in; every \
               other stays the world's. Its reply comes as a tell from that address, shown as any \
               tell and written as one of the world's own.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TELL-AWAY-LINKED"),
        station: "yalain-federation::cpu::tells::a_tell_within_the_world_to_an_away_linked_character_goes_to_the_hub",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.tell-received",
        says: "A tell from another world is shown to a present character on the network after the \
               network's text rules and answered at once: delivered, or away with its text (or the \
               default for its state).",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-TELL-IN"),
        station: "yalain-federation::cpu::tells::a_tell_from_another_world_is_shown_after_the_text_rules_and_answered_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.tell-received-refused",
        says: "A tell from another world to a character not present, off the network or unknown, or \
               from what the operator blocks, is answered offline alike; one the character ignores \
               squelched; one world's tells to one character past twenty, then one a second, \
               rate_limited.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-TELL-IN-REFUSED"),
        station: "yalain-federation::cpu::tells::a_tell_from_another_world_is_refused_alike_and_held_to_its_pace",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.tell-refused-before-sending",
        says: "A tell to another world is refused before it is sent, with its line, from a world or a \
               character off the network, to what the operator blocks, and past the teller's own \
               pace.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-TELL-REFUSED"),
        station: "yalain-federation::cpu::tells::a_tell_is_refused_here_for_the_world_the_teller_the_blocks_and_the_tellers_pace",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.tell-sent-and-answered",
        says: "A tell to another world goes to the hub under the teller's address and ends as the \
               recipient's world answers, with the line the teller's world writes for each outcome.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-TELL-OUT"),
        station: "yalain-federation::cpu::tells::a_tell_to_another_world_goes_to_the_hub_and_ends_as_its_answer_says",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.tell-to-a-channel",
        says: "A tell to a channel goes to the hub as any tell, and a teller who is no member is \
               told so.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TELL-CHANNEL"),
        station: "yalain-federation::cpu::channels::a_tell_to_a_channel_goes_as_any_tell_and_a_non_member_is_told",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.tells-wait-for-the-link",
        says: "A tell waits a minute for a link and goes once the world is welcomed; one lost with the \
               link goes again first; one kept waiting a minute ends with the network unavailable, \
               and one unanswered for a hundred seconds with its world out of reach.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-TELL-QUEUE"),
        station: "yalain-federation::cpu::tells::tells_wait_for_the_link_for_a_minute_and_for_an_answer_for_a_hundred_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.three-worlds-of-two-games-on-one-hub",
        says: "Through one hub, a world of each game and two of this game's worlds carry friends, \
               presence and tells between their players both ways, within this game and across \
               games: refused with the joining line off the network, listed in the directory at \
               @dn on, offline when a player leaves, quits, says @dn off or its world's link \
               drops, and online again when it returns; and a line in the Global room reaches \
               the other worlds, of either game, in each world's own room.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-E2E"),
        station: "yalain-testkit::network::end_to_end::players_of_both_games_reach_each_other_across_three_worlds_on_one_hub",
        tier: Tier::Network,
    },
    behaviour! {
        id: "server.network.too-new-is-time-in-the-world",
        says: "A character's time in the world is kept with it, and it is too new for the Dereth \
               Network until that time reaches the seconds the operator asks; nothing asked, no \
               character is.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-TOO-NEW-TIME"),
        station: "yalain-testkit::cpu::operator::saves::time_in_the_world_is_kept_and_a_character_ages_past_the_networks_limit",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.too-new-sends-nothing",
        says: "A character younger than federation.min_character_age_seconds sends nothing to the \
               Dereth Network: its tell is refused with Empyrean's line, its room line and its channel \
               request likewise, and the hub's asks to speak for it are refused as too new. Old enough, \
               each goes.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-TOO-NEW"),
        station: "yalain::bin.yalain::friends::tests::the_worlds_least_age_holds_back_what_a_character_sends_to_the_network",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.verify-answer-lines",
        says: "Each of the hub's answers to a dn verify is one line in the network's words: verified \
               with each approval, the key unknown and where it is set, too many attempts, any \
               other refusal with the hub's own message (no control characters, at most 200 \
               characters, ended as a sentence), and a hub that could not be reached.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-VERIFY-LINES"),
        station: "yalain-federation::cpu::verify::each_of_the_hubs_answers_is_one_line_in_the_networks_words",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.verify-at-the-console",
        says: "The console's dn verify (its word in any case) with a token of the hub's shape asks \
               the hub to verify the world, as the console's, and says so; a token of another shape, or none, is \
               answered with the command's help line, which dn help lists; with the network off it \
               says the world is not on it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-VERIFY-CONSOLE"),
        station: "yalain-federation::cpu::verify::dn_verify_at_the_console_asks_the_hub_and_says_so",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.verify-challenge-answered-once",
        says: "While a dn verify waits for the hub, and for a minute at most from when it was typed, \
               the world posts back the nonce of each of the hub's challenges for its own slug \
               once, over HTTPS with its key; a challenge for another slug, before any \
               verification, past the minute or after the hub answered is ignored.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-VERIFY-CHALLENGE"),
        station: "yalain-federation::cpu::verify::a_challenge_for_this_world_is_answered_once_while_its_verification_waits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.verify-eight-nonces",
        says: "However many of the hub's challenges arrive, at most eight nonces are posted back for \
               one verification; a new verification answers afresh.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-VERIFY-EIGHT"),
        station: "yalain-federation::cpu::verify::at_most_eight_nonces_are_answered_for_one_verification",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.verify-game-port-keeps-challenges",
        says: "The hub's challenge on the login port is taken before the packet reader and kept for \
               the world only while a verification waits; on any other port, past what a pass \
               takes, or with none waiting it is dropped. It is never answered over UDP and makes \
               no connection.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-VERIFY-GAME-PORT"),
        station: "yalain-net::cpu::hub_challenge::a_hub_challenge_on_the_game_port_is_kept_only_while_the_world_hears_them_and_never_answered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.verify-in-game-for-admins",
        says: "The world's verification said in game needs an admin's account: an admin's line goes to the \
               network marked as an admin's; a player's is the friends service's, which reads it as no word \
               it knows, and is logged; nobody hears it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-VERIFY-SAID"),
        station: "yalain-testkit::cpu::operator::moderation::dn_verify_said_in_game_is_only_an_admins",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.verify-one-at-a-time",
        says: "One verification goes at a time: a second dn verify, at the console or in game, while \
               one waits for the hub is answered that one is on its way and sends nothing; once the \
               hub answered, the next goes.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-VERIFY-ONE"),
        station: "yalain-federation::cpu::verify::a_second_verification_while_one_waits_is_refused_and_sends_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.watch-answered",
        says: "Another world's watch is answered online only for a present character on the network \
               and a watcher not blocked, and offline alike for anything else.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-WATCH"),
        station: "yalain-federation::cpu::watches::a_watch_is_answered_online_only_for_a_present_character_on_the_network",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.watchers-capped",
        says: "A character keeps at most a hundred watchers from one world and a thousand in all, \
               always one it lists, and another world's watches are held to its share of the pace.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-WATCHERS-CAP"),
        station: "yalain-federation::cpu::watches::watchers_are_capped_by_world_and_in_all_and_watches_paced_by_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.watchers-told-each-change",
        says: "A kept watcher and every friend on another world is told each change of what it may \
               see of a character: arriving, leaving, going off the network, a block lifted.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-WATCHERS"),
        station: "yalain-federation::cpu::watches::a_kept_watcher_and_every_listed_friend_is_told_each_change",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.network.websocket-url-told-to-the-hub",
        says: "A world with a WebSocket endpoint names its URL in the hello of its link to the hub, within \
               the network's rule (wss, or ws on this machine), as Empyrean does; one outside the \
               rule is left out and logged.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WS-HUB"),
        station: "yalain-federation::cpu::operator::the_hello_names_the_worlds_websocket_endpoint_when_it_has_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.a-character-with-no-start-place-is-refused",
        says: "A character the world has no start place for, a row no client's creation can \
               make since every race it can make has places in each of its start areas, is \
               refused with the client's own reason and its client goes on at the list.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NO-START-PLACE"),
        station: "yalain-testkit::cpu::operator::store_faults::a_character_with_no_start_place_is_refused_and_its_client_can_go_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.a-flood-of-calls-is-dropped",
        says: "A client sending game calls past 2,000 at once and 200 a second has what is \
               over dropped unread, and the others go on as ever. No client of January 2017 \
               comes within a tenth of that.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CALL-LIMIT"),
        station: "yalain-testkit::cpu::play::lying_client::a_client_flooding_calls_has_what_is_over_dropped_and_others_go_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.a-panic-is-told-and-the-world-stops",
        says: "A panic in the world stops it: everyone in the world is told, and every \
               connection ends rather than timing out.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PANIC-STOPS"),
        station: "yalain-testkit::cpu::operator::store_faults::a_panic_tells_everyone_and_ends_their_connections",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.a-panic-keeps-what-whole-events-kept",
        says: "A panic in the world takes nothing that whole events kept: all a pass had \
               kept before it, and all that saves failing were holding, is written as the \
               server stops.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PANIC-WHOLE"),
        station: "yalain-testkit::cpu::operator::store_faults::a_panic_while_saves_fail_still_writes_what_was_held",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.a-panic-saves-nothing-half-done",
        says: "What the piece of work that panicked had kept is dropped, and no one's \
               memory is saved after it: each character goes back to its last save, whole.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PANIC-HALF"),
        station: "yalain-testkit::cpu::operator::store_faults::what_a_panicking_piece_kept_never_reaches_the_store",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.a-record-that-will-not-read-is-not-written-over",
        says: "The world's records (the consignments, the allegiances) are never started \
               empty by a read that failed: nothing is done with them until the store \
               answers; a record that does not read is copied aside for the operator before \
               a new one is kept.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RECORD-UNREADABLE"),
        station: "yalain-testkit::cpu::operator::store_faults::a_record_that_will_not_read_is_not_written_over",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.a-save-that-fails-is-held-and-tried-again",
        says: "A save that fails loses nothing: what it was to keep stays held and is tried \
               again every few seconds, a character that leaves or comes back meanwhile \
               included, and a server stopping tries for a while longer before it says whom \
               it lost.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SAVE-HELD"),
        station: "yalain-testkit::cpu::operator::store_faults::a_character_leaving_while_saves_fail_is_kept_once_they_work",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.a-save-that-fails-is-told-to-everyone",
        says: "Saves failing are told to the operator at once and every minute, shown in \
               the status endpoint, and told to each player in the world and to one \
               entering; their working again is told too.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SAVE-TOLD"),
        station: "yalain-testkit::cpu::operator::store_faults::saves_failing_are_told_to_everyone_and_so_is_their_working_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.a-state-that-will-not-read-refuses-entry",
        says: "A character whose kept state cannot be read (the database fails, the state \
               is damaged, a newer build kept it) is refused its entry with the client's \
               own words for it, and nothing is written over what it had.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STATE-UNREADABLE"),
        station: "yalain-testkit::cpu::operator::store_faults::a_damaged_state_refuses_the_entry_and_its_bytes_stay",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.a-stop-lets-every-client-go",
        says: "A stop tells every client it was logged off, and a panic that the server \
               crashed, which takes it from the world to its disconnected screen, and every \
               connection ends; a stop after a panic tells it nothing more.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STOP-LETS-GO"),
        station: "yalain-testkit::cpu::operator::shutdown::a_stop_tells_every_client_it_was_logged_off_and_ends_its_connection",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.access-levels-become-the-admin-flag",
        says: "A database kept with access levels takes the admin flag in their place: the highest level \
               with a secret is an admin; every level below, and the highest with no secret, a player named \
               in the log; and the levels are gone from the file, kept only in the backup made before the \
               upgrade.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-MIGRATE"),
        station: "yalain-store::cpu::persistence::a_file_with_access_levels_takes_the_admin_flag_in_their_place",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.accounts-at-the-console",
        says: "The operator makes accounts at the console with a bcrypt secret, a player's unless given the \
               word admin, gives one a new secret or the admin flag (reaching its login at once), and lists \
               them, with Empyrean's commands and answers; a secret never shows in a log.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-ACCOUNTS"),
        station: "yalain-testkit::cpu::operator::moderation::accounts_are_made_rekeyed_flagged_and_listed_at_the_console",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.admin-commands-said-in-game",
        says: "A character of an admin's account says the moderation commands in game after an at sign, \
               aloud or into a room: the command runs, is answered in its chat window and heard by nobody; \
               a player's line is said as any other, logged, and does nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-SAID"),
        station: "yalain-testkit::cpu::operator::moderation::admins_say_the_commands_in_game_and_a_players_line_is_only_said",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.ban-keeps-an-account-out",
        says: "A ban boots the account's login and keeps its logins out until it ends, as Empyrean's: the \
               game login is ended before the world's name with the client's line that the account \
               is banned, and the friends ticket refused as banned; a ban run out is lifted at the \
               next login, and an unban lifts one at once.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-BAN"),
        station: "yalain-testkit::cpu::operator::moderation::a_ban_boots_the_account_and_keeps_it_out_until_it_ends_or_is_lifted",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.boot-ends-a-login",
        says: "A boot, by character, account or id, sends the login the message that ends it with the \
               client's line that its character was booted, closes it a moment later, and its \
               character leaves the world as when its client goes away.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-BOOT"),
        station: "yalain-testkit::cpu::operator::moderation::a_boot_ends_the_login_and_its_character_leaves_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.builds-given-to-played-characters",
        says: "The operator gives a build to a character out of the world that has played, as Lyceum's \
               recipes apply to Empyrean's characters: its experience raised to the build's with \
               the credits, health and vigor of the levels it gains, each skill trained and raised \
               by a player's rules, as a player who earned them holds them; at least the build's \
               items in its pack, its money, quests, hero and place as the build says; nothing \
               lowered and the rest kept; one in the world is refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-APPLY"),
        station: "yalain-testkit::cpu::operator::characters::a_build_given_to_a_played_character_raises_it_as_play_would_and_keeps_the_rest",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.character-files-keep-items-own-numbers",
        says: "A character file's state holds every number an item holds of its own, so an imported \
               item has its value, what its level gave it, its effects, its maker, its wear, its \
               map's place, its color and its inscription. The file's build names items by template and count only, \
               and an import never reads it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-EXPORT-ITEM-NUMBERS"),
        station: "yalain-testkit::cpu::operator::characters::a_character_file_keeps_what_each_item_holds_of_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.characters-deleted-at-once",
        says: "The operator deletes a character at once, with no hour in its account's list: one in the \
               world is taken out of it and its login booted, one grayed out by a deletion goes too, \
               and its name makes a new character straight away; a player's own deletion keeps the \
               hour.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-DELETE-NOW"),
        station: "yalain-testkit::cpu::operator::characters::a_character_deleted_at_once_is_gone_and_its_name_free_straight_away",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.characters-deleted-by-the-operator",
        says: "The operator deletes a character as its player's deletion does, as Empyrean's \
               deletecharacter: one in the world is booted first, and it stays in its account's list, \
               grayed out, for the deleted list's hour, then goes.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-DELETE"),
        station: "yalain-testkit::cpu::operator::characters::a_deleted_character_is_booted_and_grayed_out_for_the_hour_then_gone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.characters-exported-and-imported",
        says: "The operator writes a character to a file and brings one from a file onto an account \
               as a new character of this world, as Empyrean's export and import-character: a new id \
               and new ids for its items, its name or a new one, what it held kept, and the character \
               and its state kept whole or not at all.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-EXPORT"),
        station: "yalain-testkit::cpu::operator::characters::a_character_exported_and_imported_holds_what_it_held_under_its_new_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.characters-made-with-a-build",
        says: "The operator makes a character with a build before it first plays, as Lyceum's recipes \
               make Empyrean's: its level's experience and credits, its skills trained and raised by \
               a player's rules, its money, items, quests, hero and place, held as a player who \
               earned them holds them; a build that does not hold, or a character that has played, is \
               refused and nothing kept.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-BUILD"),
        station: "yalain-testkit::cpu::operator::characters::a_character_made_with_a_build_holds_what_a_player_who_earned_it_holds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.characters-placed-out-of-the-world",
        says: "The operator sets where a character out of the world, played or not, enters next: beside \
               a character in the world, at a place the world names, or at a cell and point, as \
               teleport reads them; one in the world is refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-PLACE"),
        station: "yalain-testkit::cpu::operator::characters::a_character_placed_out_of_the_world_enters_there_next",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.characters-saved-every-five-minutes",
        says: "Every character in the world is kept again once its last save is five minutes old \
               (the operator may give another interval), on the world's clock, as Empyrean saves \
               each player every 300 seconds; a save at an event counts, so the next comes five \
               minutes after it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PERIODIC-SAVE"),
        station: "yalain-testkit::cpu::operator::saves::a_character_in_the_world_is_kept_every_five_minutes_on_the_world_clock",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.characters-taken-out-of-the-world",
        says: "The operator takes a character out of the world at once, as Empyrean's forcelogoff: \
               out with no lingering, even one a killed client left there, seen to leave, kept \
               where it stood and not deleted, its login ended; its account enters it again \
               straight away; a name not in the world is refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-FORCE-LOGOFF"),
        station: "yalain-testkit::cpu::operator::characters::a_character_a_killed_client_left_in_the_world_is_taken_out_at_once_and_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.config-file-sets-every-option",
        says: "A configuration file sets every option the command line takes, by its name, as \
               Empyrean reads its own file: the one named, else the working folder's, else the \
               one beside the program; relative paths are read from its folder, the command line \
               wins over it, and an unknown key is warned about and left out.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-CONFIG-FILE"),
        station: "yalain::live::configured::the_server_takes_its_settings_from_its_config_file_and_answers_its_status_endpoint",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.console-answers-in-a-fixed-form",
        says: "Each console command's answer, and each character's entering and leaving the world, is \
               one log line ending in a fixed tag a program reads ([ok <command>], [refused \
               <command>], [entered \"<name>\" <id>], [left ...], [listening]), the command named \
               as itself whichever word was typed, the words before the tag for people.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-ANSWERS"),
        station: "yalain-testkit::cpu::operator::answers::the_lines_programs_read_keep_their_fixed_form",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.exported-characters-carry-their-build",
        says: "A character written to a file carries its build in a recipe's own words, readable \
               without the server's encodings: its level (or experience past a level's), money, \
               hero and place, each skill it has trained with its rating, its items and its quests; \
               a made character's is the build it was made with, and a build read from a file makes \
               a character holding the same.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-EXPORT-BUILD"),
        station: "yalain-testkit::cpu::operator::characters::an_exported_character_carries_its_build_with_its_skills_ratings",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.gag-stops-chat-five-minutes",
        says: "A gagged character is told so and then says, tells, emotes and speaks into rooms to \
               nobody for five minutes of its time in the world, as Empyrean's gag; out of the \
               world it keeps the rest; when the gag ends or is lifted it is told, and is heard.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-GAG"),
        station: "yalain-testkit::cpu::operator::moderation::a_gagged_character_is_heard_by_nobody_for_five_minutes_in_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.gamecast-shows-everyone-a-message",
        says: "The operator's gamecast shows every player in the world Broadcast from who, then the \
               message, as Empyrean's: System from the console, the issuer's character said in game by an \
               admin; a player's is only said.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-GAMECAST"),
        station: "yalain-testkit::cpu::operator::moderation::gamecast_shows_every_player_in_the_world_the_message",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.global-room-setting",
        says: "federation.global_room takes general, trade, lfg, roleplay or empty, in any case: \
               left out it is General and on, empty carries no room, anything else stops the \
               server, and a world only listed reads none of it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-GLOBAL-SETTING"),
        station: "yalain::bin.yalain::config::tests::the_global_room_setting_takes_the_four_rooms_or_empty_and_refuses_anything_else",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.moderation-answers-done-or-refused",
        says: "Each line of a moderation command's answer at the console is tagged as done or \
               refused under the command's own word ([ok boot], [refused boot], ...), its words \
               Empyrean's: a boot of a login online is done, of one not online refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-MODERATION-ANSWERS"),
        station: "yalain-testkit::cpu::operator::moderation::a_moderation_commands_answer_says_whether_it_was_done",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.moderation-kept",
        says: "Each account's access level and ban and each character's gag are kept in the store as \
               they change, and a file kept before them gains them: every account a player's, none \
               banned, nobody gagged.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-STORE"),
        station: "yalain-store::cpu::persistence::the_operators_moderation_is_kept_and_an_earlier_file_gains_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.motd-at-entry",
        says: "The operator's message of the day is shown to each player as its character enters \
               the world, as Empyrean shows its own, a notice of the kind the 2017 servers used \
               for system text; with none, nothing is shown.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-MOTD"),
        station: "yalain-testkit::cpu::operator::sessions::the_message_of_the_day_is_shown_as_each_character_enters",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.names-with-spaces-at-the-console",
        says: "Every console command that names a character reads a name with spaces whole, as the \
               rest of the line or between double quotes, which also keep a word like off or to \
               inside a name.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-NAMES"),
        station: "yalain-testkit::cpu::operator::characters::a_name_with_spaces_reaches_the_character_it_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.passwd-changes-your-own-password",
        says: "Any account changes its own password in game with `@passwd <old> <new>`, as \
               Empyrean's: a wrong old password is refused, a second try within five seconds is \
               refused, fewer than two words is answered with the count and the use, and the right \
               one keeps the new password as a bcrypt secret; nobody hears the line.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-ADMIN-PASSWD"),
        station: "yalain-testkit::cpu::operator::moderation::a_player_changes_its_own_password_in_game_only_with_the_right_old_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.player-on-the-host-enters-a-world-advertised-elsewhere",
        says: "A world that refers players from elsewhere to an address nothing answers at still \
               takes a player on its own machine into the world: the client logged in over \
               loopback is sent to the world service over loopback, and enters.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-REFER-HOST-LIVE"),
        station: "yalain::live::referral::a_player_on_the_host_enters_a_world_that_refers_others_elsewhere",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.ports-taken-said-with-its-own-exit-code",
        says: "A server whose login, world or friends port another program holds stops before it \
               starts, its log's line naming the ports and ending in [ports-taken], and exits with \
               code 3, so a program starting it can tell this from other failures and try other \
               ports.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-PORTS-TAKEN"),
        station: "yalain::live::ports::a_server_whose_port_another_program_holds_says_so_and_exits_with_its_own_code",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.refused-login-closed-at-once",
        says: "A refused login is answered with the refusal and its connection closed in the same \
               moment, as the 2017 login server ended the logins it refused for a full world and \
               as Empyrean ends a refused login, rather than left open to time out.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-LOGIN-REFUSAL-CLOSED"),
        station: "yalain-testkit::cpu::operator::sessions::a_refused_login_is_closed_with_its_refusal_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-changed-camps-keep-or-replace-creatures",
        says: "Reloading the world database with a camp that keeps more creatures keeps the ones \
               it has and makes the rest at the world's next step (one keeping fewer retires the \
               extra); a camp moved, or making other creatures, retires its creatures and makes as \
               many anew where it now is.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-CAMPS-CHANGED"),
        station: "yalain-testkit::cpu::operator::reload_camps::a_camp_keeping_more_keeps_its_creatures_and_a_camp_moved_makes_them_anew",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-from-the-console",
        says: "Typing reload at a running server's console compiles its world database again: one \
               that does not compile leaves the world as it was and says so; one that does is \
               taken by the world its players are in, without a restart, a person it moved made \
               again where it now stands.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-CONSOLE"),
        station: "yalain::live::reload::reload_at_the_console_changes_the_running_world_or_nothing",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.reload-keeps-unchanged-veins",
        says: "A vein a reload leaves as it was keeps its state: its uses so far, or, spent, its \
               coming back where it stood; a spent one the reload takes away never comes back.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-VEINS-KEPT"),
        station: "yalain-testkit::cpu::operator::reload_veins::an_unchanged_vein_keeps_its_uses_and_a_spent_one_taken_away_never_comes_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-keeps-what-the-world-holds",
        says: "Reloading the world database keeps every template the world knew, so a thing \
               already in the world still has one; templates the database adds are taken at once.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-KEEPS"),
        station: "yalain-testkit::cpu::operator::reload::a_reload_keeps_every_template_the_world_may_hold",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-leaves-unchanged-objects-alone",
        says: "Reloading the world database leaves a placed object it did not change as it is, \
               under its id, whatever else the database added or took away: a person walking its \
               route walks on from where it is, its places told with stamps going on from those \
               before.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-UNCHANGED"),
        station: "yalain-testkit::cpu::operator::reload_placed::a_person_walking_its_route_walks_on_through_a_reload_that_left_it_alone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-lets-fights-finish",
        says: "A creature fighting a player when a reload takes its camp away or changes it fights \
               on, no longer its camp's; once at rest again it leaves the world, and nothing \
               replaces it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-FIGHTS"),
        station: "yalain-testkit::cpu::operator::reload_camps::a_creature_fighting_when_its_camp_goes_leaves_the_world_once_its_fight_is_over",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-lets-go-of-chests-in-use",
        says: "A chest a reload takes away or changes while a player has it open is closed for its \
               opener first: the opener is told it closed, what it held is forgotten and gone, and \
               a use waiting on it is refused as busy.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-CHESTS"),
        station: "yalain-testkit::cpu::operator::reload_placed::a_chest_open_when_a_reload_takes_it_away_is_closed_for_its_opener",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-lets-go-of-doors-in-use",
        says: "A door a reload moves, changes or takes away while it opens or closes for a \
               player's use answers the use, refused as busy, and is made again closed, nothing \
               more of its moving told.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-DOORS"),
        station: "yalain-testkit::cpu::operator::reload_placed::a_door_a_reload_moves_while_it_opens_answers_its_use_and_stands_closed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-lets-go-of-veins-in-use",
        says: "A vein a reload takes away while a player mines it answers the use at once, the \
               vein used up, and leaves the world; no material comes of the work.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-VEINS-IN-USE"),
        station: "yalain-testkit::cpu::operator::reload_veins::a_vein_taken_away_while_mined_answers_its_miner_used_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-makes-new-placed-objects",
        says: "Reloading the world database makes each object it newly places, under an id of its \
               own, for every player whose view holds it, and for nobody else.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-NEW-PLACED"),
        station: "yalain-testkit::cpu::operator::reload_placed::a_new_placed_object_is_made_for_the_players_who_see_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-never-touches-players-things",
        says: "Reloading the world database changes only its own placed objects and its camps' \
               creatures: the items players carry or left lying, corpses, and creatures no camp \
               keeps (pets and summoned ones) stay as they are.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-PLAYERS-THINGS"),
        station: "yalain-testkit::cpu::operator::reload_camps::a_reload_never_touches_what_a_player_has_or_left_lying",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-places-moved-objects-again",
        says: "A placed object a reload moves or changes keeps its id and is sent again with a \
               newer instance stamp, where it now stands, to every client that holds it and still \
               sees it, replacing the one it holds; a client that no longer sees it forgets it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-MOVED"),
        station: "yalain-testkit::cpu::operator::reload_placed::a_moved_or_changed_placed_object_keeps_its_id_and_is_made_again_where_it_stands",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-places-veins",
        says: "A vein a reload adds stands at once, made for the players who see it; one it takes \
               away (or moves: one taken away and one new) leaves the world, forgotten by every \
               client that holds it; the others are left as they stand.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-VEINS"),
        station: "yalain-testkit::cpu::operator::reload_veins::a_vein_a_reload_adds_stands_at_once_and_one_taken_away_leaves_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-retires-removed-camps",
        says: "A camp a reload takes away takes its creatures at rest out of the world at once, \
               forgotten by every client that holds them, and none are made again.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-CAMPS-REMOVED"),
        station: "yalain-testkit::cpu::operator::reload_camps::a_camp_taken_away_takes_its_creatures_at_rest_out_of_the_world_and_none_come_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-starts-new-camps",
        says: "A camp a reload adds makes its creatures at the world's next step, made for the \
               players who see them.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-CAMPS-NEW"),
        station: "yalain-testkit::cpu::operator::reload_camps::a_new_camp_makes_its_creatures_at_the_worlds_next_step",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-takes-away-removed-objects",
        says: "A placed object a reload takes away is forgotten by every client that holds it; the \
               objects left keep their ids, whatever their places in the list now.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-REMOVED"),
        station: "yalain-testkit::cpu::operator::reload_placed::a_placed_object_taken_away_is_forgotten_by_every_client_that_holds_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-takes-what-changes-live",
        says: "Reloading the world database changes what people say at once for players already in \
               the world, and the report says which tables were taken; a reload of the same \
               database changes nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD"),
        station: "yalain-testkit::cpu::operator::reload::a_reload_changes_what_people_say_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-tells-new-conflict-areas",
        says: "A reload that changes which landblocks allow player conflict tells each player whose \
               landblock's status changed the new status at once, as walking into it would; one \
               that changes none tells nobody.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-CONFLICT"),
        station: "yalain-testkit::cpu::operator::reload::a_reload_that_changes_the_conflict_where_a_player_stands_tells_it_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reported-to-the-hub",
        says: "A server told to report to a Dereth Network hub posts a heartbeat as an Asheron's \
               Call 2 world at once, under its slug and with its key, saying it is up and how many \
               characters are in the world, and at once again when a shutdown starts, so the \
               launcher's Auberean realm lists it with its population.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-HUB-REPORT"),
        station: "yalain::live::hub::the_server_reports_itself_to_a_hub_as_an_auberean_world_with_the_characters_in_it",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.saves-spread-one-a-tick",
        says: "Characters whose periodic saves come due together are kept one a tick of the \
               world, the longest unsaved first, so no tick stalls on the database; save-world \
               keeps everyone in the world at once.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PERIODIC-SAVE-SPREAD"),
        station: "yalain-testkit::cpu::operator::saves::characters_due_together_are_kept_one_a_tick_at_the_operators_interval",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.sessions-cap-lets-admins-in",
        says: "A world holding as many logins as it may still lets in an admin's account with its right \
               ticket, as the game's own help for its user limit says. A wrong ticket for one is told \
               the world is full, as anyone is, and one address's limit holds it back as anyone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SESSION-CAP-ADMINS"),
        station: "yalain-testkit::cpu::operator::sessions::an_admins_account_with_its_ticket_gets_into_a_full_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.sessions-capped-server-full",
        says: "A world holds at most the operator's number of logins (Empyrean's 128 by default), \
               and one address at most its own number (no limit by default, an exempt address \
               none); a login past either is refused unchecked with the client's server-full \
               reason, as the 2017 login server refused one, and its connection closed.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SESSION-CAPS"),
        station: "yalain-testkit::cpu::operator::sessions::a_login_past_the_logins_the_world_holds_is_told_the_server_is_full",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.shutdown-cancelled",
        says: "A shutdown's countdown cancelled tells every player in the world the world is no \
               longer shutting down; no more lines come, the world goes on, and a later shutdown \
               counts down afresh.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SHUTDOWN-CANCEL"),
        station: "yalain-testkit::cpu::operator::shutdown::a_cancelled_shutdown_tells_every_player_and_the_world_goes_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.shutdown-counts-down-two-minutes",
        says: "Shutting the world down tells every player in it, two minutes before and then every \
               thirty seconds, how long is left, ends with the world shutting down now, and stops \
               the server twenty seconds after.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SHUTDOWN-COUNTDOWN"),
        station: "yalain-testkit::cpu::operator::shutdown::a_shutdown_counts_down_two_minutes_to_every_player_then_stops",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.shutdown-message-and-interval",
        says: "A shutdown shows the operator's message first, then counts down the operator's \
               interval: its first line at once, a line at each of Empyrean's marks still ahead \
               (an hour, 30, 15, 10 and 5 minutes and so on), and the 2017 lines from two \
               minutes on.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SHUTDOWN-INTERVAL"),
        station: "yalain-testkit::cpu::operator::shutdown::a_shutdown_shows_the_operators_message_and_counts_down_the_operators_interval",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.start-reads-each-landblock-once",
        says: "Blocks built one after another along the world's columns read each landblock once \
               for themselves and the blocks around them, and come out as blocks built alone; \
               only the columns beside the one being built stay in memory.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-START-LANDBLOCK-ONCE"),
        station: "auberean-physics::dat::objects::blocks_built_from_the_landblocks_kept_near_them_are_as_those_built_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.operator.start-reads-the-ground-once",
        says: "Reading the ground at start reads each landblock and each mesh it needs once, the \
               head of a mesh aside, and reads no material just to know it is in the data.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-START-GROUND-ONCE"),
        station: "yalain::bin.yalain::data::tests::the_ground_reads_each_landblock_and_mesh_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.start-reads-the-saved-states-once",
        says: "The tables a start reads from every saved state read them from the file once between \
               them; the second table to ask reads almost nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-START-STATES-ONCE"),
        station: "yalain::bin.yalain::data::tests::the_mounts_and_the_item_uses_read_the_saved_states_once_between_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.start-shares-the-open-files",
        says: "A start opens each of the install's files once: the files it serves clients are \
               the ones it read, and a record a table has read is not read again when another \
               asks.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-START-FILES-ONCE"),
        station: "yalain::bin.yalain::data::tests::the_files_served_are_the_files_the_start_read_and_a_record_is_read_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.status-endpoint",
        says: "With a status address, the server answers GET /health with ok and GET /status with \
               the world's state as JSON (its name, the server's version and game, uptime, open \
               or shutting down, the players in the world, logins and connections), as \
               Empyrean's status endpoint does.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STATUS-ENDPOINT"),
        station: "yalain::live::configured::the_server_takes_its_settings_from_its_config_file_and_answers_its_status_endpoint",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.status-ping-on-the-login-port",
        says: "The server answers the status ping on its login port as Empyrean does on its game \
               port: a hello with a token alone, smaller than the hello; an ask with that token, \
               from the same address and port within two windows, with the world's state, players, \
               software, version and name (no era); nothing for anything else, or past four hellos \
               and four asks a minute from one address or the replies a second.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-STATUS-PING"),
        station: "yalain-net::cpu::status_ping::a_hello_draws_a_token_and_an_ask_with_it_draws_the_worlds_status",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.stop-and-exit-at-the-console",
        says: "Typing stop or exit at the console stops the server at once as stop-now does, every \
               character in the world kept before it exits.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-STOP"),
        station: "yalain::live::stopping::stop_at_the_console_keeps_the_character_where_it_walked_and_exits",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.stop-now",
        says: "Stopping now shows every player in the world the operator's message and the \
               countdown's last line, and the server stops at once, a countdown running or not.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SHUTDOWN-STOP-NOW"),
        station: "yalain-testkit::cpu::operator::shutdown::stopping_now_shows_the_last_line_and_lets_the_server_stop_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.stop-when-the-console-ends",
        says: "A server told to stop when its console's input ends stops at once, every character in \
               the world kept, so a program that ran it and went away leaves nothing running; one \
               not told (a service, whose input ends at start) ends only its console and plays on, \
               as Empyrean's console ends at the end of its input and its world runs on.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-CONSOLE-END"),
        station: "yalain::live::stopping::the_console_ending_stops_a_server_told_to_stop_then_keeping_the_character",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.stopping-keeps-everyone-in-the-world",
        says: "However the server stops (a shutdown's end, its time run out, or the system's stop \
               signal), every character still in the world leaves it as when its client goes \
               away, and what it did is kept.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STOP-KEEPS"),
        station: "yalain-testkit::cpu::operator::shutdown::stopping_the_server_keeps_every_character_still_in_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.teleport-at-the-console",
        says: "The operator sends a character in the world through portal space, as a recall goes: \
               beside another character in the world, facing it; to a place the world names; or to a \
               cell and a point. One not in the world, or a destination that is none, is refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-OPERATOR-TELEPORT"),
        station: "yalain-testkit::cpu::operator::characters::a_teleported_character_goes_through_portal_space_to_where_it_was_sent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.the-list-of-what-is-not-handled-is-bounded",
        says: "The list of what clients asked for and the server does not do keeps at most \
               1,024 kinds; those past it are counted together, so a client naming endless \
               calls grows neither the server's memory nor its log.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UNHANDLED-BOUND"),
        station: "yalain-testkit::cpu::play::lying_client::a_client_naming_endless_unknown_calls_does_not_grow_the_servers_list_without_end",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.version-names-its-source",
        says: "The server's version text names the program and its version, its commit, its target and \
               the repository its releases come from, as Empyrean's does.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-VERSION-SOURCE"),
        station: "yalain::bin.yalain::version_text_tests::version_names_the_program_its_version_its_commit_its_target_and_its_source",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.websocket-allowed-origins",
        says: "Only a browser page from an origin in allowed_origins connects to the WebSocket endpoint; \
               none listed refuses every page, and a client naming no origin is no page and connects, \
               as on Empyrean's.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WS-ORIGINS"),
        station: "yalain-net::cpu::websocket::only_a_page_from_a_listed_origin_is_let_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.websocket-idle-timeout",
        says: "A WebSocket connection that sends nothing for idle_timeout seconds is closed, and its will \
               is delivered as if the client had sent it, each datagram to the service its port names.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WS-IDLE"),
        station: "yalain-net::cpu::websocket::an_idle_connection_is_closed_and_its_will_delivered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.websocket-limit-per-address",
        says: "One client address holds maximum_connections_per_ip_address WebSocket connections at once: \
               the next is closed, another address connects, and a closed connection gives its place \
               back.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WS-LIMIT"),
        station: "yalain-net::cpu::websocket::a_connection_past_the_limit_for_its_address_is_closed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.websocket-proxied-client-address",
        says: "The sessions take a client behind a trusted proxy at the last address the proxy's \
               X-Forwarded-For names, so its world login comes from its login's address; a proxy not \
               in trusted_proxies is itself the client.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WS-PROXY"),
        station: "yalain-net::cpu::websocket::only_a_trusted_proxy_names_the_client_to_the_sessions",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.websocket-reaches-both-services",
        says: "One WebSocket connection numbers the login's two ports and the world's two in a row from \
               its first datagram's port; a client on it is referred to the world by its own numbers, \
               logs in and plays over that one connection, and every other client keeps the service's \
               real port.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WS-SERVICES"),
        station: "yalain::live::referral::a_client_on_one_websocket_logs_in_and_reaches_the_world",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.websocket-settings-read-as-empyreans",
        says: "The server.websocket section takes Empyrean's keys, its defaults and its meanings: the \
               settings are the type both servers share.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WS-SETTINGS"),
        station: "yalain::live::network::a_world_tells_its_hub_and_its_status_endpoint_its_public_websocket_address",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.websocket-settings-refused-as-empyreans",
        says: "With the endpoint on, the server stops at its start for each setting Empyrean's refuses: a \
               plain listener off loopback without behind_tls_proxy, half a certificate, a listen or a \
               trusted proxy that is no address, a certificate that will not read, and a port another \
               program holds.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WS-REFUSALS"),
        station: "yalain::live::configured::a_websocket_setting_that_cannot_be_served_stops_the_server_at_its_start",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.websocket-url-on-the-status-endpoint",
        says: "The status endpoint reports the WebSocket endpoint's URL, null with it off, and a page from \
               an origin the endpoint admits may read it, as on Empyrean's.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WS-STATUS"),
        station: "yalain::live::network::a_world_tells_its_hub_and_its_status_endpoint_its_public_websocket_address",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.pets.aggressive-unprovoked",
        says: "An aggressive pet takes on the nearest creature within its reach without being told \
               to.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-AGGRESSIVE"),
        station: "yalain-testkit::cpu::play::pet_kinds::an_aggressive_pet_takes_on_the_nearest_creature_unbidden",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.attack-order",
        says: "An attack order sends every pet after the target, with no answer; a target more than \
               32 m from the owner is refused with its text and no pet goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-ATTACK"),
        station: "yalain-testkit::cpu::play::pets::an_attack_order_sends_the_pet_unless_the_target_is_too_far_from_its_owner",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.called-back",
        says: "A pet called back drops its fight and is at peace until it is with its owner again; \
               the order has no answer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-RETURN"),
        station: "yalain-testkit::cpu::play::pets::a_pet_called_back_drops_its_fight",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.defends-itself-by-mode",
        says: "A pet that supports its owner does not fight back what strikes it; in a mode that \
               defends itself it does; struck down, it is taken off its owner's list.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-MODES"),
        station: "yalain-testkit::cpu::play::pets::a_pet_fights_back_only_in_a_mode_that_defends_itself",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.die-with-their-owner",
        says: "An owner's death takes its pets in its death tick, each taken off its list after the \
               owner's death is told.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-DEATH"),
        station: "yalain-testkit::cpu::play::pets::an_owners_death_takes_its_pets",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.dies-of-its-kingdom",
        says: "A dying pet's physics type is its own, its owner's kingdom's bit and all, with the \
               dead bit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-DEAD-PHYSICS"),
        station: "yalain-testkit::cpu::play::pets::a_dead_lasher_keeps_its_kingdom_in_its_dead_physics_type",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.ethereality-where-it-stands",
        says: "A pet come into a landblock of another conflict status takes its owner's movement \
               ethereality for it, with its own bit, told to every client that knows it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-ETHEREALITY-WALK"),
        station: "yalain-testkit::cpu::play::pets::a_pet_put_beside_its_owner_in_a_controlled_landblock_passes_through_its_kingdoms_there",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.follow",
        says: "A pet not fighting plans its way to its owner every three seconds, stopping three \
               meters short with its speed told, and one more than 100 m away is put beside its \
               owner.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-FOLLOW"),
        station: "yalain-testkit::cpu::play::pets::a_pet_follows_its_owner_and_is_put_beside_it_when_far",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.hits-by-level",
        says: "A pet's hit grows with the square of its level, varies either side, loses a point \
               for each point of armor, and is never critical.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-DAMAGE"),
        station: "yalain-testkit::cpu::play::pet_kinds::a_pets_hit_is_its_kinds_number_times_its_level_squared_less_the_creatures_armor",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.leave-with-their-owner",
        says: "An owner leaving the world takes its pets out of it, and others who saw a pet see it \
               go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-LOGOFF"),
        station: "yalain-testkit::cpu::play::pets::an_owner_leaving_the_world_takes_its_pets_out_of_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.linger-with-their-owner",
        says: "An owner who logs off keeps its pets in the world until it leaves; then they are \
               forgotten as it goes, the owner told nothing of them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-LINGER"),
        station: "yalain-testkit::cpu::play::pets::an_owner_logging_off_keeps_its_pet_until_it_leaves",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.missile-lands-later",
        says: "A pet's missile is thrown in one story and lands after its flight in a story told \
               by what it lands on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-MISSILE"),
        station: "yalain-testkit::cpu::play::pet_kinds::a_pets_missile_lands_after_its_flight_as_a_story_of_what_it_lands_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.mode-echoed",
        says: "A pet's mode is echoed back to its owner, pet and mode, and kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-MODE-ECHO"),
        station: "yalain-testkit::cpu::play::pets::a_pets_mode_is_echoed_back_and_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.of-the-owners-kingdom",
        says: "A summoned pet is of its owner's kingdom (none: the neutral one): its creation \
               carries the kingdom, its physics type the kingdom's bit, and its movement \
               ethereality its owner's for where it stands with its own bit, the owner's \
               protection aside.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-KINGDOM"),
        station: "yalain-testkit::cpu::play::pets::a_lasher_appears_of_its_owners_kingdom_and_passes_through_what_its_owner_would",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.orbs",
        says: "An Axe Orb's swing hits four times; a Taunt Orb's strike does no harm and may turn \
               the creature on it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-ORBS"),
        station: "yalain-testkit::cpu::play::pet_kinds::an_axe_orb_hits_four_times_a_swing_and_a_taunt_orbs_strike_harms_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.ordered-to-die",
        says: "A pet ordered to die (or all of them) is taken off its owner's list and falls as a \
               creature does, in one tick; its corpse comes after and its body leaves the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-DIE"),
        station: "yalain-testkit::cpu::play::pets::a_pet_ordered_to_die_falls_and_is_taken_off_its_owners_list",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.read-from-the-skills",
        says: "A summoning skill leaves its user its pet effect, read with the pet's class and \
               lifetime; the pet shows the skill's icon.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-DEFS"),
        station: "yalain-testkit::dat::effects::the_summoning_skills_pets_read_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.pets.renamed",
        says: "A pet renamed is named in the written form as its name and its kind, told to its owner \
               and shown to everyone; the same name again shows nothing new.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-NAME"),
        station: "yalain-testkit::cpu::play::pets::a_pet_renamed_is_named_in_the_written_form_and_the_same_name_shows_nothing_new",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.rift",
        says: "A Volcanic Rift stays where it is made, its fire lands on every creature near where \
               it falls, and it vanishes at its lifetime's end.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-RIFT"),
        station: "yalain-testkit::cpu::play::pet_kinds::a_volcanic_rift_stays_its_fire_lands_on_all_near_and_it_goes_at_its_lifetimes_end",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.speed-told-entering-a-fight-mid-move",
        says: "A pet that goes into a fight while still on its way to its owner is told its kind's \
               own speed right after its fighting mode.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PET-SPEED-FIGHT"),
        station: "yalain-testkit::cpu::play::pets::a_pet_going_into_a_fight_on_its_way_is_told_its_own_speed_after_its_mode",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.speed-told-setting-off-and-arriving",
        says: "A pet setting off from standing toward its owner is told the faster of its kind's \
               and its owner's speed, and on arrival its kind's own, told even when unchanged; \
               its plans while it runs tell nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PET-SPEED"),
        station: "yalain-testkit::cpu::play::pets::a_pets_speed_is_told_as_it_sets_off_and_as_it_arrives_and_not_between",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.strike-flag-by-skill",
        says: "A pet's strike carries the basic-attack flag by its skill.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PET-STRIKE-FLAG"),
        station: "yalain-testkit::cpu::play::pets::a_pets_strike_carries_the_basic_flag_by_its_skill",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.strikes-land-late",
        says: "A creature's strike at a pet and a pet's at a creature take health when the blow \
               reaches the target after its story, as its skill shows over the time scale; one \
               dead by then is not hurt.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-LANDING"),
        station: "yalain-testkit::cpu::play::pets::a_creatures_strike_hurts_a_pet_when_its_blow_lands",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.summoned",
        says: "A Ranger's Nature's Aid ends by telling it of its Lasher (name, the skill's icon, \
               class and flags) before the skill's information, and the Lasher appears in front of \
               it, naming it as summoner.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-SUMMON"),
        station: "yalain-testkit::cpu::play::pets::natures_aid_tells_the_ranger_of_its_lasher_which_then_appears_beside_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.summoning-fails-indoors",
        says: "A turret summoned indoors fails: the summoner is told of it, then that the \
               summoning fails, and it never appears.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-FAILS"),
        station: "yalain-testkit::cpu::play::pet_kinds::a_turret_summoned_indoors_fails_and_never_appears",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.supports-its-owner",
        says: "A supporting pet takes on what its owner hurts, from its owner's first hit, and \
               strikes it; the creature it strikes fights the pet.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-ASSIST"),
        station: "yalain-testkit::cpu::play::pets::a_pet_takes_on_what_its_owner_attacks_and_the_creature_fights_the_pet",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.through-portals",
        says: "A pet goes with its owner through a portal and is beside it at the destination.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-PORTAL"),
        station: "yalain-testkit::cpu::play::pets::a_pet_goes_with_its_owner_through_a_portal",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.too-many-of-a-class",
        says: "A summoning skill used while its user has as many live pets of its class as it allows \
               is refused as too many pets; no pet is told of or appears.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-CAP"),
        station: "yalain-testkit::cpu::play::pets::a_second_lasher_is_refused_as_too_many_pets",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.pets.too-many-orders",
        says: "Pet orders run up a debt of a second each that drains a second a second: from none, \
               six at once are carried out and the seventh is refused with the seconds to wait.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PETS-THROTTLE"),
        station: "yalain-testkit::cpu::play::pets::the_seventh_order_in_a_moment_is_refused_with_the_wait",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.a-door-plays-its-scene",
        says: "A door played as a scene takes the player through it: teleporting, frozen, the scene \
               begun with no flash or glows; the scene ended, the use answered and the player at the \
               destination; drawn, it is freed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DOOR-SCENE"),
        station: "yalain-testkit::cpu::entry::places::a_door_plays_its_scene_in_place_of_portal_space_and_takes_the_player_through",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.a-place-off-the-land-is-no-place",
        says: "A place reported further from its block than the whole land's side is not \
               taken and the player stays where it was; a point that far from a block comes \
               to a place at the land's edge, and points on the land are placed as stepping \
               them block by block does.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PLACE-OFF-LAND"),
        station: "yalain-testkit::cpu::play::drops::a_place_reported_far_off_the_land_is_not_taken",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.actions-locked",
        says: "A use asked for while the player's earlier action is still carried out (in portal space, dead, walking to an earlier use's object, taking an item) is refused at once: its answer alone, with the status that actions are locked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-USES-LOCKED"),
        station: "yalain-testkit::cpu::entry::places::a_use_in_portal_space_is_refused_as_actions_locked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.allegiance-recall-unbound",
        says: "Allegiance Recall with no hall its allegiance is bound to plays it all and says the \
               allegiance is not bound; the player stays where it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ALLEGIANCE-RECALL"),
        station: "yalain-testkit::cpu::play::recalls::allegiance_recall_with_no_hall_bound_fails_and_says_so",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.arramora-recall",
        says: "Arramora's recall takes the player to its place as the cities' recalls do, protected the \
               same, with no word of where it came out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ARRAMORA-RECALL"),
        station: "yalain-testkit::cpu::play::recalls::arramoras_recall_takes_the_player_there_without_a_word",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.bind-plays-its-behavior",
        says: "Binding to a lifestone plays the player's binding behavior for about a second \
               before it is stopped, so it is seen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BIND-BEHAVIOR"),
        station: "yalain-testkit::cpu::entry::places::binding_plays_the_players_behavior_for_about_a_second",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.cannot-be-used",
        says: "A placed object of a kind that cannot be used (a forge, a road sign) is refused at \
               once wherever the player stands, with no walk: status 10001, flags 828, the \
               distance from where the player asked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CANNOT-BE-USED"),
        station: "yalain-testkit::cpu::entry::places::a_forge_or_a_road_sign_is_refused_at_once_wherever_the_player_stands",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.cannot-be-used-read",
        says: "The placed objects that cannot be used are those of the script library's forge and \
               road sign kinds; persons, portals and lifestones are not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CANNOT-BE-USED-KINDS"),
        station: "yalain-testkit::dat::interactions::forges_and_road_signs_are_told_by_their_script_kinds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.places.chest-closes-when-its-opener-walks-away",
        says: "An open chest whose opener goes more than six meters from it plays its closing, its \
               opener is told it is closed and everyone sees it shut.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHEST-WALK-AWAY"),
        station: "yalain-testkit::cpu::play::chests::an_open_chest_closes_when_its_opener_walks_away",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.chest-filled-at-each-opening",
        says: "What a chest holds is made anew at each opening from its generator profile, what was left in \
               it cleared for all who see it, by the chest itself (the item, then the chest) between its \
               flash and its opening; an item is taken out of it as from a corpse.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHEST-FILLED"),
        station: "yalain-testkit::cpu::play::chests::a_chest_is_filled_anew_at_each_opening_and_an_item_is_taken_out_into_the_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.chest-kept-by-a-quest",
        says: "A chest a quest keeps refuses at once a player whose quest is not at the phase it \
               names, and opens for one whose quest is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHEST-QUEST"),
        station: "yalain-testkit::cpu::play::chests::a_chest_a_quest_keeps_refuses_a_player_not_at_its_phase",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.chest-opens-to-its-key",
        says: "A chest's own key turns and the chest flashes and opens for all; opened, the opener alone is \
               shown what it holds, the key is used up and the use answered; closed, it locks again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHEST-KEY"),
        station: "yalain-testkit::cpu::play::chests::a_chests_key_opens_it_its_contents_shown_to_the_opener_alone_and_it_locks_as_it_closes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.chest-taken-all",
        says: "Taking everything from an open chest brings all it holds into the opener's pack.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHEST-TAKE-ALL"),
        station: "yalain-testkit::cpu::play::chests::everything_in_an_open_chest_is_taken_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.chest-unlocked-then-opened",
        says: "A chest whose key only unlocks it: the key is used up and the use answered, and a plain use \
               then opens it, unanswered as a use.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHEST-UNLOCK"),
        station: "yalain-testkit::cpu::play::chests::a_chest_its_key_only_unlocks_opens_to_a_plain_use_after",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.city-recall",
        says: "A city's recall powers up as the lifestone's does and takes the player to the city's place, \
               its text before the new place; as it ends the player is protected for 30 s, its glow \
               played, and then told the portal's magic no longer protects it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CITY-RECALL"),
        station: "yalain-testkit::cpu::play::recalls::a_city_recall_takes_the_player_to_its_city_and_protects_it_thirty_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.city-recall-nowhere",
        says: "A city's recall whose place the world lacks plays it all and ends without a word, the \
               player where it was and not protected.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CITY-RECALL-NOWHERE"),
        station: "yalain-testkit::cpu::play::recalls::a_city_recall_with_no_place_ends_without_a_word_or_protection",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.creature-opens-a-door-on-its-way",
        says: "A creature whose way leads through a closed door stops a meter and a half short of it, opens \
               it as a use does, unanswered, stands until it is open and goes through; the door closes ten \
               seconds after.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DOOR-CREATURE-OPENS"),
        station: "yalain-testkit::cpu::play::doors::a_creature_on_its_way_opens_a_closed_door_and_goes_through_once_it_is_open",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.creature-that-does-not-use-doors-is-held",
        says: "A creature whose template says it does not use doors stands at a closed door on its way and \
               never opens it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DOOR-SUMMONS-HELD"),
        station: "yalain-testkit::cpu::play::doors::a_creature_that_does_not_use_doors_is_held_at_a_closed_door",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.door-closing-cut-short",
        says: "A use of a door while it closes shuts it at once, its closed mode and shut shown to all, and \
               is answered when the closing would have ended.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DOOR-CUT-SHORT"),
        station: "yalain-testkit::cpu::play::doors::a_use_that_cuts_a_closing_short_is_answered_when_the_closing_would_have_ended",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.door-opens-and-closes-by-itself",
        says: "A door used turns passable and opens for all who see it; opened, the use is answered; ten \
               seconds on it turns solid, closes, and is shut when its closing has played.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DOOR-OPENS"),
        station: "yalain-testkit::cpu::play::doors::a_door_opens_for_everyone_stays_open_ten_seconds_and_closes_by_itself",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.door-refuses-while-moving",
        says: "A use of a door while it opens is refused as busy; a use of an open door closes it, and for \
               three seconds after a use is refused as too soon.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DOOR-REFUSALS"),
        station: "yalain-testkit::cpu::play::doors::a_door_refuses_a_use_while_it_opens_and_for_three_seconds_after_a_use_closed_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.door-walked-to",
        says: "A door out of reach is walked to within two meters before it moves.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DOOR-WALK"),
        station: "yalain-testkit::cpu::play::doors::a_door_out_of_reach_is_walked_to_within_two_meters_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.every-lifestone-binds",
        says: "Every kind of lifestone binds the player who uses it, the Lugian's, the Tumerok's \
               and the plain one as the human's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LIFESTONES-ALL"),
        station: "yalain-testkit::cpu::entry::places::every_kind_of_lifestone_binds_the_player",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.faction-shrine",
        says: "A faction shrine walks its user to within 3 m, the shrine and the player play their \
               behaviors, the player is told its kingdom rating was recorded, and the use is never \
               answered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FACTION-SHRINE"),
        station: "yalain-testkit::cpu::entry::places::a_faction_shrine_records_the_rating_and_never_answers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.faction-shrine-mounted",
        says: "A faction shrine used on a mount tells the player its use is canceled, after the \
               rating recorded.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FACTION-SHRINE-MOUNTED"),
        station: "yalain-testkit::cpu::entry::places::a_faction_shrine_used_on_a_mount_is_canceled_after_the_rating",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.glyph-opens-its-vault",
        says: "A glyph used on its vault plays its use; the vault flashes and the player goes through \
               portal space to the vault's destination, where the glyph is used up and the use \
               answered that it fits the lock.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GLYPH-VAULT"),
        station: "yalain-testkit::cpu::play::vaults::a_glyph_used_on_its_vault_takes_the_player_through_and_is_used_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.hoard-key-opens-the-hoard-to-the-fellowship",
        says: "A hoard's key used on its stockpile, its quest able to be given, gives the hoard's entry \
               quest to its user and to each of the user's fellows (each fellow told the update alone), \
               is used up, and is answered at once with its table's text.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HOARD-KEY"),
        station: "yalain-testkit::cpu::play::interactions::a_hoard_key_on_its_stockpile_gives_the_entry_quest_to_its_user_and_fellows_and_is_used_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.item-used-on-an-object-at-once",
        says: "An item used on an object its interaction table names, with no behavior to play, is \
               carried out at once with no walk: its steps run, the item is used up when the outcome \
               says so, and the use is answered with the outcome's text or that the item was used on \
               the object, flags 319 when the item went and 316 when it stayed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-INTERACTION-USE"),
        station: "yalain-testkit::cpu::play::interactions::an_item_used_on_an_object_its_table_names_is_answered_at_once_and_kept_when_its_outcome_keeps_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.item-used-on-an-object-moving-nothing-is-answered",
        says: "An item used on an object that no interaction table, door, chest, vault or step of \
               the player's quests as they stand moves is answered at once as done, the flags of an \
               item kept, and the item is kept and free to use again; a step with nowhere to take \
               the player is answered the same way once it has run, the flags of an item used up \
               when it took the item.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-USE-ON-NOTHING"),
        station: "yalain-testkit::cpu::play::interactions::an_item_used_on_an_object_while_its_step_waits_for_another_phase_is_answered_and_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.item-used-on-an-object-plays-a-behavior",
        says: "An item used on an object its interaction table names (by template, or by a class \
               the object's package is or derives from) whose own use behavior, else the object's, \
               is set plays it at once with no walk; once it has played out (a dye plant's 1.88 s, \
               the Requiescat's 2.42 s) the steps run, an item kept wears by one, one used up goes, \
               and the use is answered as one at once is; another use meanwhile is refused 1029.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-INTERACTION-BEHAVIOR"),
        station: "yalain-testkit::cpu::play::interactions::a_watering_can_on_a_dye_plant_plays_the_plants_behavior_and_is_answered_when_it_has_played",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.item-used-on-an-object-refused",
        says: "An item used on an object its interaction table names is refused at once and kept when \
               the player stands farther off than the table allows (10056) or the item's quest is not \
               where the item needs it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-INTERACTION-REFUSED"),
        station: "yalain-testkit::cpu::play::interactions::a_key_too_far_from_its_stockpile_or_whose_quest_is_under_way_is_refused_and_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.lifestone-bind-and-recall",
        says: "Using a lifestone binds the player where it stands; the lifestone recall (a weapon in hand) then powers up, tells its story, and takes the player through portal space to the place and heading it had as it bound, with a newer teleport stamp; its end gives the skill's information and starts the wait every skill shares.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LIFESTONE"),
        station: "yalain-testkit::cpu::entry::places::a_lifestone_binds_and_the_recall_takes_the_player_back_where_it_bound",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.locked-chest-refuses",
        says: "A locked chest refuses a plain use: its opening answered locked, the chest flashing, the use \
               not answered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHEST-LOCKED"),
        station: "yalain-testkit::cpu::play::chests::a_locked_chest_refuses_a_plain_use_and_flashes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.locked-door-holds-a-creature-at-its-front",
        says: "A locked door holds a creature at its front as it holds a player's plain use; one whose kind \
               lets creatures through its lock opens to it, unless its template says otherwise.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DOOR-CREATURE-LOCK"),
        station: "yalain-testkit::cpu::play::doors::a_locked_door_holds_a_creature_at_its_front_unless_its_kind_lets_creatures_through",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.locked-door-opens-from-behind",
        says: "A locked door opens to a plain use from behind it: its lock holds from its front only.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DOOR-FRONT"),
        station: "yalain-testkit::cpu::play::doors::a_locked_door_opens_to_a_plain_use_from_behind",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.locked-door-opens-to-its-key",
        says: "A locked door refuses a plain use from its front and a key not its own as the wrong key; \
               its key turns, the door unlocks and opens, the key is used up and the use answered that \
               it fits; the door locks again as it closes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DOOR-KEY"),
        station: "yalain-testkit::cpu::play::doors::a_locked_door_refuses_a_plain_use_and_opens_to_its_key_which_is_used_up",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.monarch-only",
        says: "A placed object for monarchs alone is refused at once wherever the player stands, with no walk, to anyone but the head of an allegiance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MONARCH-ONLY"),
        station: "yalain-testkit::cpu::entry::places::an_object_for_monarchs_alone_is_refused_at_once_to_anyone_else",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.person-kind-answers",
        says: "A placed object of the script library's person kind with nothing to say is answered \
               as a person: near, at once, status 0, flags 956, nothing said; far off, the player \
               is walked to within 5 m first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-KIND"),
        station: "yalain-testkit::cpu::entry::places::a_person_kind_object_with_nothing_to_say_answers_as_a_person",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.person-kinds-read",
        says: "Placed objects that answer as persons are those of the script library's person kind; \
               faction shrines are a kind of their own; portals and lifestones are neither.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-KINDS"),
        station: "yalain-testkit::dat::interactions::person_kind_objects_and_faction_shrines_are_told_by_their_script_kinds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.places.person-walks-its-route",
        says: "A placed person whose row names a route walks it as a patrolling creature walks its \
               own: to each waypoint in turn at the route's gait and its scale, resting there as long \
               as the waypoint says, back and forth or round a loop; its places are told to the \
               clients that see it, and a client new to it makes it where it stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-ROUTE"),
        station: "yalain-testkit::cpu::play::people_routes::a_placed_person_with_a_route_walks_it_resting_at_each_end_and_turning_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.portal-effects-on-arrival",
        says: "A portal's own effects (a quest given or moved on, an effect put on) take hold as the \
               player comes out of portal space, before the use is answered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PORTAL-EFFECTS"),
        station: "yalain-testkit::cpu::entry::places::a_portals_effects_are_put_on_as_the_player_comes_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.portal-instance-gate-statuses",
        says: "A placed instance's quest gate asks for its quest under way at a phase, under way at \
               any phase, or completed: never had is refused 10019, held otherwise 10020.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PORTAL-INSTANCE-STATUS"),
        station: "yalain-testkit::cpu::entry::places::a_placed_instances_gate_asks_its_quest_at_a_phase_under_way_or_completed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.portal-instance-kept-by-a-quest",
        says: "A portal whose placed instance needs a quest its template does not is refused as a \
               template's gate refuses: a use at once wherever the player stands, a walk-in alike, \
               each with the gate's answer, flags 820 and the distance, whether or not where it \
               leads is known; the same template placed without the gate takes the player.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PORTAL-INSTANCE-QUEST"),
        station: "yalain-testkit::cpu::entry::places::a_portal_its_placed_instance_keeps_closed_by_a_quest_is_refused_at_a_use_and_a_walk_in",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.portal-kept-by-a-quest",
        says: "A portal that needs a quest the player lacks is refused at once wherever the player \
               stands, with no walk and no trip; with the quest where it needs it the portal takes \
               the player.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PORTAL-QUEST"),
        station: "yalain-testkit::cpu::entry::places::a_portal_its_quest_keeps_the_player_out_of_is_refused_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.portal-recall",
        says: "Portal Recall takes a player to where the last portal it used that may be recalled to took \
               it, past portals that may not be; its text comes before the new place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PORTAL-RECALL"),
        station: "yalain-testkit::cpu::play::recalls::portal_recall_returns_to_the_last_portal_used_that_may_be_recalled_to",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.portal-storm",
        says: "Every 11 s a landblock holding the threshold of players brews a portal storm: \
               brewing (0.4) once to each player there with its own cell, imminent (0.6) at the \
               sixth to tenth ticks, at the next tick some are told they were teleported and sent \
               through portal space, at the next the rest are told it subsided (0.0); players \
               elsewhere hear nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PORTAL-STORM"),
        station: "yalain-testkit::cpu::play::storms::a_crowded_landblock_brews_a_portal_storm_that_sends_players_away",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.portal-storm-called-off",
        says: "A brewing portal storm whose landblock falls below the threshold is called off: \
               subsided (0.0) to the players left, and no storm.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PORTAL-STORM-OFF"),
        station: "yalain-testkit::cpu::play::storms::a_brewing_storm_the_crowd_thins_out_of_is_called_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.portal-trip",
        says: "Using a portal in reach flashes the portal and puts the player in portal space; a \
               second and a half later it leaves portal space, the use is answered, and it is \
               placed at the destination with a newer teleport stamp; a player that saw it at the \
               old place sees it leave.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PORTAL"),
        station: "yalain-testkit::cpu::entry::places::a_portal_takes_the_player_through_portal_space_to_its_destination",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.recall-interrupted",
        says: "Moving while the lifestone recall powers up cuts it short: the use over, \n               interrupted, the player not attacking; no vigor is spent and no wait begins.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RECALL-INTERRUPTED"),
        station: "yalain-testkit::cpu::play::specials::moving_while_the_recall_powers_up_interrupts_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.recall-never-bound",
        says: "A player never bound to a lifestone that recalls plays the whole recall (power-up, \n               story, all its vigor), is told it failed, and gets the skill's information and the \n               wait every skill shares; it goes nowhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RECALL-FAILS"),
        station: "yalain-testkit::cpu::play::specials::a_recall_never_bound_plays_and_fails",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.summoned-portal-nowhere",
        says: "A summoner keeping no place of its skill's kind plays the whole summoning and opens \
               no portal; its use ends as it would.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SUMMONED-PORTAL-NOWHERE"),
        station: "yalain-testkit::cpu::play::summoned_portals::a_summoner_never_bound_opens_no_portal",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.summoned-portal-used",
        says: "Anyone may use a summoned portal: it flashes, and the user goes through portal space \
               to the summoner's place of the portal's kind (not its own) and is answered with the \
               teleported flags.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SUMMONED-PORTAL-USE"),
        station: "yalain-testkit::cpu::play::summoned_portals::anyone_may_use_a_summoned_portal_to_the_summoners_place",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.summoned-portals",
        says: "Summon Portal and Summon Lifestone play as the recall does to their story, at half \
               the user's vigor; 0.69 s after the story the summoner is told it summoned a portal \
               and the portal appears 3 m straight ahead of it at its height, facing its way; the \
               use ends 0.90 s later and the portal goes 30 s after it appeared.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SUMMONED-PORTAL"),
        station: "yalain-testkit::cpu::play::summoned_portals::summon_portal_opens_a_portal_ahead_of_its_summoner_that_goes_after_thirty_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.use-from-rest-in-reach",
        says: "A player at rest within a person's reach and the 4 m a client at rest is allowed is \
               answered at once when it uses the person, without a further report of its place; \
               farther away it is walked there first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WALK-REST"),
        station: "yalain-testkit::cpu::play::talk::a_player_at_rest_a_little_out_of_reach_is_answered_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.vault-needs-its-glyph",
        says: "A vault used bare, or with a glyph not its own, is refused as needing its glyph and \
               flashes; the glyph is kept and the player goes nowhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-VAULT-GLYPH-NEEDED"),
        station: "yalain-testkit::cpu::play::vaults::a_vault_used_bare_or_with_another_glyph_needs_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.vault-one-trip-at-a-time",
        says: "A vault another player's trip is going through refuses a glyph at once; a trip that \
               would come within five seconds of its last is refused as the glyph's use ends, the \
               glyph kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-VAULT-REFUSALS"),
        station: "yalain-testkit::cpu::play::vaults::a_vault_refuses_while_another_goes_through_and_too_soon_after",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.walk-interrupted",
        says: "A player walked toward an object it used that moves by its own hand once the walk \
               is taken up has interrupted it: the walk is stopped and the use refused (54), with \
               the flags of its kind, at the distance the player now stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WALK-INTERRUPTED"),
        station: "yalain-testkit::cpu::entry::places::a_walk_to_a_portal_the_player_moves_off_by_hand_is_refused_as_interrupted",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.places.walked-into-portal",
        says: "Walking into one of the world's portals takes the player through it as using it \
               does, as their bodies come to touch, and not again while they stay touching; a \
               summoned portal is walked through.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WALK-IN-PORTAL"),
        station: "yalain-testkit::cpu::entry::places::walking_into_a_portal_takes_the_player_as_a_use_does_and_once_while_touching",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.plants.cross",
        says: "A ripe dye plant pollinated with a pollen of a known cross gives so many seeds of \
               the cross's species and nothing else (Berimphur with Hennacin pollen: three Siennan \
               seeds, \"You pluck 3 seeds from the plant.\"; Aurim with Siennan pollen: two Colban).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYE-PLANT-CROSS"),
        station: "yalain-testkit::cpu::play::dye_plants::a_plant_pollinated_for_a_known_cross_gives_the_seeds_of_the_cross",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.plants.examined",
        says: "Examined, a growing dye plant shows its name, an empty description, its need of water \
               and of fertilizer, its pollen and a countdown to the game time it is ripe (180 s after \
               planting); a ripe one that it is now mature; then its level, kingdom, health, vigor \
               and armor; sent again as it changes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYE-PLANT-PROFILE"),
        station: "yalain-testkit::cpu::play::dye_plants::examining_a_plant_shows_its_needs_its_pollen_and_when_it_is_ripe",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.plants.grow-through-three-looks",
        says: "A dye plant takes its first growing look at 45 s or at a thought after, its second at \
               90 s, and its species' ripe look (its plant item's) at 135 s, shown to whoever sees it \
               and to whoever comes to see it later; it thinks every 10 to 20 s.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYE-PLANT-LOOKS"),
        station: "yalain-testkit::cpu::play::dye_plants::a_plant_takes_two_growing_looks_and_then_its_species_ripe_look_as_it_ages",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.plants.harvested",
        says: "A dye plant used plays the plants' behavior and is answered when it has played out \
               (956, \"Item use succeeds.\") with a text: not ripe, \"not mature yet\", and it stays; not \
               the user's, left alone; ripe, its planter's pet removed, the plant gone, its plant item \
               and a pollen of its kind made in the pack.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYE-PLANT-HARVEST"),
        station: "yalain-testkit::cpu::play::dye_plants::a_ripe_plant_harvested_by_its_planter_gives_its_plant_and_its_pollen",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.plants.leave-with-their-planter",
        says: "A planter leaving the world takes its dye plants (its pets) out of it, as any pet's \
               owner does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYE-PLANT-LEAVES"),
        station: "yalain-testkit::cpu::play::dye_plants::a_planter_leaving_the_world_takes_its_plants_with_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.plants.needs-and-care",
        says: "A new dye plant badly needs water and fertilizer; its needs grow at its thoughts until \
               it is ripe (by nothing, one level or two); a watering can lowers its need of water by \
               one level and a bag of fertilizer its need of fertilizer, once their use has played.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYE-PLANT-CARE"),
        station: "yalain-testkit::cpu::play::dye_plants::a_watering_can_and_a_bag_of_fertilizer_each_lower_their_need_by_one_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.plants.planted-from-a-seed",
        says: "A seed used in its garden plays the planting with the seed in hand; when that has \
               played out (2.0 s) the planter is told of a new pet (the plants' class and flags, no \
               icon), the plant stands two meters straight ahead facing as the planter faces, a seed \
               is spent and the use answered \"You use {the} %1.\" (9021); selected, the plant shows its \
               health and vigor.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYE-PLANTING"),
        station: "yalain-testkit::cpu::play::dye_plants::a_seed_used_in_its_garden_stands_its_plant_two_meters_ahead_once_its_planting_has_played",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.plants.pollinated",
        says: "Pollen used on a dye plant is used up (319) and brushes onto its leaves, told after the \
               answer; on a plant already pollinated it does not stick (the breeze blows it away) and \
               the plant keeps the first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYE-PLANT-POLLEN"),
        station: "yalain-testkit::cpu::play::dye_plants::pollen_brushes_onto_a_plant_once_and_is_blown_away_from_one_already_pollinated",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.plants.species-from-the-install",
        says: "Each dye plant species' seed summons the plant of its color, the plant plays the plants' \
               behavior, the harvest's plant item carries the plant's name, the pollen may be used \
               on its plant, and the ripe look is the plant item's look, as the install defines them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYE-PLANT-SPECIES"),
        station: "yalain-testkit::dat::dye_plants::each_species_seed_plant_plant_item_pollen_and_ripe_look_agree_with_the_install",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.plants.wither",
        says: "A dye plant whose need grows past \"Death imminent!\" withers at its thought, and one \
               cared for past \"Far too much!\" withers at once: its planter is told why, its pet \
               removed, and it leaves the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-DYE-PLANT-WITHERS"),
        station: "yalain-testkit::cpu::play::dye_plants::a_plant_neglected_past_death_or_cared_for_past_far_too_much_withers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.play.calls-read-in-the-client-library",
        says: "Every call the server sends in a play of the start area (a fight to a level, a \
               corpse opened, an item taken, a quest given, a store opened and a sale bought) \
               reads back in the client's own library: the values carry the tags of the handler's \
               declared parameters, and every script object is of its package's size.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PLAY-LIBRARY"),
        station: "yalain-testkit::corpus::play::every_call_of_a_play_reads_back_against_the_client_library",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.play.recorded-families-by-shape",
        says: "The January 2017 recordings of the start area's families (a kill's award, a \
               level-up, a creature's death as seen, a corpse opened, an item taken, an attack \
               begun, a store entered, a quest moved on by a person, a sale bought) hold the \
               messages this server sends, in its order, in most occurrences; every other \
               occurrence is counted with what it lacks.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PLAY-FAMILIES"),
        station: "yalain-testkit::corpus::play::the_recorded_play_families_hold_the_shapes_this_server_sends",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.player-fights.anywhere",
        says: "Two players of different kingdoms whom kingdoms may fight anywhere (their permissions \
               letting kingdoms fight them anywhere) fight in a peace area, each passing through \
               its own kingdom alone; while one is protected it passes through every kingdom again, \
               told as the protection begins and ends, and a run on it is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PVP-ANYWHERE"),
        station: "yalain-testkit::cpu::play::player_fights::players_whom_kingdoms_may_fight_anywhere_fight_in_peace_unless_protected",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.player-fights.death-names-the-killer",
        says: "A player killed by another is announced, to everyone who sees it, as killed by that \
               player: the killer's name as the world writes it, and its object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PVP-DEATH"),
        station: "yalain-testkit::cpu::play::player_fights::a_player_killed_by_another_is_announced_with_its_killers_name",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.player-fights.harm-over-time",
        says: "A damage over time another player put on a player is told, each tick, by that player with \
               the caster its attacker and the health lost as PK damage; with no health left the \
               player dies at the caster's hand, and dead, its ticks are still told and change \
               nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PVP-OVER-TIME"),
        station: "yalain-testkit::cpu::play::player_fights::a_damage_over_time_from_another_player_is_pk_damage_and_kills",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.player-fights.hit-is-pk-damage",
        says: "A player's attack run swings at another player where who may harm whom allows it; \
               each hit's story takes the target's health and gives the same amount as PK damage, \
               and the target's health falls by it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PVP-HIT"),
        station: "yalain-testkit::cpu::play::player_fights::a_hit_on_another_player_takes_its_health_and_tells_it_as_pk_damage",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.player-fights.refused",
        says: "An attack on another player that who may harm whom forbids is refused: the basic \
               attacks fail with the reason, the player is not attacking, and the other is not \
               struck.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PVP-REFUSED"),
        station: "yalain-testkit::cpu::play::player_fights::an_attack_on_another_player_the_rule_forbids_is_refused_with_its_reason",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.player-fights.special-effects-reach",
        says: "A special's target effects reach the other player it lands on, cast by its user.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PVP-SPECIAL-EFFECTS"),
        station: "yalain-testkit::cpu::play::player_fights::a_specials_effects_reach_the_other_player_it_lands_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.player-fights.special-is-pk-damage",
        says: "Where who may harm whom allows it, a special aimed at another player is used as on a \
               creature: its story from the player with the use's number and the skill, each hook \
               that hurts the other giving the health lost as PK damage, and its health falling by \
               it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PVP-SPECIAL"),
        station: "yalain-testkit::cpu::play::player_fights::a_special_at_another_player_hurts_it_and_tells_the_pk_damage",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.player-fights.special-refused",
        says: "A special skill aimed at another player that who may harm whom forbids is refused with \
               the reason, and the other is not struck.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PVP-SPECIAL-REFUSED"),
        station: "yalain-testkit::cpu::play::player_fights::a_special_at_another_player_the_rule_forbids_is_refused_with_its_reason",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.player-fights.strike-lands",
        says: "A strike at another player lands at the time one at a creature does, hurting it \
               then, and the kill text follows the health. A player gone into portal space by \
               then is not hurt. Our choice: no recording shows a player's health falling to \
               another player's basic attack, so the time is the one measured at creatures and \
               the rest follows creatures' blows.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PVP-STRIKE-LANDS"),
        station: "yalain-testkit::cpu::play::player_fights::a_strike_at_another_player_hurts_it_as_it_lands_and_the_kill_text_comes_after",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.player-fights.who-may-harm",
        says: "Who may harm whom is the client's own check of a harmful act between two players: \
               never a dead or protected target, never where the target stands in peace nor \
               across places of different statuses, anyone where all fight, else only a player of \
               another kingdom, and in a controlled area neither of them neutral; a player whom \
               kingdoms may fight anywhere holds a peace area as kingdom against kingdom \
               anywhere, unless protected.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PVP-WHO"),
        station: "yalain-testkit::cpu::play::player_fights::who_may_harm_whom_follows_the_clients_check_of_a_harmful_act",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.presence.force-create-answered",
        says: "An object the client asks for again is made for it again when it is within the \
               player's view: another player, an object of the world, or the player itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FORCE-CREATE"),
        station: "yalain-testkit::cpu::entry::presence::an_object_asked_for_again_is_made_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.presence.head-and-look-at-relayed",
        says: "A player's head turns and the object it looks at reach every client that knows \
               the player.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HEAD-RELAY"),
        station: "yalain-testkit::cpu::entry::presence::head_turns_and_looking_at_reach_the_clients_that_know_the_player",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.presence.interior-view-by-visible-cells",
        says: "Indoors a player's client is told of what is in its own cell and in the cells \
               that cell sees: an object, a creature or another player in one of those is made \
               for it (another cell's once its scene is drawn), one elsewhere in the place only \
               as the player's cell comes to see it; what it was told of stays known within the \
               place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-INTERIOR-VIEW"),
        station: "yalain-testkit::cpu::entry::presence::objects_in_an_interior_are_made_as_the_players_cell_comes_to_see_theirs",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.presence.leaving-seen",
        says: "A player that leaves the world is seen to leave by every client that held it, on \
               its stream of the player's places, with the player's next position stamp.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LEAVE-FORGOTTEN"),
        station: "yalain-testkit::cpu::entry::presence::a_player_leaving_the_world_is_seen_to_leave_by_the_others",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.presence.others-held-weapons-are-made-in-hand",
        says: "A weapon another player holds is made in its hand for those who see it, when it is \
               wielded beside them and when they come into view; put away, the holder lets go of \
               it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SEEN-HELD"),
        station: "yalain-testkit::cpu::play::wearing::a_weapon_another_player_holds_is_made_in_its_hand_for_those_who_see_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.presence.out-of-view-let-go",
        says: "Nothing is sent as a player goes out of another's view (the client lets it go \
               itself); its movements stop reaching that client, and it is made for it again \
               when it comes back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-VIEW"),
        station: "yalain-testkit::cpu::entry::presence::a_player_out_of_view_is_let_go_and_made_again_when_it_returns",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.presence.players-made-for-each-other",
        says: "Two players in one place are made for each other: each client is told of the \
               other player's object, named and placed as the world shows it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PLAYERS-SEEN"),
        station: "yalain-testkit::cpu::entry::presence::two_players_in_one_place_are_made_for_each_other",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.presence.positions-reach-observers",
        says: "A player's reported place reaches every client that knows it as the player's \
               position, stamped newer each time, on that client's own ephemeral stream for the \
               player.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-POSITION-RELAY"),
        station: "yalain-testkit::cpu::entry::presence::a_players_reported_place_reaches_the_clients_that_know_it_with_newer_stamps",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-board-gives-what-it-lists",
        says: "A postings board gives one of the quests its own description lists, saying its \
               line, and no other while that one is under way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-GIVERS-BOARD"),
        station: "yalain-testkit::corpus::world_db::a_postings_board_gives_one_of_the_quests_it_lists_at_a_time",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.a-giving-says-its-numbered-line",
        says: "A person whose table numbers its quests says, as it gives one, the line \
               numbered for that quest.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-LINES"),
        station: "yalain-testkit::corpus::world_db::a_person_says_the_line_numbered_for_the_quest_it_gives",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.a-heros-quest-only-for-a-hero",
        says: "A quest whose setup says heroes alone is given only to a hero, by a kill or a step, and \
               to anyone else nothing is given or said. Heroes were given such quests in all 14 known \
               cases in 2017; no one else's giving was recorded either way, so the rest follows the \
               setup.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-HERO-ONLY"),
        station: "yalain-testkit::cpu::play::kill_quests::a_kill_or_a_step_gives_a_heros_quest_only_to_a_hero",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-kill-advances-from-the-phase-before",
        says: "An effect that advances a quest to a phase moves it there only from the phase \
               before, and one that completes it only from its last phase; from any other phase \
               nothing moves and nothing is told.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-ADVANCE"),
        station: "yalain-testkit::cpu::play::kill_quests::an_advance_moves_from_the_phase_before_and_a_completion_from_the_last_phase",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-kill-gives-a-quest",
        says: "A creature whose death gives a quest gives it to its killer at the first phase, when \
               the killer has never had it, or has done or failed it and its wait is over.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-BESTOW"),
        station: "yalain-testkit::cpu::play::kill_quests::a_kill_gives_its_quest_to_a_killer_free_to_take_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-kill-moves-a-quest-under-way-on",
        says: "A creature whose death increments a quest moves its killer's quest under way on a \
               phase, each kill one, and the kill at the last phase completes it; each move is told \
               as an advance to the phase, then the completion.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-INCREMENT"),
        station: "yalain-testkit::cpu::play::kill_quests::each_kill_moves_a_quest_under_way_on_a_phase_and_the_last_completes_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-kill-moves-no-quest-not-under-way",
        says: "A kill moves on no quest its killer does not have under way, never had or done, and \
               tells nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-NOT-HELD"),
        station: "yalain-testkit::cpu::play::kill_quests::a_kill_moves_on_no_quest_its_killer_does_not_have_under_way",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-kill-reaches-fellows-as-its-quest-says",
        says: "A kill's move reaches the killer's fellows when the quest shares, those it limits \
               only within ten levels of the killer (inferred) and within 160 meters of it; a fellow is moved \
               as the killer is, never told a refusal, and a quest that does not share stays the \
               killer's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-FELLOWS"),
        station: "yalain-testkit::cpu::play::kill_quests::a_kill_moves_a_fellows_quest_only_as_the_quest_says",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-kill-sends-what-2017-sent",
        says: "Every update the January 2017 servers sent of a sample of kill quests, played by \
               killing a creature of the install that names the quest, is sent with the same kind, \
               reason and record: 263 of 263.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-REPLAY"),
        station: "yalain-testkit::corpus::world_db::a_kill_sends_the_updates_recorded_kills_were_sent",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.a-kill-step-comes-before-its-effect",
        says: "A world's own step a kill triggers runs first, and a quest it moved is not moved \
               again by the creature's effect at that kill.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-STEP-FIRST"),
        station: "yalain-testkit::cpu::play::kill_quests::a_kills_step_runs_first_and_its_quest_is_not_moved_again_by_the_effect",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-kills-giving-needs-the-least-level",
        says: "A kill does not give a quest to a killer under the quest's least level: the killer is \
               told so in an update of the giving kind whose record holds nothing, a fellow is told \
               nothing. In January 2017 two such kills were refused so, and all 83 givings of known \
               level were at or above it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-LEAST-LEVEL"),
        station: "yalain-testkit::cpu::play::kill_quests::a_killer_under_a_quests_least_level_is_refused_it_and_told",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-kills-reach-from-setup",
        says: "How a kill's move on a quest reaches fellows is read from the quest's setup: shared \
               or not, each limit named or not; a quest with no setup of its own shares nothing \
               unless a row says so. A last phase spanning two waits for something other than a \
               kill.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-SETUP"),
        station: "yalain-testkit::dat::quests::a_quests_setup_says_how_a_kill_reaches_fellows_and_its_definition_whether_its_last_phase_waits",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.quests.a-kills-refused-giving-is-told",
        says: "A killer whose kill would give a quest it has under way, whose wait is not over or \
               that it has done once is told why, in an update of the giving kind holding a record \
               of nothing, and its quest stays as it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-REFUSED"),
        station: "yalain-testkit::cpu::play::kill_quests::a_killer_is_told_why_a_kills_giving_was_refused_and_its_quest_stays_as_it_was",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-last-phase-that-waits-holds",
        says: "A kill does not end a quest whose last phase waits for something else; it stays at \
               that phase.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-LAST-WAITS"),
        station: "yalain-testkit::cpu::play::kill_quests::a_last_phase_that_waits_for_something_else_is_not_ended_by_a_kill",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-last-phases-subquests-finish-the-quest",
        says: "A quest's last phase done by subquests is the quest done, with the move that finishes the last of them, and that person says the quest's completion.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LAST-SUBQUESTS"),
        station: "yalain-testkit::cpu::play::quest_uses::a_last_phase_done_by_subquests_is_the_quest_done_and_its_person_says_its_completion",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-person-gives-and-takes-back-kill-quests",
        says: "A sample of kill quests the January 2017 recordings hold from giving to end, played \
               through the database's rows and the creatures' death effects, sends only updates \
               those servers sent, and ends at the use of the person who took them back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-GIVERS"),
        station: "yalain-testkit::corpus::world_db::a_person_gives_a_kill_quest_and_takes_it_back_as_the_recordings_show",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.a-person-sends-whoever-uses-it",
        says: "A person who sends whoever uses it says its line and sends them to where \
               2017's users landed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-SENDS"),
        station: "yalain-testkit::corpus::world_db::a_person_who_sends_whoever_uses_it_says_its_line_and_sends_them_where_2017_landed",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.a-person-sends-whoever-uses-it-speaks",
        says: "A person who sends whoever uses it says its line as it does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PERSON-SENDS-LINE"),
        station: "yalain-testkit::cpu::play::talk::a_person_who_sends_whoever_uses_it_says_its_line_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-persons-wait-runs-only-in-the-world",
        says: "A person's wait after giving a quest counts the player's time in the world, not the time it was away.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-CLOCK-PERSON"),
        station: "yalain-testkit::cpu::play::quest_uses::a_persons_wait_counts_only_the_players_time_in_the_world",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-posting-waits-its-timer",
        says: "A postings board gives its quest with its unseen under-way quest, no other \
               until it is done, and none for its wait after.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-POSTINGS"),
        station: "yalain-testkit::corpus::world_db::a_posting_is_given_finished_and_waited_for_as_its_definition_says",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.a-reminder-at-its-phase-sends-no-one",
        says: "A person reminds a player at the phases heard, sending no one: a portal beside it does the sending.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LAHRY-REMINDERS"),
        station: "yalain-testkit::corpus::world_db::lahry_reminds_a_player_at_the_fifth_phase_and_the_portal_beside_him_sends_them",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.a-required-quest-holds-back-a-giving",
        says: "A quest whose setup names a required quest and status is given, by a kill or a step, \
               only to a player with that quest at that status; otherwise nothing is given or said. In \
               January 2017 all 16 kills naming such a quest whose required quest was never had gave \
               nothing and said nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-REQUIRED"),
        station: "yalain-testkit::cpu::play::kill_quests::a_kill_or_a_step_gives_a_quest_only_with_the_quest_it_requires_at_its_status",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-saved-quest-keeps-what-it-had-left",
        says: "A quest under way in a save keeps the time it had left when it was kept, neither lost nor given time back.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-CLOCK-SAVES"),
        station: "yalain-testkit::cpu::entry::keeping::a_quest_under_way_in_a_save_keeps_the_time_it_had_left_when_it_was_kept",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-step-may-fail-its-quest",
        says: "A step may fail a quest under way, told as a failure; a quest with no wait may be given again by the next use.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STEP-FAIL"),
        station: "yalain-testkit::cpu::play::quests::a_step_may_fail_its_quest_and_it_may_then_be_given_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-step-requires-a-quest-free",
        says: "A step may require another quest free to be given: never had, or its wait \
               over.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-POSTINGS-FREE"),
        station: "yalain-testkit::cpu::play::quests::a_step_that_requires_a_quest_free_waits_for_that_quests_wait",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-strangers-kill-moves-nothing",
        says: "A kill moves nothing for a player who is no fellow of the killer, whatever it holds \
               and however near it stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-STRANGER"),
        station: "yalain-testkit::cpu::play::kill_quests::a_kill_moves_nothing_for_a_player_who_is_no_fellow_of_the_killer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.a-use-that-leads-on",
        says: "A use of a person or an object whose step leads somewhere tells its update and \
               takes the player in the same instant.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-USE-LEADS-ON"),
        station: "yalain-testkit::cpu::play::quests::a_persons_step_that_leads_somewhere_tells_its_update_then_takes_the_player",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.an-effects-name-names-its-quest",
        says: "An effect's name says what it does to which quest: the kind, the quest by the \
               enumeration's name with or without the separator, and for an advance the phase; a \
               name of any other kind, or of no quest, says nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-NAMES"),
        station: "yalain-testkit::cpu::play::kill_quests::an_effects_name_says_what_it_does_to_which_quest",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.an-item-acquired-moves-its-quest",
        says: "An item coming into a player's hands moves the quest its template names on being \
               acquired, from the phase before and no other, after any step on its taking and in the \
               place of its move.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ACQUIRE-EFFECTS"),
        station: "yalain-testkit::cpu::play::kill_quests::an_item_acquired_moves_the_quest_its_template_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.an-obelisk-search-leads-on",
        says: "Each giving and each stone's advance of an obelisk search moves its quest and sends \
               no one; the portal beside each stone takes the seeker on to the next.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-OBELISK-SEARCH"),
        station: "yalain-testkit::corpus::world_db::an_obelisk_search_moves_on_at_each_stone_and_the_portal_beside_it_goes_on",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.an-unrecorded-quest-reaches-no-fellow",
        says: "A quest with no setup and no recording to say otherwise moves for its killer and \
               for no fellow, near as the fellow stands and whatever it holds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-UNRECORDED"),
        station: "yalain-testkit::corpus::world_db::a_quest_with_no_setup_and_no_recording_moves_its_killers_quest_and_no_fellows",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.cancel-a-quest",
        says: "A quest under way that may be cancelled is cancelled on request: one update of the \
               cancel kind with the quest's record, and the quest no longer under way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-CANCEL"),
        station: "yalain-testkit::cpu::play::own_requests::a_quest_cancelled_is_updated_as_cancelled_and_no_longer_under_way",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.chain-has-no-person-wait",
        says: "A person whose quests follow one another gives the next as soon as the last is done, with no two \
               hours' wait between them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-CHAIN-2017"),
        station: "yalain-testkit::cpu::play::quest_uses::a_person_whose_quests_follow_one_another_gives_the_next_at_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.completion-adds-points",
        says: "A quest's completion adds its points to its player's quest rating: whole, whatever its challenge against the player's level, and every time; a row may say otherwise than its setup.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-POINTS"),
        station: "yalain-testkit::cpu::play::quests::a_completion_adds_its_points_to_the_rating_every_time_whatever_the_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.completion-paid-whole",
        says: "A quest whose setup says its challenge is ignored pays its completion whole, whatever \
               its challenge against the player's level.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-PAID-WHOLE"),
        station: "yalain-testkit::cpu::play::quests::a_completion_its_setup_says_ignores_its_challenge_is_paid_whole",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.completion-pays-a-share",
        says: "A quest may pay a share of what its player's level takes to reach the next, rounded \
               down, at least and at most what its setup or its row says.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-PAY-SHARE"),
        station: "yalain-testkit::cpu::play::quests::a_completion_pays_a_share_of_what_the_level_takes_within_its_limits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.completion-pays-by-challenge",
        says: "A quest's completion pays its experience weighed by its challenge against the \
               player's level as a kill's is weighed; a quest without a challenge pays it whole.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-XP"),
        station: "yalain-testkit::cpu::play::quests::a_completion_pays_its_experience_weighed_by_challenge_against_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.done-once-from-setup",
        says: "Whether a quest is done once is read from its setup in the data: one solution is \
               done once; two, any number, or a package that does not say, is not.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-ONCE-SETUP"),
        station: "yalain-testkit::dat::quests::a_quests_setup_says_whether_it_is_done_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.quests.done-once-without-rows",
        says: "A world whose rows say nothing of how many times a quest may be done still gives a \
               quest its setup says is done once only once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-ONCE-NO-ROWS"),
        station: "yalain-testkit::corpus::world_db::a_quest_its_setup_says_is_done_once_is_not_given_again_whatever_the_rows_say",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.empty-words-are-still-listed",
        says: "A quest whose name and description are empty strings is listed and told as any other; a quest is hidden for naming no description.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-EMPTY-WORDS"),
        station: "yalain-testkit::corpus::world_db::the_quests_2017_listed_with_empty_words_are_listed",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.every-death-effect-names-a-quest",
        says: "Every effect a creature template of the install names that reads as a quest effect \
               names a quest of the install.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KILL-NAMED-ALL"),
        station: "yalain-testkit::dat::quests::every_quest_effect_a_creature_names_names_a_quest_of_the_install",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.quests.every-quest-held-listed-at-entry",
        says: "A character enters the world with every quest it holds in its journal, under way, \
               done, failed or cancelled, each one no longer under way with what is left of its \
               wait before it may be taken again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-LIST-AT-ENTRY"),
        station: "yalain-testkit::cpu::entry::keeping::every_quest_held_is_listed_at_entry_with_what_is_left_of_its_wait",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.failed-waits-to-be-given-again",
        says: "A quest that failed is given again only once its wait is over; until then the person \
               used says its refusal.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-FAILED-WAIT"),
        station: "yalain-testkit::cpu::play::quest_uses::a_failed_quest_is_given_again_only_once_its_wait_is_over",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.fails-on-death",
        says: "A quest that fails when its player dies fails as the player dies, told before the \
               death's message.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-FAILS-ON-DEATH"),
        station: "yalain-testkit::cpu::play::quest_uses::a_quest_that_fails_on_death_fails_as_its_player_dies",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.fails-when-its-time-runs-out",
        says: "A quest with a duration fails when its time runs out: one update tells it failed, \
               with its whole wait before it may be taken again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-RUNS-OUT"),
        station: "yalain-testkit::cpu::play::quests::a_timed_quest_fails_when_its_time_runs_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.given-again-no-wait",
        says: "A done quest with no wait is given again at once; one its definition says is done \
               once is never given again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-GIVEN-AGAIN"),
        station: "yalain-testkit::cpu::play::quest_uses::a_done_quest_with_no_wait_is_given_again_at_once_unless_it_is_done_once",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.given-and-moved-on",
        says: "Using the person a quest's first step names gives the quest after the person's \
               line; using the next one moves it on to its second phase; using the first again \
               does nothing more.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-STEPS"),
        station: "yalain-testkit::cpu::play::trade::quest_givers_give_a_quest_and_move_it_on_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.given-out-of-the-world-counts-from-entry",
        says: "A quest given while its character was out of the world counts from when the character enters.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-CLOCK-GIVEN-AWAY"),
        station: "yalain-testkit::cpu::entry::keeping::time_away_moves_a_quests_time_on_but_never_past_now",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.hidden-never-listed",
        says: "A hidden quest a character holds is never in the journal it enters the world with; \
               the shown ones are.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HIDDEN-NEVER-LISTED"),
        station: "yalain-testkit::cpu::entry::keeping::a_hidden_quest_is_never_listed_at_entry",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.hidden-never-told",
        says: "A hidden quest is given, moved on and completed like any other, but no update of it \
               is ever sent to its player.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HIDDEN-NEVER-TOLD"),
        station: "yalain-testkit::cpu::play::quests::a_hidden_quest_is_held_and_moved_on_but_never_told",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.hidden-pays-quietly",
        says: "A hidden quest's completion pays with no text and no update told.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HIDDEN-PAY"),
        station: "yalain-testkit::cpu::play::quests::a_hidden_quests_completion_pays_with_no_text",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.item-on-object-takes-the-player",
        says: "A held item used on an object that a quest step names with a place: the player \
               plays the use, two and a half seconds later the object flashes and the player \
               goes through portal space; on arrival the quest moves on, the item is taken and \
               the player is at the step's place.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-VAULT"),
        station: "yalain-testkit::cpu::play::quests::an_item_used_on_an_object_takes_the_player_and_moves_the_quest_on_arrival",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.least-level-from-the-setup",
        says: "A quest's least level is the one its setup names unless its row names another, and its \
               setup also says whether it is for heroes alone and which quest it requires at which \
               status.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-SETUP-GATES"),
        station: "yalain-testkit::dat::quests::a_quests_setup_says_who_it_is_for",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.quests.least-level-refused",
        says: "A quest is not given to a player under its least level; the person says its refusal \
               of that quest instead.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-LEAST-LEVEL"),
        station: "yalain-testkit::cpu::play::quest_uses::a_quest_is_not_given_under_its_least_level_and_the_person_says_why",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.monarch-only-is-read",
        says: "A quest's setup says whether it is for the monarch of an allegiance; one setup says so, and nothing yet holds a quest to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-MONARCH-ONLY"),
        station: "yalain-testkit::dat::quests::a_quests_setup_says_whether_it_is_for_a_monarch",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.quests.one-event-several-steps",
        says: "One use runs every quest step it triggers, each quest moved on at most once, a \
               step another of the same use made possible included; a second use does neither \
               again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-CHAINS"),
        station: "yalain-testkit::cpu::play::quests::one_use_completes_a_quest_and_gives_the_next_that_requires_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.one-giving-at-a-time",
        says: "A person who gives several quests gives one of them at a use, none while one is \
               under way, and, when they have no wait of their own, none for two hours after the \
               last it gave, saying its too-soon line; one whose quests have waits gives the next \
               in the use that completes one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-ONE-AT-A-TIME"),
        station: "yalain-testkit::cpu::play::quest_uses::a_person_with_several_quests_gives_one_at_a_time_and_none_until_its_wait_is_over",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.points-a-pattern-supports",
        says: "Quests no recording measured add what the person's other quests all add, or a guide's figure; where measures disagree they add nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QP-MORE"),
        station: "yalain-testkit::corpus::world_db::quests_no_recording_measured_add_what_their_pattern_or_a_guide_supports",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.points-with-no-setup",
        says: "A quest with no setup adds to a quest rating what 2017's ratings moved by as it was completed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WDB-QUEST-POINTS-2017"),
        station: "yalain-testkit::corpus::world_db::the_quests_measured_with_no_setup_add_what_2017_added",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.postings-pay-a-share",
        says: "The postings pay a share of what the player's level takes to reach the next, at most what it comes to at level 72.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-POSTING-PAY"),
        station: "yalain-testkit::corpus::world_db::a_posting_pays_the_share_the_guide_gives",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.ran-out-before-leaving",
        says: "A quest with no time left when its character left the world is failed when the \
               character enters, its wait counted from when the time ran out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-RAN-OUT-AWAY"),
        station: "yalain-testkit::cpu::entry::keeping::a_quest_with_no_time_left_when_its_player_left_is_listed_failed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.ratings-as-2017",
        says: "A player's rating moved in 2017 by the points of what it completed between two of its profiles in a session.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-RATINGS-2017"),
        station: "yalain-testkit::corpus::world_db::a_players_rating_moved_in_2017_by_the_points_of_what_it_completed",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.recorded-givings",
        says: "People give the quests 2017 recorded them giving, with their lines, their reminder and their under-level refusal, and what the journal asks for finishes them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WDB-RECORDED-GIVINGS-2017"),
        station: "yalain-testkit::corpus::world_db::people_give_the_quests_2017_recorded_and_say_their_lines_for_them",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.required-quests-have-rows",
        says: "The quests other quests' setups require stand in the world.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REQUIRED-QUESTS"),
        station: "yalain-testkit::corpus::world_db::the_quests_other_quests_require_are_in_the_world",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.reward-done-with-its-quest",
        says: "A hidden reward quest is completed with the quest it rewards, untold, so an item that \
               needs the reward (a Crystalline Tyrant Egg) works once that quest is done.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-REWARDS"),
        station: "yalain-testkit::cpu::play::items::a_reward_quest_is_completed_with_the_quest_it_rewards",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.step-effects-after-the-update",
        says: "A step whose effects come last (the Esper Warder's bestowal of three weapon recipes) tells the \
               quest's update first and then the recipes learned; other steps tell their effects first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-EFFECTS-LAST"),
        station: "yalain-testkit::cpu::play::crafting::a_step_with_its_effects_last_tells_the_update_before_the_recipe",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.step-for-some-races",
        says: "A quest step that names races runs only for a player of one of them: the Esper Warder \
               gives each race its own three weapon recipes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STEP-RACES"),
        station: "yalain-testkit::cpu::play::quests::a_step_for_some_races_runs_only_for_a_player_of_one_of_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.step-on-a-quest-given",
        says: "A quest given, a hidden one too, runs the step waiting on its giving; one withheld from a player \
               without what it requires runs none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-GIVEN-2017"),
        station: "yalain-testkit::cpu::play::crafting::a_quest_given_runs_the_step_waiting_on_it_and_one_withheld_runs_none",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.step-on-a-quest-given-row",
        says: "A world database step may name a quest's giving as what moves it, and that reads back as written.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-GIVEN-ROW"),
        station: "yalain-world-db::cpu::database::a_step_on_a_quest_given_compiles_and_reads_back_as_written",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.subquests-read",
        says: "A quest's phases read from the data the quests they are done by, and whether all of \
               them or one is needed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SUBQUESTS-READ"),
        station: "yalain-testkit::dat::quests::a_quests_phases_read_the_subquests_they_are_done_by",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.quests.subquests-told-as-their-quest",
        says: "A quest whose phase is done by subquests gives them with the phase and tells each of \
               their moves as its own record, its phase kept and its journal the phase's entry with \
               those of the subquests still open; the last done moves it on to its next phase; no \
               subquest is told or listed on its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SUBQUESTS"),
        station: "yalain-testkit::cpu::play::quest_uses::a_quests_subquests_are_told_as_its_record_and_the_last_moves_it_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.the-antechamber-code",
        says: "Three obelisks touched in the recorded twelve-touch order send the player on; a wrong touch fails the hidden quest and the order starts over.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WDB-ANTECHAMBER-2017"),
        station: "yalain-testkit::corpus::world_db::three_obelisks_touched_in_their_order_lead_on_and_a_wrong_touch_starts_over",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.time-and-waits-as-2017-recorded-them",
        says: "Between two recorded entries of a character, a timed quest's time left fell by no more than the time that passed and a wait to be taken again by all of it, and this server's records say the same from the same time away.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-CLOCK-RECORDED"),
        station: "yalain-testkit::corpus::keeping::a_timed_quests_time_runs_in_play_and_a_wait_on_the_calendar_across_recorded_entries",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.time-runs-only-in-the-world",
        says: "A timed quest's time stands still while its player is away, and when it was given moves on by the time away.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-CLOCK"),
        station: "yalain-testkit::cpu::entry::keeping::a_timed_quests_time_stands_still_while_its_player_is_away",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.timed-as-the-records-show-them",
        says: "Every record of a quest under way shows time left at the phases the world's row \
               times, and a quest the row times in one phase shows none at the others.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WDB-QUEST-TIMED-PHASES-2017"),
        station: "yalain-testkit::corpus::world_db::every_phase_a_recorded_quest_shows_time_at_is_one_the_world_times",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.timed-in-one-phase",
        says: "A quest whose duration runs in one phase shows its time and may fail of it only in \
               that phase. In a first phase the time counts from the giving, in a later one the \
               whole of it starts as that phase comes, with when it was given unchanged. Its time \
               stands still out of the world there too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-TIMED-PHASE"),
        station: "yalain-testkit::cpu::play::quests::a_quest_timed_in_a_later_phase_counts_from_when_that_phase_comes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.under-level-line-is-the-persons",
        says: "A person says its under-level line to a player too low for what it gives, and to no one else.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-UNDER-LEVEL-LINE"),
        station: "yalain-testkit::corpus::world_db::a_person_says_its_under_level_line_to_a_player_too_low_and_to_no_one_else",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.under-level-line-out-of-the-plain-pool",
        says: "A placed person with anything else to say never says its under-level line to anyone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-UNDER-LEVEL-LINES"),
        station: "yalain-testkit::corpus::world_db::no_placed_person_with_anything_else_to_say_says_its_under_level_line_to_anyone",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.quests.update-effects-come-first",
        says: "A quest update puts on the player the effects the quest's definition names for it (a \
               recipe taught, a reward quest given) before the update itself is told.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-UPDATE-EFFECTS"),
        station: "yalain-testkit::cpu::play::quests::an_update_puts_on_its_definitions_effects_before_it_is_told",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.quests.waits-run-while-away",
        says: "A wait to be taken again runs while its player is away, a done, a failed and a cancelled quest's alike.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-CLOCK-WAITS"),
        station: "yalain-testkit::cpu::entry::keeping::a_wait_to_be_taken_again_runs_while_its_player_is_away",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.regen.a-creature-unhurt-regains-a-share-a-second",
        says: "A creature not hurt for eight seconds regains its greatest health in a minute, \
               in whole points as the share adds up; nothing is sent for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REGEN-CREATURE"),
        station: "yalain-testkit::cpu::play::regeneration::a_creature_unhurt_for_eight_seconds_regains_its_greatest_health_in_a_minute",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.regen.combat-rates-from-effects",
        says: "A player's combat regeneration rates are what its effects give it (a buff such as \
               Robustify, a worn item's bonus) and nothing of its own, so without one it regains \
               nothing in combat; its peace rates are 1 as its effects change them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REGEN-COMBAT-RATES"),
        station: "yalain-testkit::cpu::play::regeneration::the_combat_rates_are_what_the_players_effects_give_it_and_nothing_of_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.regen.combat-when-fighting-hurt-or-moving",
        says: "A player regains at its combat rates for a step when it fought or was hurt at any \
               moment since the last step, or is moving at the step itself; a walk that stopped \
               before the step leaves it at rest.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REGEN-COMBAT-STATE"),
        station: "yalain-testkit::cpu::play::regeneration::a_hurt_or_a_walk_under_way_at_the_step_puts_it_at_the_combat_rates_a_walk_that_stopped_does_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.regen.rate-effects-read-from-their-definitions",
        says: "The effects that change a player's regeneration rates read as the data defines \
               them: Robustify, a crafted item's bonus, a life-regeneration jewel.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REGEN-RATE-EFFECTS"),
        station: "yalain-testkit::dat::effects::the_effects_that_give_regeneration_rates_read_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.regen.share-of-the-greatest-with-effects",
        says: "Each regeneration step gives a share of the vital's greatest value with the \
               player's effects, and fills it to that value.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REGEN-GREATEST"),
        station: "yalain-testkit::cpu::play::regeneration::each_step_is_a_share_of_the_greatest_with_the_players_effects",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.regen.steps-at-rest",
        says: "A player's health and vigor come back on one timer, each step a fresh 3.5 to 7.5 s \
               after the last, health sent before vigor: at rest health fills in a minute and \
               vigor in half of one, each step's gain rounded down, nothing sent for a full \
               vital.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REGEN-STEPS"),
        station: "yalain-testkit::cpu::play::regeneration::at_rest_health_and_vigor_come_back_together_in_steps_of_three_and_a_half_to_seven_and_a_half_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.regen.vigor-from-nothing",
        says: "A player's vigor spent to nothing (a recall takes all of it) comes back at rest, like any vigor below its greatest.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REGEN-VIGOR"),
        station: "yalain-testkit::cpu::play::specials::vigor_spent_to_nothing_by_a_recall_comes_back_at_rest",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.replay.recorded-logins-answered-as-recorded",
        says: "Each January 2017 login replayed against the server, its client's messages sent \
               again in order, is answered with the messages the login server sent, in the same \
               order and on the same queues, apart from the differences the server's register \
               explains.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REPLAY"),
        station: "yalain-testkit::corpus::replay::every_recorded_2017_login_is_answered_as_recorded",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.replay.recorded-world-entries-answered-as-recorded",
        says: "Each January 2017 world entry replayed against the server is answered with the \
               recorded entry's shape, and the drawn scene with the recorded answer, apart from \
               the differences the server's register explains.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORLD-REPLAY"),
        station: "yalain-testkit::corpus::world_entry::every_recorded_2017_world_entry_is_answered_with_the_recorded_shape",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.replay.recorded-world-invariants",
        says: "The January 2017 world connections keep the invariants the server keeps: a \
               message about an object names one the client was told of and has not forgotten, \
               and an object's position and teleport stamps only move forward, apart from the \
               breaches the server's register explains.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORLD-INVARIANTS"),
        station: "yalain-testkit::corpus::invariants::the_recorded_2017_world_connections_keep_the_object_and_stamp_invariants",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.replay.recorded-world-sessions-replayed-whole",
        says: "Each January 2017 world session replayed whole against the server, in a world \
               holding the objects the recording made, keeps the known-object and stamp \
               invariants throughout; its positions, head turns, looks, objects asked for again, \
               selections, examinations, chat, allegiance queries and uses of objects are answered \
               as the tallies show, every difference from the recording named.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SESSION-REPLAY"),
        station: "yalain-testkit::corpus::sessions::every_recorded_2017_world_session_replays_with_the_invariants_kept",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.replay.the-checker-orders-stamps-as-the-client-does",
        says: "The checker of what a world connection sends orders stamps as the client does: the larger of two is \
               newer while they are less than half the circle apart and the smaller from there on, exactly half \
               included.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CHECKER-STAMPS"),
        station: "yalain-testkit::cpu::play::rules::the_checker_orders_stamps_as_the_client_does",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.selection.examining-what-another-holds",
        says: "An item in another player's pack answers no examination until the examiner knows \
               of it, as an offer in a trade makes it known.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-EXAMINE-HELD"),
        station: "yalain-testkit::cpu::entry::examine::an_item_in_another_players_pack_answers_only_once_offered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.selection.vitals-of-the-selected",
        says: "Selecting a player is answered with its selection information: the selected \
               object, then its health, player-killer damage, greatest health, vigor, vigor lost \
               to player killers and greatest vigor.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SELECTION"),
        station: "yalain-testkit::cpu::entry::examine::selecting_a_player_is_answered_with_its_vitals",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.active-cap-for-who-is-no-hero",
        says: "An active skill stops at 50 for a player who is no hero, in its rating and in a \
               raise; a hero's goes on to the skill's own cap.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SKILL-CAP-HEROES"),
        station: "yalain-testkit::cpu::play::skills::an_active_skill_stops_at_fifty_for_who_is_no_hero_and_goes_on_for_a_hero",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.advancement-table",
        says: "A skill's level in its advancement table is the highest whose experience it holds \
               (none still reaches level 1), and a raise costs the experience to the next, none \
               at the top.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SKILL-TABLE"),
        station: "yalain-testkit::cpu::play::skills::a_tables_level_and_raise_cost_follow_the_library",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.area-hooks-read-from-their-definitions",
        says: "Skills used on creatures read with their hooks as the data defines them: an area \
               hook's radius and flags, missiles, target effects, and a toggled skill's aura as \
               its user effect.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AREA-DEFS"),
        station: "yalain-testkit::dat::effects::the_skills_used_on_creatures_read_with_their_areas_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.skills.hero-credits-by-level",
        says: "Reaching a level that brings hero skill credits (from level 45, 100 a level in the \
               client's level table) adds them with the level's skill credit, the repository \
               sent with both.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HERO-CREDITS-BY-LEVEL"),
        station: "yalain-testkit::cpu::play::skills::a_level_from_45_brings_its_hero_skill_credits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.hero-skills-paid-in-hero-credits",
        says: "A hero skill is trained only by a hero (else 30047) and paid in hero skill \
               credits (short, 30006): the hero cost leaves the hero skill credits, the skill \
               is kept at that cost and the repository is sent with the hero skill credits \
               left.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HERO-SKILLS"),
        station: "yalain-testkit::cpu::play::skills::a_hero_skill_is_trained_only_by_a_hero_and_paid_in_hero_skill_credits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.last-use-kept-across-sessions",
        says: "A skill used before a logoff comes back in the next session start with when it was \
               last used, its mask saying so; the time away does not count, so a skill still \
               waiting at the logoff is still waiting at the entry.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KEEP-SKILL-USE"),
        station: "yalain-testkit::cpu::entry::keeping::a_skills_last_use_comes_back_and_its_wait_does_not_run_while_away",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.list-trained-skills",
        says: "Asked, the server lists one line for each skill the player trained, with its \
               experience and its credits, and none for the skills it was given.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SKILL-LIST"),
        station: "yalain-testkit::cpu::play::own_requests::the_trained_skills_list_has_a_line_for_each_trained_skill_only",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.raise",
        says: "Raising a trained skill spends the experience to its advancement table's next \
               level: the skill's information, the experience left, its text; short of \
               experience, a text alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SKILL-RAISE"),
        station: "yalain-testkit::cpu::play::skills::raising_a_skill_spends_the_experience_to_its_tables_next_level",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.rating-at-no-vigor",
        says: "At no vigor a player's rating in a skill that says so is the least there is, and it adds \
               nothing to the skills it is a parent of; with any vigor, and in any other skill, the \
               rating is whole.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SKILL-NO-VIGOR"),
        station: "yalain-testkit::cpu::play::skills::at_no_vigor_a_skill_that_says_so_rates_nothing_and_others_keep_their_rating",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.rating-with-effects",
        says: "A skill's rating counts its holder's effects on that skill, each change by whole \
               points, its parents' as changed and its cap still holding, in what its hooks add, \
               its effects' spellcraft and the rating its information caches.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SKILL-WITH-EFFECTS"),
        station: "yalain-testkit::cpu::play::skills::a_skills_rating_counts_its_own_and_its_parents_effects_up_to_its_cap",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.recall-skills-read",
        says: "Portal Recall, the cities' recalls and Allegiance Recall read as the lifestone's recall: a \
               10 s power-up, only on their user, all its vigor, the 30 s wait; Portal Recall's \
               recall on its target, the others' on their user, the cities' with their protection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RECALL-SKILLS"),
        station: "yalain-testkit::dat::skills::the_recall_skills_read_as_the_lifestones",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.skills.summoning-skills-read",
        says: "Summon Lifestone and Summon Portal read as recalls at half the user's vigor: a 10 s \
               power-up, only on their user, each naming the effect that opens its portal.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SUMMON-SKILLS"),
        station: "yalain-testkit::dat::skills::the_summoning_skills_read_as_recalls_at_half_vigor",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.skills.train",
        says: "Training a skill the player may train spends its credits: the repository with the \
               credits left, the skill's information, its text; a refusal is a text alone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SKILL-TRAIN"),
        station: "yalain-testkit::cpu::play::skills::training_a_skill_spends_its_credits_and_again_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.untrain-after-earning-its-experience",
        says: "A skill with experience in it is untrained once the player has earned that much again: \
               until then the repository names it with the experience to earn; then the skill is \
               forgotten, its experience given back to spend and its credits back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-UNTRAIN-PENDING"),
        station: "yalain-testkit::cpu::play::untraining::a_skill_with_experience_in_it_is_untrained_once_that_much_is_earned_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.untrain-at-once",
        says: "A skill with no experience in it is untrained at once: the player is told, the skill \
               forgotten and its credits back in the repository.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-UNTRAIN-NOW"),
        station: "yalain-testkit::cpu::play::untraining::a_skill_with_no_experience_in_it_is_untrained_at_once_and_its_credits_come_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.untrain-cancelled",
        says: "An untraining under way can be cancelled: the repository names none, the player is \
               told, and the skill is kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-UNTRAIN-CANCEL"),
        station: "yalain-testkit::cpu::play::untraining::an_untraining_under_way_cancelled_keeps_the_skill",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.skills.used-skills-read-from-their-definitions",
        says: "Each used skill's definition gives how it is used: the distances it reaches, its \
               waits, its vigor, the implements it is used with, what it plays and its hooks \
               (Deadeye: a missile from 2 to 80 meters with a bow, every 2 seconds, for 32 vigor).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SKILL-USAGE"),
        station: "yalain-testkit::dat::skills::the_starting_skills_read_with_how_they_are_used",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.social.aliases-kept-with-the-options",
        says: "An alias the player sets or removes is kept with its options, to come back at its next \
               entry; nothing is answered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ALIAS"),
        station: "yalain-testkit::cpu::play::own_requests::an_alias_set_is_kept_with_the_options_and_removed_again_unanswered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.social.calls-read-in-the-client-library",
        says: "Every call this server sends for the allegiance, fellowships, trades, books, the \
               story's scenes and the market reads back in the client's own library: the values \
               carry the tags of the handler's declared parameters and every script object is of \
               its package's size.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SOCIAL-LIBRARY"),
        station: "yalain-testkit::corpus::social::every_call_of_a_social_day_reads_back_against_the_client_library",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.social.follow-starts-and-stops",
        says: "A player asking to follow an object is told \"Started following.\" and given one standing \
               order to keep within a meter of it, never sent again as it moves; following again first \
               stops the old follow (\"Stopped following.\", the order stopped), and an attack or a use stops \
               it too.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-FOLLOW"),
        station: "yalain-testkit::cpu::play::follow::a_player_follows_another_until_it_follows_again_or_uses_something",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.social.recorded-families-by-shape",
        says: "The recorded 2017 families of these answers (an oath taken on each side, fellows \
               joining and leaving, a fellowship left or disbanded, a trade's items exchanged, a \
               story shard's scene, a book read, a broker's catalog, a consignment put up) hold \
               the shapes this server sends, most of each in this server's order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SOCIAL-FAMILIES"),
        station: "yalain-testkit::corpus::social::the_recorded_social_families_hold_the_shapes_this_server_sends",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.social.recorded-requests-read",
        says: "Every request the recorded 2017 clients sent to make a fellowship, recruit, take a \
               place, swear, take an oath, ask for a trade, offer an item and accept a trade reads \
               in this server, a trade's acceptance with what the trade held.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SOCIAL-REQUESTS"),
        station: "yalain-testkit::corpus::social::the_recorded_clients_requests_read_in_this_server",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.social.using-another-player-is-refused",
        says: "Using another player is refused at once as unusable, with the distance between them, \
               and the other is told nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-USE-PLAYER"),
        station: "yalain-testkit::cpu::play::own_requests::using_another_player_is_refused_and_the_other_is_told_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.combat-speed-read",
        says: "A skill's definition may say how much faster it goes; three do.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-COMBAT-SPEED-READ"),
        station: "yalain-testkit::dat::skills::only_three_skills_say_they_go_faster",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.specials.combat-speed-squared",
        says: "A skill whose definition says it goes faster is told at the square of that over the \
               player's time scale, and what comes next follows its animation at that scale.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-COMBAT-SPEED-SQUARED"),
        station: "yalain-testkit::cpu::play::specials::a_skill_that_says_it_goes_faster_is_told_at_the_square_of_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.critical-by-weapon-only",
        says: "A special is critical only by what its weapon adds to the chance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPECIAL-CRITICAL"),
        station: "yalain-testkit::cpu::play::specials::a_specials_critical_comes_only_from_its_weapon",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.critical-size",
        says: "A special's critical does a quarter more than its plain hit, not double. Our choice: \
               fitted to 173 recorded criticals; other forms fit about as well.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-SPECIAL-CRITICAL-SIZE"),
        station: "yalain-testkit::cpu::play::specials::a_specials_critical_does_a_quarter_more_than_its_plain_hit",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.cut-by-a-transmute",
        says: "A transmute cuts short a skill's use, told or not; using an item or going into portal \
               space cuts nothing by itself.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CUT-ITEM-TRIP"),
        station: "yalain-testkit::cpu::play::specials::a_transmute_cuts_a_special_short_and_an_item_used_does_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.cut-short",
        says: "A use not made on the move is cut short by moving, by stopping all attacks, or by \
               stopping attacks on its target; after its story it counts as used (the skill's \
               information first), its behavior stops on the player and what it wields, a player \
               turned is stopped; a missile already thrown still arrives.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CUT-SHORT"),
        station: "yalain-testkit::cpu::play::specials::moving_after_a_specials_story_cuts_it_short_and_it_counts_as_used",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.from-rest",
        says: "A special used from rest raises the attack state, puts the player in its weapon's \n               combat mode and turns it to its target, pays its vigor (the skill's and the \n               weapon's), tells the special's story with the use's number, then at the animation's \n               end sends the skill's information, lowers the attack state, stops the player and \n               puts it at peace; a missile's outcome is told by its target as it arrives.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPECIAL-REST"),
        station: "yalain-testkit::cpu::play::specials::deadeye_from_rest_shoots_and_ends_with_the_skills_information",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.hook-adds-by-rating",
        says: "A hook with points adds to its hit by the player's rating in the skill, along the \
               lines between its points, rounded down.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HOOK-ADDS"),
        station: "yalain-testkit::dat::effects::a_hooks_points_read_in_full_and_add_along_their_lines",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.specials.hook-of-the-left-hand",
        says: "A special whose hook strikes with the left hand does the damage of what the left hand \
               holds, a shield's too; the right hand's weapon if the left holds nothing with damage.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPECIAL-LEFT-HAND"),
        station: "yalain-testkit::cpu::play::specials::a_hook_of_the_left_hand_does_the_damage_of_what_it_holds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.inside-a-run",
        says: "Inside an attack run a special waits for the run's next swing and takes it; the \n               run's basic swings go on after it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPECIAL-RUN"),
        station: "yalain-testkit::cpu::play::specials::riposte_inside_a_run_takes_one_swing_and_the_run_goes_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.one-waits",
        says: "While a special runs one request waits; a newer request replaces it and the replaced \n               one is told over with no reason.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPECIAL-WAITS"),
        station: "yalain-testkit::cpu::play::specials::a_newer_request_replaces_the_one_waiting",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.opening-exploited",
        says: "A creature that attacks is open a moment later for seven and a half seconds (the \n               golden rays); a starting special that hits it then exploits the opening: the \n               player's private effect, the rays stopped, the creature's story of 20 more damage.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPECIAL-OPENING"),
        station: "yalain-testkit::cpu::play::specials::a_starting_special_exploits_a_creatures_opening",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.range-between-bodies",
        says: "A skill's range is measured between the bodies, each a cylinder of its look's \
               radius and height: too far beyond its greatest range, too close within its least.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RANGE-BODIES"),
        station: "yalain-testkit::cpu::play::specials::a_skills_range_is_measured_between_the_bodies",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.recovery",
        says: "A special used again before its recovery has passed since its last use's end is \n               refused as not reset; after it, it is used.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPECIAL-RECOVERY"),
        station: "yalain-testkit::cpu::play::specials::a_special_waits_its_recovery_from_its_last_use",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.refused",
        says: "A refused skill's use is answered in one tick: the attack state raised (unless it \n               is), the use over, why, and the attack state lowered; the checks, first failure \n               first: dead, not wielding the right item, no vigor, not reset, vigor short of the \n               cost, no target or a dead one, out of range, moving. The recall needs something in \n               the right hand and all of the player's vigor.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SPECIAL-REFUSED"),
        station: "yalain-testkit::cpu::play::specials::a_special_or_the_recall_with_an_empty_hand_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.vigor-lowered-by-effects",
        says: "A player's effects can lower what its skills cost, a fraction rounded up, down to half \
               the skill's own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-VIGOR-LOWERED"),
        station: "yalain-testkit::cpu::play::specials::an_effect_lowers_what_a_skill_costs_down_to_half_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.vigor-of-what-is-held",
        says: "A skill costs its own, what its weapons cost and what its shield costs, each by the \
               item's own number or its template's, unless the skill ignores it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-VIGOR-OF-HELD"),
        station: "yalain-testkit::cpu::play::holds::a_use_pays_for_each_weapon_and_the_shield_by_their_own_numbers",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.specials.vigor-of-what-is-held-under-an-aura",
        says: "What a player's auras multiply a held item's cost by comes off that item, rounded \
               down.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-VIGOR-HELD-AURA"),
        station: "yalain-testkit::cpu::play::specials::an_aura_on_what_is_held_lowers_its_cost_rounded_down",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.store.a-sale-the-world-cannot-make-is-not-paid-for",
        says: "A sale whose template the world doesn't hold, or whose acquiring does something \
               other than move a quest, which the world can't do, is refused, not found: nothing \
               is paid and nothing made. Our choice.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-UNMAKEABLE"),
        station: "yalain-testkit::cpu::play::trade::a_purchase_of_what_the_world_cannot_make_is_refused_and_nothing_is_paid",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.store.greeting-and-store",
        says: "Using a storekeeper in reach is answered, then the storekeeper greets the player \
               by name and opens its store on its first page.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-OPEN"),
        station: "yalain-testkit::cpu::play::trade::a_storekeeper_used_greets_the_player_and_opens_its_store",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.store.no-room-refuses-the-purchase",
        says: "Buying with no room in the pack is refused for want of room and the page sent again; \
               nothing is paid and nothing made.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-NO-ROOM"),
        station: "yalain-testkit::cpu::play::trade::a_purchase_into_a_full_pack_is_refused_and_nothing_is_paid",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.store.purchase",
        says: "Buying a sale with enough money takes its price from the player's money and makes \
               the item in the player's pack.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-BUY"),
        station: "yalain-testkit::cpu::play::trade::a_purchase_takes_its_price_and_makes_the_item_in_the_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.store.purchase-within-a-stack-and-in-reach",
        says: "A purchase of more than a stack, or from beyond the storekeeper's reach, \
               finds no such sale (2017: 23 purchases of 1 to 300, each a stack or less).",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-LIMITS"),
        station: "yalain-testkit::cpu::play::trade::a_purchase_of_too_many_or_from_afar_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.store.sale-without-icon-layers",
        says: "A store page lists each sale as its profile, its icon when it has layers, and its \
               name; a sale without layers has no icon and names its product's icon instead, and \
               every sale's references point at its own objects.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-ICONLESS"),
        station: "yalain-testkit::cpu::play::trade::a_sale_without_icon_layers_keeps_every_sales_references_on_its_own_objects",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.store.sales-for-others-from-the-install",
        says: "Who each of a store's sales is for (a race, a least level, quests at a status) and \
               whether the store hides the sales a player may not have are read from the install's \
               store listing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-GATED-INSTALL"),
        station: "yalain-testkit::corpus::world_db::who_a_stores_sales_are_for_is_read_from_the_install",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.store.sales-for-others-hidden",
        says: "A store that hides what a player may not have leaves off its page the sales of \
               another race, a higher level or a quest the player does not hold at its status, and \
               still counts them in its size.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-GATED"),
        station: "yalain-testkit::cpu::play::trade::a_store_that_hides_sales_for_others_leaves_them_off_the_page_but_counts_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.store.sales-for-others-restricted",
        says: "A store that does not hide them shows the sales a player may not have restricted, \
               and buying one finds no sale and takes nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-GATED-SHOWN"),
        station: "yalain-testkit::cpu::play::trade::a_store_shows_sales_for_others_restricted_and_will_not_sell_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.store.vendor-left-past-eight-meters",
        says: "A shopper who walks more than eight meters from a vendor is told it left the store's \
               catalog, and the store is no longer sent again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-WALK-OFF"),
        station: "yalain-testkit::cpu::play::trade::a_shopper_walking_off_from_a_vendor_leaves_its_catalog",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.store.vendor-store-sent-again-while-shopping",
        says: "While a shopper stays in a vendor's store, the store's size is sent again a few \
               seconds after entering and about every half minute after; leaving the store ends it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-REFRESH"),
        station: "yalain-testkit::cpu::play::trade::a_vendors_store_is_sent_again_every_half_minute_until_the_shopper_leaves_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.story.shard-bestows-its-scene",
        says: "Using a story shard plays the shard's behavior with the camera on it; ten seconds \
               later the scene is added to the player's story, the player frozen, protected, \
               played the scene (the whole act when it completes the act's seven) and taken into \
               portal space, coming out at the shard's destination; once the scenes are done the \
               protection ends three and a half seconds later; the scene is in the act registry at \
               the next entry.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORY-SHARD"),
        station: "yalain-testkit::cpu::play::story::a_story_shard_bestows_its_scene_and_takes_the_player_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.story.shard-completes-its-quest",
        says: "A vault's shard that has a quest completes it as the player comes out, before its use is answered, every time it is used.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SHARD-QUESTS"),
        station: "yalain-testkit::cpu::play::story::a_vaults_shard_completes_its_quest_as_the_player_comes_out",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.story.shard-names-its-vaults-quest",
        says: "Every vault's shard names the hidden quest of its vault, with that quest's setup's pay and points.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WDB-SHARD-QUESTS-2017"),
        station: "yalain-testkit::corpus::world_db::a_vaults_shard_completes_the_quest_of_its_vault",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.story.starting-scenes",
        says: "A new character holds the story's starting scenes, each added as it first enters, \
               after its session start.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORY-START"),
        station: "yalain-testkit::cpu::play::story::a_new_character_is_given_the_starting_scenes_as_it_first_enters",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.talk.a-reminder-is-said-with-a-quest-under-way",
        says: "A person's line asking why the player is still there is said with one of its quests under way, and never to a player with none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-WDB-QUEST-REMINDERS-2017"),
        station: "yalain-testkit::corpus::world_db::a_reminder_line_is_said_with_a_quest_under_way_and_not_without",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.talk.a-reminder-may-name-its-phase",
        says: "A reminder naming a phase is said at that phase of its quest only.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REMIND-PHASE"),
        station: "yalain-testkit::cpu::play::quest_uses::a_reminder_naming_a_phase_is_said_only_at_that_phase",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.talk.broadcast-names-the-player",
        says: "A broadcast a player's act brought on carries that player's name as its variable, \
               the same to everyone who hears it; one said of itself carries none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BROADCAST-NAMES"),
        station: "yalain-testkit::cpu::play::talk::a_broadcast_a_player_brought_on_names_that_player_to_all_who_hear_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.talk.fighting-and-dying",
        says: "A creature with lines for a fight says one as it goes after a player, and one with \
               lines for its death says one as it dies, each the way its line is said.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TALK-FIGHT"),
        station: "yalain-testkit::cpu::play::talk::a_creature_says_a_fighting_line_as_it_goes_after_a_player_and_a_dying_line_as_it_dies",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.talk.idle-broadcast",
        says: "A creature whose idle lines are broadcast says one every 20 to 50 s whether a \
               player is near or not, as a local broadcast of kind 256 from the creature that \
               every player in its landblock and the eight around it reads, and no player \
               farther off.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TALK-BROADCAST"),
        station: "yalain-testkit::cpu::play::talk::an_idle_broadcast_reaches_the_speakers_landblock_and_the_eight_around_it_every_twenty_to_fifty_seconds",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.talk.literal-sayings-and-emotes",
        says: "A line written as new prose goes as literal text, which the client shows as it is; \
               a line said aloud reaches every client that knows the speaker, on the speaker; an \
               emote is chat-window text of the kind that names its speaker.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TALK-LITERAL"),
        station: "yalain-testkit::cpu::play::talk::new_prose_goes_as_literal_text_and_a_line_can_be_said_aloud_or_emoted",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.talk.near-and-idle",
        says: "A person with lines for a player near greets a player within 10 m once, not again \
               within 5 minutes; one with idle lines says one every 60 to 120 s while a player is \
               within 20 m (the reach and the waits authored).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TALK-NEAR"),
        station: "yalain-testkit::cpu::play::talk::a_person_greets_a_player_come_near_once_and_chatters_while_one_is_near",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.talk.too-soon-is-kept-from-a-plain-use",
        says: "A person's too-soon line is said only when it is too soon, never at a plain use.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TOO-SOON-USE"),
        station: "yalain-testkit::cpu::play::quest_uses::a_persons_too_soon_line_is_not_said_at_a_plain_use",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.talk.used-with-nothing-to-give",
        says: "A person used with no quest step for the player says a reminder of one of its \
               quests the player has under way, else one of its lines for a use, picked at \
               random, the lines heard said before those only derived from their names.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TALK-USE"),
        station: "yalain-testkit::cpu::play::talk::a_person_used_with_nothing_for_the_player_says_one_of_its_lines_or_a_reminder",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.talk.with-a-quest-step",
        says: "A step a person's use runs says the step's line, then the lines heard said with \
               that update of that quest, in their order, the step's line not twice; a line only \
               derived from its name is said only when none was heard.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TALK-STEP"),
        station: "yalain-testkit::cpu::play::talk::a_step_says_its_line_then_the_lines_heard_with_its_update_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.trade.an-attuned-item-is-refused",
        says: "An item bound to its holder among a trade's offers stops the trade once both accept: \
               its move refused, the trade reset, nothing moved, the trade still open.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TRADE-ATTUNED"),
        station: "yalain-testkit::cpu::play::trading::an_attuned_item_offered_fails_the_trade_and_nothing_moves",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.trade.an-offer-no-longer-held-moves-nothing",
        says: "An offer no longer held in full when both have accepted (put down, sold, \
               split off) is taken back, told to both, and both must accept again: nothing \
               moves, and neither side gives anything for it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TRADE-STALE"),
        station: "yalain-testkit::cpu::play::integrity::an_offer_dropped_after_accepting_moves_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.trade.asked-and-opened",
        says: "Asking a player beside for a trade tells both of it, each with the other's name; \
               the other asking back opens it for both.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TRADE-OPEN"),
        station: "yalain-testkit::cpu::play::trading::a_trade_asked_is_told_to_both_and_asking_back_opens_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.trade.closed",
        says: "Closing a trade tells both who closed it; walking apart closes it as too far; \
               leaving the world closes it for the other.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TRADE-CLOSE"),
        station: "yalain-testkit::cpu::play::trading::a_trade_is_closed_by_either_side_by_distance_and_by_leaving",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.trade.kept-whole-across-a-crash",
        says: "A trade is one transaction with everything else a pass of the world did: a \
               server dying at any point of its save leaves the trade done on both sides or \
               on neither, and the database opens cleanly.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-TRADE-WHOLE"),
        station: "yalain-testkit::cpu::operator::store_faults::a_server_dying_at_any_point_of_a_trades_save_keeps_it_whole",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.trade.no-room-fails-the-trade",
        says: "When both have accepted and a side's pack has no room for what it would receive, \
               both are told the trade failed and which side lacks room; nothing moves and the \
               trade stays open.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TRADE-NO-ROOM"),
        station: "yalain-testkit::cpu::play::trading::a_pack_without_room_for_the_offer_fails_the_trade_and_nothing_moves",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.trade.offered-accepted-exchanged",
        says: "An offer is told to both and the other side made aware of the item; offering it \
               again is refused; a changed offer clears both acceptances; with both accepting, \
               each item (or part of a stack, as a new item) moves to the front of the other's \
               pack, the move told to both, the trade reset and both told it finished, the trade \
               staying open.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TRADE-EXCHANGE"),
        station: "yalain-testkit::cpu::play::trading::both_accepting_moves_the_offered_item_to_the_front_of_the_others_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.archive-for-this-target",
        says: "A release's archive is the one its release.json lists for the target this build  \
               was made for, whichever comes first; a release with none for it is refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-ARCHIVE"),
        station: "yalain::bin.yalain::update::tests::the_archive_taken_is_the_one_for_this_builds_target",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.backup-and-way-back",
        says: "A release's install backs the database up first; when the new server fails its  \
               health check, the old program and the database with its rows come back, and the  \
               release is recorded as failed and not tried again.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-WAY-BACK"),
        station: "yalain::bin.yalain::update::tests::a_release_that_fails_its_health_check_is_rolled_back_with_its_database_and_not_retried",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.bad-checksum-refused",
        says: "A download whose SHA-256 is not the one SHA256SUMS lists is refused: nothing is  \
               installed and the new program never runs.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-CHECKSUM"),
        station: "yalain::bin.yalain::update::tests::a_checksum_that_does_not_match_refuses_the_release_and_installs_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.cancelled-with-the-shutdown",
        says: "An operator who cancels the shutdown cancels the update, and the next check  \
               stages it again.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-CANCEL"),
        station: "yalain::bin.yalain::update::tests::a_cancelled_countdown_cancels_the_update_and_the_next_check_stages_it_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.check-by-hand",
        says: "yalain update --check, needing no install, reads the source its configuration  \
               names and says what it would do; without --check or --apply it prints its usage  \
               and exits with 2.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-CHECK-BY-HAND"),
        station: "yalain::live::stopping::update_check_reads_the_source_the_configuration_names_and_says_what_it_would_do",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.update.command-line",
        says: "yalain update takes --check or --apply, --config and --policy, and refuses  \
               anything else in Empyrean's words.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-COMMAND"),
        station: "yalain::bin.yalain::update::tests::the_update_command_takes_check_or_apply_and_a_config_and_a_policy",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.database-backup-and-restore",
        says: "The backup before an update is a sibling of the database named for the update,  \
               the newest three kept, and putting it back removes what the file became and its  \
               write-ahead log.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-BACKUP"),
        station: "yalain-store::cpu::persistence::an_updates_backup_is_named_for_it_and_puts_the_database_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.needs-a-step-or-a-hand-refused",
        says: "A release that cannot be upgraded to straight from this version, or that needs a  \
               configuration key set by hand, is reported and not taken.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-REFUSED"),
        station: "yalain::bin.yalain::update::tests::a_release_that_cannot_upgrade_straight_or_needs_a_key_set_by_hand_is_not_taken",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.newer-release-installed",
        says: "A newer release on the source is found, checked against SHA256SUMS and  \
               release.json, staged, its program run once, and installed with the old files  \
               moved aside and the database backed up first, then health-checked.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-INSTALL"),
        station: "yalain::bin.yalain::update::tests::a_newer_release_is_found_checked_installed_and_health_checked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.not-what-it-declares-refused",
        says: "A release whose own program reports another database schema or world database  \
               than its release declares is refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-DECLARES"),
        station: "yalain::bin.yalain::update::tests::a_release_that_is_not_what_it_declares_is_refused",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.off-patch-minor",
        says: "server.update off installs nothing, patch takes patch releases that change  \
               nothing but the program (a patch of the world database is fetched), minor also  \
               takes minor releases with their migrations; by hand with neither set means  \
               minor.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-POLICY"),
        station: "yalain::bin.yalain::update::tests::off_patch_and_minor_take_what_they_take_on_empyrean",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.own-world-needs-no-fetch",
        says: "A world serving a folder of its own does not use the world database a release pins, so \
               its update fetches none and is not refused when none can be fetched; one lying over a \
               base of no named version does use it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-OWN-WORLD"),
        station: "yalain::bin.yalain::update::tests::a_world_serving_its_own_folder_needs_no_fetch_of_the_pinned_world_database",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.players-warned-by-the-shutdown-countdown",
        says: "The countdown an update warns players with shows every player in the world its  \
               own length at once, then the shutdown's lines, and the server is done twenty  \
               seconds after the last.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-WARNING-LINES"),
        station: "yalain-testkit::cpu::operator::shutdown::an_updates_warning_counts_down_its_own_seconds_to_every_player",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.players-warned-first",
        says: "The running server stages a release with the world running, then counts down  \
               server.update_warning_seconds; nothing is installed until the world has ended by  \
               itself, and a server stopped from outside installs nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-WARNED"),
        station: "yalain::bin.yalain::update::tests::the_running_server_warns_players_first_and_installs_only_after_the_world_ends_by_itself",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.reads-its-own-tags",
        says: "Of a repository holding the releases of many programs, only the tags beginning  \
               yalain-v are read.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-TAGS"),
        station: "yalain::bin.yalain::update::tests::only_yalains_tags_are_read_from_a_repository_of_many_programs",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.reads-what-packaging-writes",
        says: "The release.json the packaging writes and the facts --release-facts prints are  \
               what the updater reads.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-INDEX"),
        station: "yalain::bin.yalain::update::tests::the_release_json_the_packaging_writes_and_the_facts_the_program_prints_are_read",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.running-server-restarts-into-the-release",
        says: "A running server taken for an older version counts down, stops, installs the  \
               release, passes its health check on the real install and restarts into it in the  \
               same process. Its status address is free for the health check and then for the new  \
               server, which answers there.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-RESTART"),
        station: "yalain::live::stopping::the_running_server_counts_down_installs_and_restarts_into_a_patch_release",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.update.says-what-it-would-do",
        says: "The check says, in Empyrean's words with Yalain's name, what it would install  \
               and why it would not install each other release.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-DESCRIBE"),
        station: "yalain::bin.yalain::update::tests::the_check_says_what_it_would_install_and_why_it_would_not",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.settings",
        says: "server.update, server.update_check_hours, server.update_warning_seconds and  \
               server.update_source are read with Empyrean's defaults (off, 6, 300, empty),  \
               without a warning.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-KEYS"),
        station: "yalain::bin.yalain::update::tests::the_update_keys_are_read_with_empyreans_defaults",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.source",
        says: "New releases are looked for in the repository the build came from, a GitHub  \
               repository named instead, or a mirror of the releases API; anything else is  \
               refused.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-SOURCE"),
        station: "yalain::bin.yalain::update::tests::the_release_source_is_the_builds_repository_or_a_configured_mirror",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.update.world-database-fetched-first",
        says: "A release serving a newer world database has it fetched by its own program while  \
               it is staged, before anything is installed and before the new program starts; a  \
               fetch that fails refuses the release, and a release serving the same one fetches  \
               nothing.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-UPDATE-FETCH"),
        station: "yalain::bin.yalain::update::tests::a_newer_world_database_is_fetched_by_the_new_program_before_anything_is_installed",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.attack-speed-effects",
        says: "A player's attack speed effects, its class's top one and what adds on top, set the time \
               scale of its swings, shots and specials, and so when they land and when the next \
               comes, the pause after each unchanged; a hold brings them to a fifth.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PLAYER-ATTACK-PACE"),
        station: "yalain-testkit::cpu::play::attack_speed::a_players_attack_speed_effects_quicken_its_swings",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.attack-speed-slows",
        says: "An effect slowing a player's attacks slows them by its whole size, as one quickening them \
               does. Our choice: no recording holds a swing of a slowed player.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PLAYER-ATTACK-SLOW"),
        station: "yalain-testkit::cpu::play::attack_speed::an_effect_slowing_a_players_attacks_slows_its_swings",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.basic-attacks-go-by-their-effects",
        says: "A run swings only the basic attacks whose required effects its player carries, all of \
               them, and whose barring effects it carries none of.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BASIC-BY-EFFECTS"),
        station: "yalain-testkit::cpu::play::attack_speed::a_run_swings_only_what_its_players_effects_allow",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.basic-attacks-read-from-their-definitions",
        says: "A player's basic attacks are the used skills with weights to be drawn by; each read \n               with its ranges and the implements it is used with.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AUTO-ATTACK-DEFS"),
        station: "yalain-testkit::dat::effects::the_basic_attacks_read_as_the_data_defines_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.weapons.basic-attacks-read-what-they-want",
        says: "A used skill's definition names the effects required, the effects barring it and the \
               skills to be trained, each read with it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BASIC-READ-WANTS"),
        station: "yalain-testkit::dat::skills::an_alchemists_basic_attacks_name_what_they_want_and_what_bars_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.weapons.basic-attacks-want-their-skills",
        says: "A run swings no basic attack whose named skills its player has not trained at the \
               ratings named.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BASIC-WANTS-SKILLS"),
        station: "yalain-testkit::cpu::play::attack_speed::a_basic_attack_waits_for_the_skills_it_names",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.bow-made-strung",
        says: "A bow is made strung however it came: its own mode is the bow's; nothing else in a \
               hand is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BOW-STRUNG"),
        station: "yalain-testkit::cpu::play::holds::a_looted_bow_is_made_strung",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.bow-shoots",
        says: "A bow's run stands: the player stopped and turned to its target before each swing; \
               beyond a meter between the bodies it shoots (its story names the bow, which plays \
               the behavior, and the target tells each arrival), within one it strikes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WEAPON-ATTACKS"),
        station: "yalain-testkit::cpu::play::weapons::a_bow_shoots_from_afar_and_its_target_tells_each_arrival",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.hold-from-template",
        says: "A thing in a hand is held as its template says: the hands it may go to and prefers, \
               those it rules out, where it hangs; a weapon answered as wielded, anything else as \
               worn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HOLD-FROM-TEMPLATE"),
        station: "yalain-testkit::cpu::play::holds::a_looted_bow_takes_both_hands_and_hangs_as_its_template_says",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.hold-read",
        says: "How a thing is held reads from its template's qualities.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-HOLD-READ"),
        station: "yalain-testkit::dat::effects::a_things_hold_reads_from_its_template",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.weapons.hook-adds-by-rating",
        says: "A basic attack's hook with points adds by the player's rating in it, on top of the \
               weapon's damage, doubled by a critical with the rest. Our choice: specials measure \
               this; of basic attacks only 13 swings of one player could show it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-BASIC-HOOK-ADDS"),
        station: "yalain-testkit::cpu::play::two_blows::a_basic_attacks_hook_adds_by_the_players_rating",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.immolations-attacks",
        says: "An alchemist under Immolation swings the fiery basic attacks in place of its plain ones, \
               rated by what the buff raises. Read from the skills' and effects' definitions; no \
               recording holds a use of it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-BASIC-IMMOLATION"),
        station: "yalain-testkit::dat::effects::immolation_turns_an_alchemists_basic_attacks_fiery",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.weapons.implement-by-class",
        says: "What a player wields is known by its class: a weapon of a known class draws from \
               the basic attacks its kind allows in both hands; of no known class it swings the \
               plain ones as a sword's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-IMPLEMENT-BY-CLASS"),
        station: "yalain-testkit::cpu::play::two_blows::a_weapon_known_by_its_class_fights_as_its_kind",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.implement-by-class-read",
        says: "A weapon's implement kind is what its class says.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-IMPLEMENT-CLASSES"),
        station: "yalain-testkit::dat::effects::a_weapons_implement_is_what_its_class_says",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.weapons.mode-by-implement",
        says: "A player's combat mode is what it wields: by the right hand's implement, and for \
               a one-handed weapon by the left's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MODE-BY-IMPLEMENT"),
        station: "yalain-testkit::cpu::play::two_blows::a_players_combat_mode_comes_from_what_it_wields",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.mode-by-stance",
        says: "A player's combat mode is the stance its right hand's template names for what the \
               left holds: alone, with a shield, or with a second weapon.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-MODE-BY-STANCE"),
        station: "yalain-testkit::cpu::play::holds::the_combat_mode_is_the_stance_the_right_hands_template_names_for_the_left",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.off-hand-takes-no-stamp",
        says: "A weapon going to or from the off hand takes no stamp of its holder's, as only the \
               primary hand's moves the slider that carries one, so what the holder does next is \
               the next stamp a client sees.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-OFF-HAND-STAMP"),
        station: "yalain-testkit::cpu::play::paper_doll::a_weapon_in_the_off_hand_takes_no_stamp_on_or_off",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.standing-run-refusals",
        says: "A run that stands is told it is too far beyond every basic attack's range, and that \
               it cannot shoot while moving, about a second apart, its combat mode coming with its \
               first swing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STANDING-RUNS"),
        station: "yalain-testkit::cpu::play::weapons::a_bow_run_too_far_or_moving_is_told_so_each_second",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.stave-near-and-far",
        says: "A stave strikes within ten meters between the bodies and throws beyond nine, its \
               stories' second hook the strike or the throw naming the stave.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STAVE-ATTACKS"),
        station: "yalain-testkit::cpu::play::weapons::a_stave_strikes_near_and_throws_far",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.swing-when-due",
        says: "A player's swing comes as soon as it is due, not at the world's next step.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SWING-WHEN-DUE"),
        station: "yalain-testkit::cpu::play::two_blows::a_swing_comes_when_it_is_due_not_at_the_worlds_next_step",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.time-scale-bounds-of-delay",
        says: "A player's time scale before its effects stays between 1 and 1.13, whatever it wears. \
               Our choice: no recording holds a set of under 46 or over 96 combat delay.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-PLAYER-DELAY-BOUNDS"),
        station: "yalain-testkit::cpu::play::attack_speed::the_time_scale_of_delay_stays_within_what_was_seen",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.time-scale-follows-delay",
        says: "A player's time scale before its effects falls with the combat delay of all it wears \
               and wields, by each item's own number, and its swings with it: 1.23 less 0.0024 a \
               point. What an aura holds on a weapon changes the number shown, not its holder's \
               time scale.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-PLAYER-DELAY-PACE"),
        station: "yalain-testkit::cpu::play::attack_speed::a_players_swings_slow_with_the_combat_delay_it_wears_and_wields",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.two-blows",
        says: "A basic attack with two blows tells a result for each: drawn apart, doing the \
               damage of the weapon in the hand its hook names, each landing at its own time; \
               its next swing comes after its own period.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TWO-BLOWS"),
        station: "yalain-testkit::cpu::play::two_blows::a_skill_that_strikes_twice_tells_a_result_for_each_blow",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.two-blows-read",
        says: "The skills that strike twice read with two hooks that strike, each naming its hand.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TWO-BLOWS-READ"),
        station: "yalain-testkit::dat::effects::the_skills_that_strike_twice_read_with_each_blows_hand",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.weapons.two-throws",
        says: "A skill that throws twice names each hand's weapon; each throw is drawn apart, with \
               its own damage, and its target tells each as it arrives.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TWO-THROWS"),
        station: "yalain-testkit::cpu::play::two_blows::a_skill_that_throws_twice_names_each_hands_weapon_and_tells_each_arrival",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.two-throws-read",
        says: "The orbs' three basic attacks read with two thrown hooks, a hand each.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TWO-THROWS-READ"),
        station: "yalain-testkit::dat::effects::the_skills_that_throw_twice_read_with_each_throws_hand",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "server.weapons.walking-run-walks-silently",
        says: "A run whose attacks walk, on a target out of reach, has one order to walk and is told \
               nothing until it arrives, when it swings; it goes on until the player stops it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WALKING-RUN"),
        station: "yalain-testkit::cpu::play::weapons::a_sword_run_out_of_reach_is_ordered_once_and_told_nothing_until_it_arrives",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.withheld-run-waits",
        says: "A run with no basic attack it may swing waits without a word and swings when it may; a \
               special waiting for a swing still goes. Our choice: three records of an effect barring \
               a player's attacks are held, none with a run under it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-BASIC-WITHHELD"),
        station: "yalain-testkit::cpu::play::attack_speed::a_run_with_nothing_it_may_swing_waits",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.weapons.worn-is-not-wielded",
        says: "What a player wears is not what it wields: its weapon's attacks, mode and damage \
               are its hands', whatever it wears and in whatever order it came by them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORN-NOT-WIELDED"),
        station: "yalain-testkit::cpu::play::two_blows::what_is_worn_does_not_take_the_place_of_what_is_wielded",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world-db.release-notes-from-the-change-log",
        says: "A world database release's notes are its change log's sections from after the \
               last release up to the release, newest first, with the archive's SHA-256; a \
               release the change log has no section for is not made.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WDB-RELEASE-NOTES"),
        station: "yalain-world-db::cpu::release::the_notes_are_the_change_log_s_sections_since_the_last_release_newest_first",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world-db.release-the-same-bytes-for-the-same-files",
        says: "A world database release built twice from the same files is the same archive, byte \
               for byte: its entries in path order, stored, with one fixed date, so anyone can \
               rebuild a release and check its SHA-256.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-WDB-RELEASE-REPRO"),
        station: "yalain-world-db::cpu::release::a_release_built_twice_from_the_same_files_is_the_same_bytes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.a-client-gone-leaves-its-character-lingering",
        says: "A character whose client went away without logging off lingers, glowing, and leaves \
               at its leaving behavior about fifteen seconds on, without the trip into portal \
               space.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOG-DROPPED"),
        station: "yalain-testkit::cpu::entry::logging_in_again::a_character_whose_client_said_goodbye_lingers_and_leaves_at_its_leaving_behavior",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.a-save-is-whole-or-not-at-all",
        says: "A character's save, and a purge, is kept whole or not at all: one that fails part \
               way leaves the save before it whole, as Empyrean keeps each batch of saves in one \
               transaction.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-WHOLE-SAVES"),
        station: "yalain-store::cpu::persistence::a_save_or_purge_that_fails_part_way_leaves_the_one_before_it_whole",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.a-wielded-weapon-is-kept-across-sessions",
        says: "A weapon wielded at a logoff is in hand at the next entry: the session start lists \
               it by its place and by itself (the hands it takes, its implement, an empty look), \
               and it is made with the player as its holder, hung from the hand.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KEEP-WIELDED"),
        station: "yalain-testkit::cpu::entry::keeping::a_bow_wielded_at_logoff_is_in_hand_again_at_the_next_entry",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.database-backed-up-before-a-schema-upgrade",
        says: "Before its schema is upgraded, a database file that holds anything is copied \
               beside itself as it was, named for the versions and the time, and the newest \
               three copies are kept, as Empyrean backs up before a migration; a file a newer \
               Yalain kept is refused and left as it was.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-BACKUPS"),
        station: "yalain-store::cpu::persistence::a_database_is_backed_up_as_it_was_before_its_schema_is_upgraded",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.database-foreign-keys-hold",
        says: "The database keeps no character without its account and no saved state or list \
               without its character, as Empyrean enforces its foreign keys; a purge goes, its \
               rows first.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-FOREIGN-KEYS"),
        station: "yalain-store::cpu::persistence::no_character_is_kept_without_its_account_nor_its_state_without_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.database-write-ahead-log",
        says: "The database is kept with a write-ahead log, as Empyrean keeps its own: a save is \
               in the file once made, readable by another connection while the server runs, and \
               there after a crash.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-STORE-WAL"),
        station: "yalain-store::cpu::persistence::the_database_is_kept_with_a_write_ahead_log_and_a_save_outlives_a_crash",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.entered-by-referral",
        says: "Entering the world with a character sends a referral to the world service's port \
               and server id; the world connection the client then opens belongs to that \
               character.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-REFERRAL"),
        station: "yalain-testkit::cpu::entry::character_select::entering_the_world_refers_the_client_to_the_world_service",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.entered-without-referral",
        says: "A server set to send no referral carries the world on the login connection: the \
               same entry, its private messages numbered from 1 at every entry, positions sent \
               back; logging off returns to the character list on that connection.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NO-REFERRAL"),
        station: "yalain-testkit::cpu::entry::world::without_a_referral_the_world_comes_on_the_login_connection",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.entry-in-the-recorded-order",
        says: "As a character enters, its world connection carries the player's object, the \
               message making it the client's player, its description, a new character's conflict \
               status, the arrival effect, a new character's movement ethereality, a freeze of its \
               movement, the end of the portal effect, the session start and the welcome, in that \
               order; private messages are numbered from 1 on the connection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORLD-ENTRY"),
        station: "yalain-testkit::cpu::entry::world::entering_sends_the_player_its_object_description_and_session_start_in_the_recorded_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.entry-refusals",
        says: "Entering with a character already in the world is refused with reason 11, and \
               with one not on the account with reason 13.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ENTRY-REFUSED"),
        station: "yalain-testkit::cpu::entry::character_select::a_character_in_the_world_or_not_on_the_account_cannot_enter",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.item-instance-stamp-counts-entries",
        says: "An item made in a session is made with instance stamp 0, and with one more each \
               time its holder enters again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KEEP-ITEM-STAMP"),
        station: "yalain-testkit::cpu::entry::keeping::an_items_instance_stamp_is_one_more_at_each_entry",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.left-to-the-character-list",
        says: "Leaving the world is answered with the character's exit, then the account's \
               characters by name and with their looks; the world connection ends.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LEAVE"),
        station: "yalain-testkit::cpu::entry::character_select::leaving_the_world_answers_the_exit_then_the_lists",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.log-off-lingers",
        says: "A player who logs off stays in the world about nineteen seconds, frozen and glowing, \
               plays its leaving near the end, then goes, and only then is back at the character \
               list.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LOGOFF-LINGER"),
        station: "yalain-testkit::cpu::entry::logging_off::a_player_logging_off_lingers_in_the_world_before_it_leaves",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.objects-around-the-player",
        says: "The objects of the world in the player's view are made for it as it enters: \
               indoors those of its own landblock in the cells its cell sees, outdoors those \
               within a landblock of it; those that come into view as it walks are made as they \
               do; a static object keeps its own id.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-OBJECTS-AROUND"),
        station: "yalain-testkit::cpu::entry::world::objects_in_range_are_made_as_the_player_enters_and_as_it_walks_into_their_landblock",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.one-character-of-an-account-at-a-time",
        says: "An account has one character in the world at a time: entering another is refused \
               with reason 11 until the first has left.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOG-ONE-PER-ACCOUNT"),
        station: "yalain-testkit::cpu::entry::logging_in_again::a_second_character_of_the_account_waits_until_the_first_has_left",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.options-kept",
        says: "The options the client sends (game options, window places and opacities, \
               shortcuts, chat settings) come back in the session start of the character's next \
               entry.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-OPTIONS"),
        station: "yalain-testkit::cpu::entry::world::options_the_client_sends_come_back_in_the_next_session_start",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.position-sent-back-and-kept",
        says: "The place the client reports for its player is sent back to it as the player's \
               position on the player's own position stream, each stamped newer than the last; \
               the character enters again where it left, facing as it faced.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-POSITION"),
        station: "yalain-testkit::cpu::entry::world::a_reported_place_is_sent_back_and_the_character_enters_there_next_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.scene-drawn-frees-movement",
        says: "When the client says it has drawn the world, the server frees the player's \
               movement and ends the arrival effect.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SCENE"),
        station: "yalain-testkit::cpu::entry::world::the_drawn_scene_frees_the_player_and_ends_the_arrival_effect",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.session-start-reads-in-the-client-library",
        says: "The session start the server builds reads back against the client's own script \
               library: its values carry the tags of the handler's declared parameters and each \
               script object its package's size.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-SESSION-START"),
        station: "yalain-testkit::corpus::content::the_session_start_reads_back_against_the_client_library",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.world.start-from-the-character-data",
        says: "A new character starts in its race's start area as the character generation data \
               places it (a human in one of the eighteen copies of the training hall) with its \
               race's starting skills.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-START"),
        station: "yalain-testkit::corpus::content::new_humans_start_in_a_copy_of_the_training_hall_with_their_races_skills",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "server.world.state-kept-between-sessions",
        says: "Where a character stands when it leaves the world, its qualities and its options are \
               kept, and it enters next time as it left, across the server stopping and starting \
               again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORLD-STATE"),
        station: "yalain-store::cpu::persistence::a_characters_world_state_outlives_reopening_the_database_and_goes_with_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.treasure-own-kept-across-sessions",
        says: "A treasure item's own least level and Arcane Lore stay with it across a logout \
               and are listed in its profile, and its holder's hero skill credits are kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-OWN-KEPT"),
        station: "yalain-testkit::cpu::entry::examine::a_treasure_items_own_requirements_and_the_hero_credits_are_kept_across_a_logout",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.treasure-value-kept-across-sessions",
        says: "A treasure item keeps the value it was drawn across a logout, so it is named and \
               valued as before on its next entry.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TREASURE-VALUE-KEPT"),
        station: "yalain-testkit::cpu::entry::examine::a_treasure_items_drawn_value_is_kept_across_a_logout",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.worn-armor-kept-across-sessions",
        says: "Armor worn at a logout is worn again at the next entry, and the character list and \
               other players show the character wearing it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KEEP-WORN"),
        station: "yalain-testkit::cpu::entry::keeping::a_chest_piece_worn_at_logoff_is_worn_again_and_seen_at_the_next_entry",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.worn-effects-kept-across-sessions",
        says: "A worn item's effects stay on across a logout, the same records at the next entry, \
               and a made item keeps its own bonus.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-WORN-KEPT"),
        station: "yalain-testkit::cpu::entry::keeping::a_worn_items_effect_and_its_own_bonus_are_kept_across_a_logout",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.world.worn-items-listed-as-2017-listed-them",
        says: "Every recorded session start lists the places its worn items fill together, and \
               each worn or wielded item in both of its inventory tables as this server builds \
               one: the places it takes, where it is, its implement, and its look (empty for a \
               weapon, the appearance it gives its wearer for armor).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-KEEP-WIELDED-TABLES"),
        station: "yalain-testkit::corpus::keeping::every_recorded_worn_or_wielded_item_is_listed_as_this_server_lists_it",
        tier: Tier::Corpus,
    },
];
