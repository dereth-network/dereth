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
        id: "server.auras.reach-fellows",
        says: "An aura's pulses reach its user's fellows within its range as they reach its user; a fellow \
               gone beyond it that holds an earlier pulse's record is told the next pulse missed it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AURA-FELLOWS"),
        station: "yalain-testkit::cpu::play::auras::an_auras_pulses_reach_fellows_within_its_range",
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
        id: "server.chat.messages-as-2017-sent-them",
        says: "The chat messages the server builds are those the 2017 servers sent: the empty \
               allegiance profile, a new character's channels, each emote's behavior, tell \
               confirmations and refusals, and the chat service's lines and answers, byte for byte \
               apart from the ids a packed graph numbers its entries with.",
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
        says: "A crude knife cuts 11 units from any creature, a hunting knife 16 from creatures below \
               level 17 and more from stronger ones, twice as many for an expert cut, which comes more \
               often the more the butcher's level passes the creature's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-BUTCHERY-UNITS"),
        station: "yalain-testkit::cpu::play::butchery::a_knife_cuts_its_units_by_the_creatures_level",
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
        id: "server.craft.new-character-registry",
        says: "A new character is told its craft skills, every one at level 1, then its whole \
               craft registry with the world's starting recipes; a returning one, its registry.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-CRAFT-REGISTRY"),
        station: "yalain-testkit::cpu::play::crafting::a_new_character_gets_every_craft_skill_and_the_starting_recipes",
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
        id: "server.craft.recipes-read-from-their-definitions",
        says: "Recipes and craft skills read from their saved states with the library's names: a \n               recipe's craft skill, animation and cycles, its ingredient slots, its checks and \n               their outcomes' actions (items made, ingredients used up, effects, texts), its \n               wait; a craft skill's cap.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-RECIPE-DEFS"),
        station: "yalain-testkit::dat::effects::the_recipes_read_as_the_data_defines_them",
        tier: Tier::Dat,
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
        id: "server.craft.tool-wear-kept-across-sessions",
        says: "A worn tool keeps its durability across a logout: it comes back as worn as it left.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-TOOL-WEAR-KEPT"),
        station: "yalain-testkit::cpu::play::butchery::a_worn_knife_keeps_its_durability_across_a_logout",
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
        id: "server.effects.records-stack",
        says: "The same effect put on twice is two records with ids counting on; neither replaces \
               nor refreshes the other, and each runs out at its own time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-EFFECTS-STACK"),
        station: "yalain-testkit::cpu::play::effects::the_same_effect_twice_is_two_records_each_running_out_on_its_own",
        tier: Tier::Cpu,
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
        id: "server.examination.skills-needed-listed",
        says: "An item that takes a skill or Arcane Lore to use lists them under its requirements: \
               each skill by name, then the Arcane Lore needed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-ITEM-SKILLS-NEEDED"),
        station: "yalain-testkit::cpu::entry::examine::an_item_lists_the_skills_and_arcane_lore_it_takes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.experience.solo-kill-award",
        says: "A solo kill pays the creature's death experience in full from ten levels below the \
               killer to nine above, 15% less a level further below (at least 1%), 20% less a \
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
        id: "server.gives.cannot-get-there",
        says: "A walk to give that ends at rest out of reach is refused as one that could not get \
               there: nothing handed over, the item never leaving the pack.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-CANT-GET-THERE"),
        station: "yalain-testkit::cpu::play::gives::a_give_whose_walk_ends_out_of_reach_is_refused",
        tier: Tier::Cpu,
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
        id: "server.gives.maps-without-rows",
        says: "A world whose rows name no place for the surveyor's maps (the pinned release) sends no \
               one anywhere by a map.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-MAPS-NO-ROWS"),
        station: "yalain-testkit::corpus::world_db::a_world_naming_no_place_for_the_maps_sends_no_one_by_a_map",
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
        id: "server.gives.received-moves-on",
        says: "An item a give's step hands the player that moves the same quest on when received moves \
               it after the step's own update: two records, in that order.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-GIVE-RECEIVED"),
        station: "yalain-testkit::cpu::play::gives::an_item_a_give_brings_moves_its_quest_on_after_the_step",
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
        id: "server.inventory.inscribing-an-item",
        says: "A weapon, armor, a shield, jewelry or a tool takes an inscription and the player is \
               told it is inscribed; any other item is refused with a text.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-INSCRIBE"),
        station: "yalain-testkit::cpu::play::own_requests::a_weapon_takes_an_inscription_and_a_mote_is_refused",
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
        id: "server.inventory.wield-keeps-the-packs-order",
        says: "Wielding answers with the item's place in the pack as the client keeps it (the pack's order, closed up behind anything taken out), before the weapon is drawn and the item moves; whatever was in hand goes back to the pack's end first.",
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
        id: "server.net.lost-packets-resent",
        says: "Over a link that loses packets both ways, a login still completes: the side that \
               misses a packet asks for it again and the other resends it under its original key.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-LOSS"),
        station: "yalain-testkit::cpu::entry::transport::a_login_completes_over_a_link_that_loses_every_fifth_datagram",
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
        says: "The console's dn shows the world's link and its blocks, dn block and dn unblock block or \
               lift a world or Name@world for everyone on this world, and with the network off each \
               says so.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-CONSOLE"),
        station: "yalain-federation::cpu::operator::the_console_shows_the_link_and_the_blocks_and_blocks_and_lifts",
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
               drops, and online again when it returns.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-NET-E2E"),
        station: "yalain-testkit::network::end_to_end::players_of_both_games_reach_each_other_across_three_worlds_on_one_hub",
        tier: Tier::Network,
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
        id: "server.operator.reload-from-the-console",
        says: "Typing reload at a running server's console compiles its world database again: one \
               that does not compile leaves the world as it was and says so; one that does is \
               taken by the world its players are in, without a restart.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-CONSOLE"),
        station: "yalain::live::reload::reload_at_the_console_changes_the_running_world_or_nothing",
        tier: Tier::Live,
    },
    behaviour! {
        id: "server.operator.reload-keeps-what-the-world-holds",
        says: "Reloading the world database leaves the camps as they were until a restart, and \
               keeps every template the world knew, so a thing already in the world still has \
               one; templates the database adds are taken at once.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD-KEEPS"),
        station: "yalain-testkit::cpu::operator::reload::a_reload_keeps_the_camps_and_every_template_the_world_may_hold",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "server.operator.reload-takes-what-changes-live",
        says: "Reloading the world database changes what people say at once for players already \
               in the world; a placed object that moved waits for a restart, and the report says \
               which is which.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-SRV-RELOAD"),
        station: "yalain-testkit::cpu::operator::reload::a_reload_changes_what_people_say_at_once_and_holds_placed_objects_for_a_restart",
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
        id: "server.places.actions-locked",
        says: "A use asked for while the player's earlier action is still carried out (in portal space, dead, walking to an earlier use's object, taking an item) is refused at once: its answer alone, with the status that actions are locked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-USES-LOCKED"),
        station: "yalain-testkit::cpu::entry::places::a_use_in_portal_space_is_refused_as_actions_locked",
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
        id: "server.quests.cancel-a-quest",
        says: "A quest under way that may be cancelled is cancelled on request: one update of the \
               cancel kind with the quest's record, and the quest no longer under way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-CANCEL"),
        station: "yalain-testkit::cpu::play::own_requests::a_quest_cancelled_is_updated_as_cancelled_and_no_longer_under_way",
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
        id: "server.quests.least-level-refused",
        says: "A quest is not given to a player under its least level; the person says its refusal \
               of that quest instead.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-LEAST-LEVEL"),
        station: "yalain-testkit::cpu::play::quest_uses::a_quest_is_not_given_under_its_least_level_and_the_person_says_why",
        tier: Tier::Cpu,
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
        id: "server.quests.ran-out-while-away",
        says: "A quest whose time ran out while its character was away is failed when the character \
               enters, its wait counted from when the time ran out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-RAN-OUT-AWAY"),
        station: "yalain-testkit::cpu::entry::keeping::a_quest_whose_time_ran_out_while_away_is_listed_failed",
        tier: Tier::Cpu,
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
        id: "server.quests.update-effects-come-first",
        says: "A quest update puts on the player the effects the quest's definition names for it (a \
               recipe taught, a reward quest given) before the update itself is told.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-QUEST-UPDATE-EFFECTS"),
        station: "yalain-testkit::cpu::play::quests::an_update_puts_on_its_definitions_effects_before_it_is_told",
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
        id: "server.story.starting-scenes",
        says: "A new character holds the story's starting scenes, each added as it first enters, \
               after its session start.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-STORY-START"),
        station: "yalain-testkit::cpu::play::story::a_new_character_is_given_the_starting_scenes_as_it_first_enters",
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
        id: "server.weapons.basic-attacks-read-from-their-definitions",
        says: "A player's basic attacks are the used skills with weights to be drawn by; each read \n               with its ranges and the implements it is used with.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SRV-AUTO-ATTACK-DEFS"),
        station: "yalain-testkit::dat::effects::the_basic_attacks_read_as_the_data_defines_them",
        tier: Tier::Dat,
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
               message making it the client's player, its description, the arrival effect, a \
               freeze of its movement, the end of the portal effect, the session start and the \
               welcome, in that order; private messages are numbered from 1 on the connection.",
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
