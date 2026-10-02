//! Magic -- the spellbook, the spell bar and the vitae lamp.
//!
//! One file per subject, so that two changes adding rows at the same time do not edit the same
//! file. [`ROWS`] is in id order; the registry's own test asserts that, and that no id and no
//! evidence handle is repeated anywhere in it.

// `behaviour!` is `#[macro_export]`ed by `mod.rs` above this module's declaration, so it is in
// textual scope here and needs no import.
use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "enchantments.a-purge-takes-the-timed-ones-and-leaves-the-permanent",
        says: "A purge from the shard takes away every enchantment that was going to run out and \
               leaves the permanent ones exactly where they were; the purge that takes only the \
               harmful ones leaves the helpful ones alone as well. The tally of helpful and harmful \
               enchantments ends at what survived rather than at nothing, and an item's own cooldown \
               is counted in neither tally although it is still in force.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O234-PURGE"),
        station: "dereth-testkit::cpu::magic::scenario_a_purge_leaves_the_permanent_enchantments",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "enchantments.dispel.empties-the-same-two-panels-and-says-nothing",
        says: "A dispel takes the row out of the effects pane and the skill back to its base number \
               exactly as an expiry does, and writes nothing in the chat window at all -- which is \
               what separates the panels following the shard from the panels following the line.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1127-DISPEL"),
        station: "dereth-testkit::dat::magic::scenario_a_dispel_empties_the_same_panels_without_a_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "enchantments.duration.a-timed-buff-shows-its-real-remaining-time-across-a-relog",
        says: "A timed buff shows the time it really has left rather than nothing: a thirty-minute \
               spell cast ten minutes ago reads twenty minutes, and after the player logs out and \
               back in it reads what the shard has counted it down to and goes on counting down \
               rather than starting over. A spell an equipped item carries has no time to show and \
               the pane leaves its cell empty instead of writing a zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P161"),
        station: "dereth-testkit::dat::magic::scenario_a_timed_buff_shows_its_real_remaining_time_across_a_relog",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "enchantments.expiry.the-row-stays-until-the-shard-takes-it-and-not-when-its-time-runs-out",
        says: "The client keeps no expiry clock of its own: a buff whose own time has run out long \
               ago is still drawn in the pane, the skill it raised is still raised, and nothing has \
               announced an expiry -- because it is the shard that decides an enchantment is over \
               and says so, and the number the pane draws is allowed to go past zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1127"),
        station: "dereth-testkit::dat::magic::scenario_the_row_survives_its_own_duration_running_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "enchantments.lamp-panel.a-long-description-shows-its-scrollbar-and-a-short-one-hides-it",
        says: "In the buff lamp's panel, selecting a spell whose description is longer than the \
               text pane brings up the pane's scrollbar, which the shipped layout keeps hidden \
               until there is something to scroll.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P113-LAMP-PANEL"),
        station: "dereth-ui-screens::dat::panels::lamp_panels::description_scroll::a_description_longer_than_the_pane_shows_the_bar",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "enchantments.lamp-panel.a-press-on-a-spell-row-selects-and-describes-it-and-a-second-clears-it",
        says: "A press on a spell row in the buff lamp's panel selects that row, draws it as \
               selected while the others stay plain, and fills the panel's text with the spell's \
               name followed by its description.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G31-LAMP-PANEL"),
        station: "dereth-ui-screens::dat::panels::lamp_panels::spell_selection::a_real_press_on_a_spell_row_selects_it_and_describes_the_spell",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "enchantments.lamp-panel.the-buff-and-debuff-lamps-each-list-only-their-own-kind",
        says: "The buff lamp and the debuff lamp open two different panels: a click on the debuff \
               lamp opens its own panel listing only the harmful enchantment on the character and \
               neither of the helpful ones, while the buff lamp's panel, still shut, lists \
               nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G18-LAMP-PANEL"),
        station: "dereth-ui-screens::dat::panels::lamp_panels::the_debuff_lamp_opens_the_other_instance_and_it_shows_only_the_harmful_enchantment",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "enchantments.pane.a-second-buff-reaches-a-pane-that-is-already-open",
        says: "A second buff arriving while the effects pane is open is drawn in it without the \
               player clicking anything again, and both buffs are counted -- so a pane that empties \
               is a pane following what the character carries and not one that was drawn once and \
               then abandoned.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1127-SECOND"),
        station: "dereth-testkit::dat::magic::scenario_a_second_buff_reaches_an_already_open_pane",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "enchantments.qualities.a-listed-whole-number-or-decimal-property-reads-enchanted-and-an-unlisted-one-stored",
        says: "A whole-number or decimal property of the character that the shipped list of \
               enchantable properties names is read with its enchantments applied -- an allegiance \
               rank of 5 under a +1 reads 6, a health regeneration rate of 0.5 under a x1.5 reads \
               0.75 -- while the plain read of the same property still gives what the shard stored. \
               A property the list does not name keeps its stored value whatever spell names it, a \
               property the character does not have is not conjured by a spell on it, and without \
               the list nothing is enchanted at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ENCHANTED-READS-FILTER"),
        station: "dereth-client-model::dat::character::enchanted_quality_reads::a_listed_whole_number_or_decimal_property_reads_enchanted_and_an_unlisted_one_reads_stored",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "enchantments.qualities.every-property-the-client-reads-plainly-is-outside-the-shipped-filter",
        says: "Every whole-number and decimal property the client reads for a panel, a formula or a \
               check without its enchantments -- burden, deaths, augmentations, vitae, level, \
               heritage, coin, house, the spell formula's augmentations, the vital, skill, load, run \
               and jump inquiries -- is one the shipped list of enchantable properties does not name, \
               so reading it plainly gives the same number the enchanted read would.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ENCHANTED-READS-PLAIN"),
        station: "dereth-client-model::dat::character::enchanted_quality_reads::every_property_the_client_reads_plainly_is_outside_the_shipped_filter",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "enchantments.removal.takes-the-row-out-of-the-pane-the-skill-back-to-base-and-says-so",
        says: "When the shard takes an enchantment away the row leaves the effects pane while the \
               pane stays open, the Skills page puts the skill back to its base number in its plain \
               colour, and exactly one line naming that spell and saying it has expired is written \
               on the magic channel.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1127-REMOVE"),
        station: "dereth-testkit::dat::magic::scenario_the_shards_removal_empties_the_pane_and_the_skill",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "enchantments.vitals.a-buff-on-a-maximum-still-raises-it",
        says: "A spell raising maximum health raises the maximum the panel shows, while a spell \
               naming the current health leaves the current health as the shard stored it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ENCHANTED-VITALS-MAX"),
        station: "dereth-client-model::dat::character::enchanted_quality_reads::a_buff_on_a_vital_maximum_still_raises_it_while_one_naming_the_current_value_does_not",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "enchantments.vitals.an-all-vitals-multiplier-scales-the-maxima-and-not-the-current-values",
        says: "A spell that multiplies every vital at once -- the swamp blights and the culinary \
               debuff take them all to 0.6 -- lowers maximum health, stamina and mana, while the \
               current health, stamina and mana the shard stored are read as they are: a character \
               at 40 of 50 health reads 40 of 30. Only the maxima are on the shipped list of \
               enchantable vitals.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ENCHANTED-VITALS-ALL"),
        station: "dereth-client-model::dat::character::enchanted_quality_reads::an_all_vitals_multiplier_scales_the_maxima_and_leaves_the_current_values_as_stored",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "magic.cast.a-fizzle-ends-the-cast-prints-its-line-and-burns-nothing",
        says: "When the shard answers a cast by saying the spell fizzled, the client stops waiting, \
               writes exactly that sentence on the magic channel, and takes nothing out of the \
               player's pack -- the components are the shard's to spend, and the counts move only \
               when the shard's own update arrives. An answer that says nothing went wrong takes the \
               client out of waiting and writes no line at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1126"),
        station: "dereth-testkit::dat::magic::scenario_a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "magic.cast.a-refusal-the-client-makes-itself-is-said-in-its-own-words-and-never-sent",
        says: "A cast the client refuses by itself never reaches the shard and never puts the player \
               in waiting: a target the spell cannot be cast on is named in the refusal, nothing \
               selected at all gets its own sentence, and a missing component gets a third -- taken \
               before the target is even looked at. The same gesture at a legal target does go out \
               and does announce itself, so the three silences are refusals and not a client that \
               never casts.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1126-REFUSAL"),
        station: "dereth-testkit::dat::magic::scenario_a_refusal_the_client_makes_itself_never_reaches_the_shard",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "magic.cast.a-sent-cast-stops-the-players-body",
        says: "A cast the client actually sends, at a target or at the player himself, first \
               brings the player's body to a complete stop, once; a cast the client refuses on its \
               own stops nothing, and while the shard is driving the body the cast still goes out \
               but the body is not stopped.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O250-CAST-SENT"),
        station: "dereth-client::cpu::magic::spell_casting::a_cast_that_is_sent_asks_the_body_to_stop",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "magic.cast.a-spell-bar-click-puts-the-cast-request-on-the-wire",
        says: "Clicking cast on the spell bar with a targeted spell picked and a target selected \
               sends one targeted cast request naming that spell and that target, and counts one \
               spell cast.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O250-CAST"),
        station: "dereth-client::cpu::magic::spell_casting::a_click_on_the_spell_bar_puts_a_cast_on_the_wire",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "magic.cast.the-shards-recorded-fizzle-prints-the-same-line-and-does-not-end-the-cast",
        says: "The shard's own recorded fizzle -- the bytes it really sent, replayed -- writes the \
               same sentence on the same channel, and leaves the cast in flight: saying what went \
               wrong and ending the cast are two different messages, and a client that did both on \
               one of them would end the cast twice.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1126-RECORDED"),
        station: "dereth-testkit::dat::magic::scenario_a_recorded_fizzle_prints_the_line_without_ending_the_cast",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "magic.enchantments.a-multiple-removal-announces-and-a-dispel-does-not",
        says: "When several enchantments are removed at once the client prints each one's has \
               expired. line, and when several are dispelled at once it takes them away just the \
               same and prints nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P24-ENCHANTMENTS"),
        station: "dereth-client::gpu::magic::enchantment_receivers::a_multiple_removal_announces_and_a_multiple_dispel_does_not",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "magic.fizzle.the-recorded-fizzle-plays-the-bodys-emitter-and-sound",
        says: "A recorded fizzle effect aimed at the player puts exactly one particle emitter on \
               his body, drawing the shipped fizzle particles, and raises the fizzle sound from \
               his body's own sound table.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1167-FIZZLE"),
        station: "dereth-client::gpu::magic::fizzle_effect::the_recorded_fizzle_reaches_the_bodys_emitter_and_raises_its_sound",
        tier: Tier::Gpu,
    },
    behaviour! {
        id: "magic.formula.a-spells-tapers-are-the-accounts-and-not-the-dats",
        says: "The components a spell costs are worked out for the player's own account: the taper \
               slots differ from the formula the shipped data stores and differ again for another \
               account, while a short formula with no tapers is the same for everyone.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1118-FORMULA-SPELLS"),
        station: "dereth-client-model::dat::magic::account_formula_tapers::the_world_formula_is_the_accounts_and_not_the_dats",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "magic.formula.carrying-the-foci-or-the-augmentation-switches-to-scarab-plus-prismatic-tapers",
        says: "A character carrying the foci of a spell's school is charged a scarab and four \
               prismatic tapers for that spell instead of its long formula, and only for spells of \
               that school; without the foci the spell keeps its full formula with this account's \
               tapers.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116F-FORMULA-CARRYING"),
        station: "dereth-client-model::dat::magic::scarab_and_taper_formula::carrying_the_foci_switches_the_formula_to_scarab_plus_prismatic_tapers",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "magic.research.a-test-outside-magic-mode-or-without-a-target-is-refused-and-never-sent",
        says: "A formula test outside magic mode, with nothing selected, or on a world without \
               spell research is refused by the client in its own words and never sent.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-RESEARCH-REFUSAL"),
        station: "dereth-client::cpu::magic::spell_casting::a_test_outside_magic_mode_or_without_a_target_is_refused_and_never_sent",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "magic.research.a-tested-formula-goes-out-as-component-ids-and-the-target",
        says: "On a world with spell research, testing a formula in magic mode with a target \
               sends the laid components as their component ids, in the order laid, the unused \
               slots zero, and the target; the player then waits on the answer as on a cast.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-RESEARCH-TEST"),
        station: "dereth-client::cpu::magic::spell_casting::a_tested_formula_goes_out_as_component_ids_and_the_target",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "magic.resist.a-squelched-magic-channel-drops-the-same-recorded-bytes",
        says: "A player who has squelched the magic channel is not shown the resist at all: the same \
               recorded bytes reach the client and no line appears, and the gate counts that it \
               dropped one -- so the silence is the squelch and not a message the client failed to \
               read.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1168-SQUELCH"),
        station: "dereth-testkit::dat::magic::scenario_a_squelched_magic_channel_drops_the_same_bytes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "magic.resist.the-shards-own-sentence-is-drawn-verbatim-on-the-magic-channel",
        says: "When a target resists a spell the shard writes the sentence naming it and the client \
               draws that sentence unchanged, with no prefix and no punctuation of its own, on the \
               channel the shard marked it with and in that channel's colour. The recording shows \
               the pattern it belongs to: every such sentence is followed within milliseconds by the \
               resist sound, played on the caster's own body.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1168"),
        station: "dereth-testkit::dat::magic::scenario_a_recorded_resist_is_drawn_verbatim_on_the_magic_channel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "magic.resist.the-sound-that-comes-with-it-reaches-the-clients-own-queue",
        says: "The sound the shard sends beside a resist is accepted and queued rather than dropped: \
               the recording's own bytes, re-addressed to this character, are decoded and counted as \
               a sound event with nothing left unhandled.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1168-SOUND"),
        station: "dereth-testkit::dat::magic::scenario_the_resist_sound_reaches_the_clients_queue",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "magic.targeting.a-spell-aims-at-what-its-formulas-target-component-names",
        says: "Whether a spell needs a target, and what it may be cast at, follows the component that \
               closes its formula: a war bolt such as Frost Blast III must be aimed at a creature, an \
               item enchantment such as Blade Bane I at an item, and a portal tie at a portal, while \
               a spell whose closing component names nothing is cast without a target.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-SPELL-TARGET-COMPONENT"),
        station: "dereth-client-model::dat::magic::spell_target_type::a_spells_target_type_comes_from_its_formulas_target_component",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "panels.redraw.the-component-list-follows-a-count-changing-under-an-unchanged-component",
        says: "How many of a spell component the player owns is redrawn when it changes, although \
               the list of components has not; spending three of something is the ordinary case \
               and not an edge, and its picture arriving later counts the same way.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O487-COMPONENTS"),
        station: "dereth-testkit::dat::inventory::scenario_an_owned_component_count_redraws_its_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.strip.a-click-on-a-header-clears-the-selection-and-the-first-one-does-nothing",
        says: "Clicking a heading on the components page unselects whatever was selected in the \
               world and takes the highlight off the page with it -- except the very first heading, \
               which is the one arm that does nothing at all and leaves the selection where it was.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P16-HEADER"),
        station: "dereth-testkit::dat::magic::scenario_a_click_on_a_header_clears_the_selection_and_the_first_one_does_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.strip.a-click-on-a-row-selects-that-component-in-the-world",
        says: "Pressing a component's row on the components page selects that component in the \
               world -- the object the row stands for, carrying the flag the selection marker reads \
               -- and asks the shard for nothing, so it is a selection and not a use.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P16"),
        station: "dereth-testkit::dat::magic::scenario_a_click_on_a_component_row_selects_that_component_in_the_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.strip.a-components-icon-has-a-black-outline-and-a-clear-surround",
        says: "The magic window draws a component's icon from the component table with its \
               opaque white outline turned opaque black, around a clear surround; on a world of \
               the files before Throne of Destiny, whose icons store no alpha, the icon's black \
               surround is the clear colour.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-R2-COMPONENT-ICON"),
        station: "dereth-client::dat::rendering::component_icons::a_components_icon_draws_a_clear_surround_and_a_black_outline_on_either_world",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.strip.a-header-is-drawn-only-for-a-kind-the-player-holds-something-of",
        says: "The components page gives a heading to the kinds the player actually holds something \
               of and to no others: an empty pack draws no heading at all, one kind draws one \
               heading, a second kind somewhere else in the order draws a second, and emptying a \
               kind takes its heading away again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116E-HEADERS"),
        station: "dereth-testkit::dat::magic::scenario_a_header_is_drawn_only_for_a_kind_the_player_holds_something_of",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.strip.a-row-shows-how-many-are-held-and-goes-away-at-none",
        says: "A component's row on the components page says how many of that kind the player is \
               carrying, counting two piles of one kind as their sum and following each pile as it \
               comes and goes. The last one leaving takes the row away rather than leaving a row \
               reading nought.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116E-COUNT"),
        station: "dereth-testkit::dat::magic::scenario_a_component_row_shows_how_many_are_held_and_goes_away_at_none",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.strip.it-rebuilds-when-the-pack-changes-and-not-every-frame",
        says: "The page is redrawn when something in the pack moves and at no other time: frames on \
               which nothing moved redraw nothing, and the next component to arrive redraws it \
               again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116E-EDGE"),
        station: "dereth-testkit::dat::magic::scenario_the_component_page_rebuilds_on_the_pack_and_not_every_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.strip.selecting-a-component-in-the-world-highlights-its-row",
        says: "Selecting a component object anywhere in the world highlights that component's row \
               on the page, matching on the kind rather than on the object -- so a second pile of a \
               kind highlights the same row as the first. Selecting the same thing again does \
               nothing, selecting something that is not a component clears the highlight, and \
               doing that twice over does nothing the second time. The highlight the page puts on \
               itself is not sent back out as a selection of its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P45-SELECTION"),
        station: "dereth-testkit::dat::magic::scenario_selecting_a_component_in_the_world_highlights_its_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.strip.the-held-count-follows-a-stack-size-the-shard-changes",
        says: "When the shard says a stack the player is carrying is a different size, the number \
               the components page shows for that kind moves by the difference and the column on \
               screen is redrawn with it, rather than keeping the number the login left.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P45-STACK"),
        station: "dereth-testkit::dat::magic::scenario_the_held_count_follows_a_stack_size_the_shard_changes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.strip.the-icon-is-the-shipped-tables-and-not-the-objects",
        says: "The picture beside a component's name is the one the shipped component table gives \
               that kind and not the one the object itself carries, while the name beside it is the \
               object's -- so a stack whose own picture is wrong is still drawn correctly.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116E-ICON"),
        station: "dereth-testkit::dat::magic::scenario_the_component_row_icon_is_the_shipped_tables_and_not_the_objects",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.strip.the-list-is-the-census-of-what-the-player-carries",
        says: "The components page is a census of the pack a recorded session really left the \
               player holding: a heading for each kind he holds something of and nothing else, the \
               rows of that kind under it, and each row naming its own component, saying how many \
               he has and how many he wants -- in the three columns on screen and not only in the \
               panel's own idea of them. Every row stands for a real component object of its kind.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P45-CENSUS"),
        station: "dereth-testkit::dat::magic::scenario_the_components_page_is_the_census_of_what_the_player_carries",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.strip.the-walk-is-kind-order-with-each-kinds-rows-under-its-header",
        says: "The page is walked in the shipped order of the kinds, each heading followed by the \
               rows that belong under it, so a formula whose components fall in several kinds \
               draws one heading and one row for each of them in that order.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116E-ORDER"),
        station: "dereth-testkit::dat::magic::scenario_the_component_walk_is_kind_order_with_each_kinds_rows_under_its_header",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-components.the-pack-is-counted-as-the-player-fills-it",
        says: "The spell components the player is carrying are counted as they reach him: an item \
               moved into his pack is counted as the size of its stack, or as one when it carries no \
               stack at all, and an item that is not a component is not counted whatever pack it is \
               in. A side pack full of components is walked when the character arrives, walking it a \
               second time does not double anything, a stack that changes size moves the count by the \
               difference, and giving one of two away leaves the kind still owned.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O234-COMPONENTS"),
        station: "dereth-testkit::cpu::magic::scenario_the_component_tally_follows_the_pack",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "spell-components.what-the-player-asks-to-keep-reaches-the-shard-and-the-vendor",
        says: "How many of a component the player says he wants to keep in stock is told to the shard \
               and remembered here, and a number the panel will not accept is neither told nor \
               remembered -- the panel is given the stored one back instead. Asking a shop to fill him \
               up buys only what he is short of, priced by that shop, and tells him in his chat window \
               which component the shop does not stock.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O234-DESIRED"),
        station: "dereth-testkit::cpu::magic::scenario_the_desired_level_reaches_the_shard_and_the_vendor_buys_the_shortfall",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "spell-examine.cancel.a-look-cancels-an-appraisal-in-flight-and-only-the-first-of-two-does",
        says: "Looking at a spell while the player is waiting to hear about an item he is appraising \
               tells the shard to forget that item -- one message carrying nothing, not a second \
               request -- and the pointer is left alone, because asking about nothing and arming the \
               examine cursor are two different things and the client does not confuse them. A \
               second look straight afterwards has nothing left to forget and says nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116B"),
        station: "dereth-testkit::dat::magic::scenario_a_look_cancels_an_appraisal_in_flight_and_only_the_first_of_two_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.cancel.a-look-with-nothing-in-flight-asks-the-shard-nothing",
        says: "Looking at a spell when the player is waiting to hear about nothing puts no message \
               on the wire at all -- the description still opens and still names the spell, so the \
               silence is the client declining to cancel and not a look that never happened.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116B-QUIET"),
        station: "dereth-testkit::dat::magic::scenario_a_look_with_nothing_in_flight_asks_the_shard_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.components.a-click-on-an-icon-selects-the-one-in-the-pack",
        says: "Clicking a component icon in a spell's description selects the component object the \
               player is carrying for that slot, and the description stays on the spell rather than \
               being replaced by the component -- so the player can look at what a spell costs and \
               then pick one of the things it costs out of his own pack without losing the page.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116C"),
        station: "dereth-testkit::dat::magic::scenario_a_click_on_a_component_icon_selects_the_one_in_the_pack",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.components.each-icon-names-its-own-slot",
        says: "Every icon of the formula selects its own component and not the first one the pack \
               happens to hold, so the row under the pointer is what the lookup is keyed on.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116C-EACH"),
        station: "dereth-testkit::dat::magic::scenario_each_component_icon_names_its_own_slot",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.components.one-the-player-does-not-carry-selects-nothing",
        says: "A component the player has none of is drawn all the same and clicking it changes \
               nothing at all -- no selection, and nothing half-done either. The slot beside it \
               still selects in the same run, so the silence is the lookup refusing and not the \
               click failing to arrive.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116C-NONE"),
        station: "dereth-testkit::dat::magic::scenario_a_component_the_player_does_not_carry_selects_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.components.the-answer-is-a-representative-of-the-class-and-not-one-object",
        says: "The object a component icon stands for is a representative of that kind rather than \
               one particular thing: the answer does not wander between two looks, and taking the \
               answered one out of the pack while another of the same kind is still there leaves \
               the icon answering the one that stayed. A kind the player carries none of, and \
               something that is not a component at all, both answer nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116C-WALK"),
        station: "dereth-testkit::dat::magic::scenario_the_component_a_slot_stands_for_is_a_representative_of_its_kind",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.components.the-secondary-click-does-nothing-on-an-icon",
        says: "The secondary button does nothing on a component icon -- no selection, no second \
               look at the spell, nothing -- and the ordinary click at the very same point still \
               selects, so the icon is reached by one button only.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116C-RIGHT"),
        station: "dereth-testkit::dat::magic::scenario_the_secondary_click_does_nothing_on_a_component_icon",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.formula.a-foci-for-another-school-leaves-the-long-formula-alone",
        says: "A foci for the wrong school changes nothing: the spell is still charged the long \
               per-account formula, because the shortcut is granted by the foci that belongs to that \
               spell's own school and not by any container the player happens to be carrying.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116F-WRONG"),
        station: "dereth-testkit::dat::magic::scenario_a_foci_for_another_school_leaves_the_long_formula_alone",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.formula.carrying-a-foci-changes-what-the-client-would-spend",
        says: "A character carrying the foci of a spell's own school is charged a scarab and a \
               handful of prismatic tapers instead of the long formula, and the client works that \
               out from the shipped table rather than being told: the same spell, the same \
               character, the foci in a side pack, and the list the client would spend is a \
               different and shorter one.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116F-FORMULA"),
        station: "dereth-testkit::dat::magic::scenario_carrying_a_foci_changes_what_the_client_would_spend",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.formula.the-account-the-shard-names-is-the-one-the-client-keeps",
        says: "The name the shard greets the player with when it offers his characters is the one \
               the client keeps, and a second greeting naming somebody else replaces it rather than \
               blending the two -- before any greeting arrives the client has no name at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1118-ACCOUNT"),
        station: "dereth-testkit::dat::magic::scenario_the_account_the_shard_names_is_the_one_the_client_keeps",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.formula.the-formula-the-client-would-cast-follows-that-account",
        says: "Which tapers a spell costs depends on that name: the list the client would spend \
               changes when the greeting arrives, is not the one the shipped table stores, and \
               changes again for a different name -- so the slots that move are the account's and \
               not a constant.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1118-FORMULA"),
        station: "dereth-testkit::dat::magic::scenario_the_formula_the_client_would_cast_follows_that_account",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.formula.the-look-lists-the-tapers-this-account-must-carry",
        says: "The description a player reads lists the components this account must carry: not the \
               ones the shipped table stores, and not another account's -- which is what makes one \
               wrong taper both a wrong list and a cast that would fail.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1118-PANE"),
        station: "dereth-testkit::dat::magic::scenario_the_look_lists_the_tapers_this_account_must_carry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.formula.the-look-with-a-foci-lists-a-scarab-and-four-tapers",
        says: "Looking at a spell while carrying its school's foci lists a scarab and four \
               prismatic tapers, and draws that many icons rather than the long formula's.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116F-PANE"),
        station: "dereth-testkit::dat::magic::scenario_the_look_with_a_foci_lists_a_scarab_and_four_tapers",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.formula.the-look-without-a-foci-lists-the-long-per-account-formula",
        says: "The same look at the same spell without the foci lists the long per-account formula \
               and no prismatic taper at all, so the short list is the foci's doing and not one \
               fixed answer the pane always gives.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116F-LONG"),
        station: "dereth-testkit::dat::magic::scenario_the_look_without_a_foci_lists_the_long_per_account_formula",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.formula.the-shipped-foci-table-reaches-the-game-model-with-the-description",
        says: "Which foci belongs to which school is shipped data the client reads at startup, and \
               it reaches the part of the client that works out a formula when the character's own \
               description arrives -- before that the client answers that no school has one, so the \
               hand-off is what makes the shortcut reachable at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116F-MAPPER"),
        station: "dereth-testkit::dat::magic::scenario_the_shipped_foci_table_reaches_the_game_model_with_the_description",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.formula.two-accounts-are-shown-different-tapers-for-one-spell",
        says: "Two accounts looking at one spell are shown two different lists, so the list is \
               derived from the name the shard greeted each of them with and not from the character, \
               the world or a constant.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1118-OTHER"),
        station: "dereth-testkit::dat::magic::scenario_two_accounts_are_shown_different_tapers_for_one_spell",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.marks.a-closed-window-still-re-marks-its-rows",
        says: "A description the player has closed still keeps its marks up to date, so re-opening \
               it shows what he is carrying now rather than what he was carrying when he shut it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116D-CLOSED"),
        station: "dereth-testkit::dat::magic::scenario_a_closed_window_still_re_marks_its_rows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.marks.a-component-arriving-clears-its-mark-and-one-leaving-brings-it-back",
        says: "A component arriving in the pack clears its mark while the description is open and \
               without the page being drawn again, and the same component leaving brings the mark \
               back -- so what the player reads follows his pack rather than the moment he opened \
               the window.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116D-LIVE"),
        station: "dereth-testkit::dat::magic::scenario_a_component_arriving_clears_its_mark_and_one_leaving_brings_it_back",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.marks.a-second-of-the-same-kind-keeps-the-mark-off",
        says: "Being marked is about the kind and not the count: two piles of one component, one of \
               them given away, and the mark stays off, because the player still owns some. It is \
               the last one leaving that brings the mark back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116D-OWNED"),
        station: "dereth-testkit::dat::magic::scenario_a_second_of_the_same_kind_keeps_the_mark_off",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.marks.everything-carried-marks-nothing-and-nothing-carried-marks-everything",
        says: "The two ends of the range, so neither answer is a constant: a player carrying every \
               component of a formula sees no mark at all, and one carrying none of them sees every \
               slot marked.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116D-ENDS"),
        station: "dereth-testkit::dat::magic::scenario_everything_carried_marks_nothing_and_nothing_carried_marks_everything",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.marks.the-components-the-player-lacks-are-the-ones-that-are-marked",
        says: "A spell's description marks the components the player has none of and leaves the \
               ones he is carrying plain -- and it does that when the page is first drawn, not only \
               when something later moves in his pack, which matters because the shipped layout \
               draws the mark over every icon until something takes it off.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116D"),
        station: "dereth-testkit::dat::magic::scenario_the_components_the_player_lacks_are_the_ones_that_are_marked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.marks.the-notice-moves-for-a-component-and-for-nothing-else",
        says: "The marks are brought up to date when a component moves and at no other time: an \
               ordinary item moving into the same pack does nothing, and frames on which nothing \
               moved do nothing -- so this is not a panel redrawing itself every frame.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116D-SERIAL"),
        station: "dereth-testkit::dat::magic::scenario_the_notice_moves_for_a_component_and_for_nothing_else",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.pane.a-second-look-re-opens-the-window-the-close-control-shut",
        says: "The close control in the description window's corner shuts it, and the next look at a \
               spell opens it again on that spell -- both halves, because a window that never closed \
               and one that closed and stayed shut look the same after a single press.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116-CLOSE"),
        station: "dereth-testkit::dat::magic::scenario_a_second_look_re_opens_the_window_the_close_control_shut",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.pane.a-secondary-click-on-a-known-spell-opens-its-description",
        says: "Clicking a spell the character knows with the secondary button opens the examine \
               window on the description of that spell: its own name in the title, its school, what \
               it costs to cast, how far it reaches, and the components of its formula in the order \
               the formula lists them -- each one drawn as an icon and not only as a line of text. \
               Nothing is asked of the shard for any of it.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116"),
        station: "dereth-testkit::dat::magic::scenario_a_secondary_click_on_a_spell_opens_its_description",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.pane.an-enchantment-shows-how-long-it-lasts-and-no-range",
        says: "An enchantment's description says how long it lasts and leaves the range line empty, \
               where a bolt's says how far it reaches and leaves the duration line empty -- an \
               absent number is an empty line and never a zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116-DURATION"),
        station: "dereth-testkit::dat::magic::scenario_an_enchantment_shows_how_long_it_lasts_and_no_range",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.pane.the-primary-click-selects-the-row-and-examines-nothing",
        says: "The ordinary click on the same row does what it always did -- it selects the spell, \
               so the delete button points at it -- and opens no description at all, which is what \
               makes the secondary click a gesture of its own rather than any press opening the \
               window.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116-LEFT"),
        station: "dereth-testkit::dat::magic::scenario_the_primary_click_selects_the_row_and_examines_nothing",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.pane.the-same-look-from-the-cast-bar-opens-it-without-selecting",
        says: "The same look works on a spell sitting on the casting bar and opens the same \
               description -- and there it moves no selection at all, which is the difference \
               between the two lists the gesture reaches.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116-BAR"),
        station: "dereth-testkit::dat::magic::scenario_the_same_look_from_the_cast_bar_opens_it_without_selecting",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spell-examine.pane.the-secondary-click-moves-the-books-own-selection-too",
        says: "The look also moves the spellbook's own selection to the row it was made on, so the \
               delete button is left pointing at the spell the player has just read about.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1116-SELECT"),
        station: "dereth-testkit::dat::magic::scenario_the_secondary_click_moves_the_books_own_selection_too",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.caption.selecting-a-spell-or-a-wand-names-it",
        says: "The spell bar's caption is empty while nothing is selected and no wand is held, and \
               pressing a spell on the bar writes that spell's name into it, following each new \
               selection.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1147-CAPTION"),
        station: "dereth-ui-screens::dat::magic::spell_bar_caption_and_ring::selecting_a_spell_on_the_bar_names_it_in_the_caption",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.cast-button.its-state-and-tooltip-follow-the-selected-spell-its-target-and-the-wand",
        says: "With a spell that needs a target selected on the bar, the cast button is greyed and \
               its tooltip asks for a target; selecting a suitable target enables it with CAST, \
               the spell and the target's name, and an unsuitable one greys it again and asks for \
               an appropriate target.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1164-CAST-BUTTON"),
        station: "dereth-ui-screens::dat::magic::spell_bar_caption_and_ring::cast_button::cast_button_a_targeted_spell_walks_its_three_arms",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.click.a-press-selects-a-clipped-row-and-refuses-foreign-or-empty-rows",
        says: "A press on a spell bar row that is only partly on screen selects that spell, rings \
               it and scrolls the bar just far enough to show the whole row. A press made on \
               another window never selects on the bar, a right-button press selects nothing, and \
               a double press casts the spell already selected.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-SPELLBAR-POPULATION-CLICK"),
        station: "dereth-ui-screens::dat::magic::spell_bar_population::actual_pointer_selects_clipped_tail_and_rejects_foreign_or_empty_rows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.click.one-click-selects-and-only-the-double-click-casts",
        says: "A click on a spell on the casting bar, made with a real pointer and hit-tested \
               against the shipped tree, selects that spell and casts nothing; the second press of \
               a double click on the very same place is what casts it. The two are told apart by \
               the gesture alone, which is why one click picking a spell cannot cast it by \
               accident.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-G40"),
        station: "dereth-testkit::dat::magic::scenario_one_click_selects_and_only_the_double_click_casts",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.drag-hint.a-spell-over-a-bar-row-lights-that-row-and-the-landing-row-does-not-keep-it",
        says: "A spell carried from the spellbook over a spell bar row lights that row green, and \
               letting go there adds the spell to the tab at that row and takes the green off, so \
               the row it landed on does not stay lit.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1115-DRAG-HINT"),
        station: "dereth-ui-screens::dat::magic::spell_bar_drag_hints::the_row_the_spell_lands_on_does_not_keep_its_green",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.favorite.model-and-wire-move-together",
        says: "Dragging a spell onto the spell bar both inserts the row the client draws and puts \
               the matching message on the wire in the same gesture, so the bar survives a relog; \
               a drop on an occupied row inserts rather than appending, and a bank outside the \
               eight the player has writes nothing and sends nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F9"),
        station: "dereth-testkit::cpu::magic::scenario_spell_favorite_moves_model_and_wire",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "spellbar.favorite.removal-reaches-the-copy-a-relog-reads",
        says: "Taking a spell off the bar removes the row the client draws and tells the shard, and \
               both the addition and the removal reach the packed copy of the bar the client sends back \
               at the next options save, so a change survives the relog rather than being lost at the \
               next settings message.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F9-REMOVE"),
        station: "dereth-testkit::cpu::magic::scenario_a_favorite_removal_reaches_the_packed_copy",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "spellbar.keys.a-real-key-press-moves-the-selection-the-ring-and-the-scroll",
        says: "Pressing the shipped next-spell key steps the selection along the casting bar one \
               spell at a time, moves the ring on to that spell and off every other, and scrolls \
               the bar just far enough to reveal the row that was off the end -- a row already on \
               screen is not scrolled to. Moving the selection puts nothing on the wire at all.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-RING"),
        station: "dereth-testkit::dat::magic::scenario_a_real_key_press_moves_the_selection_the_ring_and_the_scroll",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.keys.every-shipped-magic-key-raises-its-own-instruction-and-the-release-raises-none",
        says: "Every key the shipped bindings give the casting bar reaches the client's own magic \
               arm and raises exactly the instruction that key stands for -- the nine that pick a \
               numbered slot each naming their own -- and letting the key go raises nothing, so the \
               keys produce as many instructions as there are keys and not twice as many.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-MAGIC"),
        station: "dereth-testkit::dat::magic::scenario_every_shipped_magic_key_raises_its_own_instruction",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.keys.the-frames-own-pass-delivers-what-was-queued-in-order",
        says: "The instructions those keys raise are drained by the client's own per-frame pass and \
               delivered to the casting bar: a frame with none queued moves nothing, a frame with \
               one delivers it and empties the queue, and a frame with two delivers both in the \
               order they were raised -- which is what a player mashing the key produces.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-DRAIN"),
        station: "dereth-testkit::dat::magic::scenario_the_frames_own_pass_delivers_what_was_queued_in_order",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.keys.the-instructions-move-the-selection-and-the-cast-keys-cast",
        says: "The selection instructions step the casting bar forwards and backwards, jump to its \
               first and last spells, and wrap round at both ends; the cast instruction casts \
               whatever is selected, and one that names a numbered slot moves the selection to that \
               slot first and then casts it. A slot past the end of the bar does nothing at all -- \
               not even a refusal.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-BAR"),
        station: "dereth-testkit::dat::magic::scenario_the_instructions_move_the_selection_and_the_cast_keys_cast",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.quickslots.empty-padding-never-casts",
        says: "A quick-slot key casts the spell at that position among the spells the bar actually \
               shows, and a favourite the character no longer knows is dropped from the bar with \
               the shard told once to forget it. A key for an empty padding slot at the end of the \
               bar, or past it, and a press or double press on an empty row, cast nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-SPELLBAR-POPULATION-QUICKSLOTS"),
        station: "dereth-ui-screens::dat::magic::spell_bar_population::quickslots_index_filtered_ui_rows_and_empty_padding_never_casts",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.ring.the-ring-and-underlay-follow-the-endowment-selection-and-paging",
        says: "With a wand held, the ring round the wand's icon on the spell bar goes down once a \
               spell is picked on the open tab, and paging to a tab with no spell selected brings \
               the ring back and captions the wand and its spell again.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1147-RING"),
        station: "dereth-ui-screens::dat::magic::spell_bar_caption_and_ring::paging_the_bar_re_reads_the_ring_and_the_caption_for_the_open_tab",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.tabs.a-frame-fills-all-eight-bar-lists",
        says: "On the first frame all eight tabs of the spell bar are filled with real rows \
               holding the player's favourite spells in the order the player put them, each row \
               drawing its spell's icon, and the first nine rows of each tab show their shortcut \
               numeral while the rest show none.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-SPELLBAR-POPULATION-TABS"),
        station: "dereth-ui-screens::dat::magic::spell_bar_population::frame_populates_all_eight_actual_listboxes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.tabs.the-tab-instructions-walk-the-eight-banks-and-stop-at-the-ends",
        says: "The four bank instructions walk the casting bar's banks: one forward and one back \
               return to where they started, the last and first instructions open the last and the \
               first bank, and a bank the client does not know falls back to the first in either \
               direction.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O410-TABS"),
        station: "dereth-testkit::dat::magic::scenario_the_tab_instructions_walk_the_eight_banks",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.transfer.a-double-click-or-a-drag-from-the-book-adds-the-spell-to-the-open-tab",
        says: "Double-clicking a spell in the spellbook adds it to the end of the open spell bar \
               tab and selects it there, without changing what the spellbook itself has selected; \
               double-clicking a spell the tab already holds adds nothing and sends nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FABLE-SPELL-TRANSFER-TRANSFER"),
        station: "dereth-ui-screens::dat::magic::spell_bar_transfer::a_double_click_appends_the_spell_to_the_open_tab_and_never_moves_the_selection",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbar.transfer.a-spell-dragged-off-a-tab-is-removed",
        says: "Picking a spell up off a spell bar tab and starting to drag it removes it from that \
               tab and tells the shard at once, as soon as it is lifted and before it is let go \
               anywhere.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-FABLE-SPELL-TRANSFER-TRANSFER-SPELL"),
        station: "dereth-ui-screens::dat::magic::spell_bar_transfer::dragging_a_spell_off_a_tab_removes_it_when_it_is_picked_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbook.click.reveals-and-selects-the-row",
        says: "Clicking a partly off-screen spell row scrolls the list so the row is fully \
               visible and draws the selection ring on that row and no other, in the same frame; \
               the selection and the scroll position stay put on later frames with no further \
               input.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-SPELLBOOK"),
        station: "dereth-testkit::dat::magic::scenario_a_spellbook_click_reveals_and_selects",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbook.delete.asks-the-shard-and-predicts-nothing",
        says: "Deleting a spell from the book asks the shard to remove it and changes nothing locally, \
               so a refusal by the shard cannot leave the player looking at a book that has already \
               lost the spell.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F9-DELETE"),
        station: "dereth-testkit::cpu::magic::scenario_deleting_a_spell_predicts_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "spellbook.filter.a-refill-after-a-filter-press-resets-the-scroll",
        says: "Pressing a school filter button in the spellbook while its list is scrolled down \
               refills the list with only the spells of the schools still shown and puts it back \
               at the top, its first row at the top of the pane.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-ASTRA-ITEMLIST-NAVIGATION-FILTER"),
        station: "dereth-ui-screens::dat::magic::spellbook_selection_scroll::actual_filter_button_resets_scroll_after_refill",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbook.learned-spell.appears-without-a-relog",
        says: "A spell learned during a session appears in the book straight away, so it can be cast \
               and put on a bar without logging out; the first one makes the book, a repeat leaves the \
               page already there alone, unlearning takes only that one back out, and one learned \
               before the character's own description has arrived is dropped rather than crashing.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O937-SPELL"),
        station: "dereth-testkit::cpu::magic::scenario_a_learned_spell_appears_without_a_relog",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "spellbook.redraw.a-badge-follows-a-spells-own-flags-under-an-unchanged-list",
        says: "A spell's own flags moving while the list of spells stays exactly the same puts the \
               badge that stands for them on the icon the row draws, and clearing the flag takes it \
               off again -- and neither of those disturbs the order of the rows.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O567-BADGE"),
        station: "dereth-testkit::dat::magic::scenario_a_spellbook_badge_follows_a_spells_own_flags",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbook.redraw.an-identical-frame-does-not-rebuild-and-a-changed-spell-list-does",
        says: "The spellbook fills its rows on the first pass, does nothing at all on a pass whose \
               spells are identical, and rebuilds when a spell leaves the list -- so the gate that \
               stops it redrawing every frame is alive rather than welded shut or welded open.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O567-GATE"),
        station: "dereth-testkit::dat::magic::scenario_an_identical_spellbook_frame_does_not_rebuild_and_a_changed_list_does",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbook.redraw.the-rows-re-sort-when-a-spells-place-in-the-order-moves",
        says: "A spell's place in the player's own order moving, with the same spells in the list, \
               re-sorts the rows on screen and not merely the panel's idea of them; moving it back \
               puts them back, and the gate closes again behind each change.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O567-ORDER"),
        station: "dereth-testkit::dat::magic::scenario_the_spellbook_rows_re_sort_when_a_spells_place_in_the_order_moves",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbook.removal.reaches-the-book-the-bar-and-the-copy-a-relog-reads",
        says: "When the shard takes a spell away, the page leaves the book the player reads and the \
               row leaves the bar it was sitting on, in the very next frame; the client then tells the \
               shard that the bar has lost it, so the packed copy of the bar it sends back no longer \
               carries the spell and the row does not come back at the next relog.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F15"),
        station: "dereth-testkit::dat::magic::scenario_a_removed_spell_leaves_the_book_the_bar_and_the_copy",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "spellbook.the-bar-keeps-a-spell-the-shipped-table-cannot-draw",
        says: "A spell the character really knows but which the shipped spell data has no row for \
               keeps its place on the bar: no icon and no name is invented for it, so it is drawn \
               nowhere, and it is not quietly taken off the bar either and nothing about it is sent to \
               the shard -- because what is asked is what the character knows and not what the panel \
               can draw.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-F15-PRUNE"),
        station: "dereth-testkit::dat::magic::scenario_the_bar_keeps_a_spell_the_table_cannot_draw",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vitae.lamp.lights-for-a-live-penalty-and-the-panel-says-how-much",
        says: "A vitae arriving while the player is in the world lights the lamp straight away, and \
               the panel behind it says what the penalty is and how much more experience he has to \
               earn before it lifts. A character with none is a known full-strength character with \
               the lamp dark, and a vitae is counted among neither the helpful nor the harmful \
               enchantments.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1120"),
        station: "dereth-testkit::dat::magic::scenario_a_live_vitae_lights_the_lamp_and_fills_the_panel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vitae.lamp.lights-only-for-a-real-penalty",
        says: "The vitae lamp is lit only while the character actually carries a vitae penalty: before \
               the character's own description has arrived, and for a character carrying none, it reads \
               exactly as it did when nothing could light it, and removing the penalty puts it back.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-O133"),
        station: "dereth-testkit::cpu::magic::scenario_the_vitae_lamp_lights_only_for_a_penalty",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "vitae.panel.follows-every-tick-and-goes-dark-at-full-strength",
        says: "Every arrival as the penalty wears off rewrites the panel the player has open, with no \
               click of his own and a different line each time; the lamp stays lit while any penalty \
               is left and goes dark the moment he is back to full strength, at which point the panel \
               says so. The entry itself stays where it is, because the shard replaces it as it wears \
               off rather than taking it away.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1120-TICK"),
        station: "dereth-testkit::dat::magic::scenario_the_panel_follows_every_tick_and_goes_dark_at_full_strength",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "vitae.penalty.takes-the-skill-down-leaves-the-row-plain-and-spares-the-attributes",
        says: "A vitae takes every skill down by its own share, and the Skills page still draws the \
               row in its plain colour -- a player carrying one is not shown a whole list of debuffs. \
               A real buff on top of the penalty is still drawn as a buff, the footer shows the \
               penalty as its own number, and an attribute does not move at all: the penalty is on \
               the skill and not on that.",
        since: RETAIL,
        evidence: Evidence::Private("AC-EVID-P1120-SKILLS"),
        station: "dereth-testkit::dat::magic::scenario_the_penalty_takes_the_skill_down_but_leaves_the_row_plain",
        tier: Tier::Dat,
    },
];
