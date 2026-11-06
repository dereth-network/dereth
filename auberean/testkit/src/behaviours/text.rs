//! Localized text: string references written to text, the string tables' meta language, numbers
//! in a language's format.

use super::{Behaviour, Evidence, Tier, RETAIL};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "text.data.every-entry-renders",
        says: "Every entry of every English string table of the end-of-retail data renders with \
               any plain values put in it, and English writes numbers in base 10 with five least \
               digits, no leading zeros, groups of three, a point and a comma, and its \
               hexadecimal numerals repeat the zero after the nine.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-TABLES"),
        station: "auberean-text::dat::tables::every_english_entry_renders_and_english_writes_numbers_as_its_rules_say",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "text.meta.capitals",
        says: "A caret before a value capitalizes its first letter after any white space or \
               punctuation; a backslash, a slash or an at sign counts as the first letter.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-META-CAPITALS"),
        station: "auberean-text::cpu::meta::a_capital_is_the_first_letter_after_spaces_and_punctuation",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.meta.choice-by-flags",
        says: "A choice writes the alternative whose flags best match its variable's (2 for each \
               flag the variable has and each negated flag it lacks, 1 for none), the last of the \
               best, and nothing when nothing scores.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-META-CHOICE-BY-FLAGS"),
        station: "auberean-text::cpu::meta::a_choice_writes_the_last_best_scoring_alternative_or_nothing",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.meta.escapes",
        says: "An escaped mark is text and stays escaped until the whole result is unescaped, and \
               an escaped q is a double quote.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-META-ESCAPES"),
        station: "auberean-text::cpu::meta::an_escaped_mark_is_text_and_stays_escaped",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.meta.gender-letters",
        says: "M and F in a value's flags stand for the language's male and female letters and \
               themselves.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-META-GENDER-LETTERS"),
        station: "auberean-text::cpu::meta::m_and_f_stand_for_the_languages_letters",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.meta.item-names",
        says: "A text marked as an item name writes its prefix, middle and suffix variables, then \
               in brackets the letters of their flags each part allows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-META-ITEM-NAMES"),
        station: "auberean-text::cpu::meta::an_item_name_carries_its_parts_letters",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.meta.numbered-choices",
        says: "A number label ties a choice to the variable with its number; an unnumbered choice \
               takes the unnumbered variable, else the one numbered -1, and a variable numbered \
               below zero is not written.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-META-NUMBERED-CHOICES"),
        station: "auberean-text::cpu::meta::a_numbered_choice_follows_its_variable_and_a_negative_variable_is_not_written",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.meta.player-name-tags",
        says: "A player's name becomes a value of the meta language: its own flags cut off, trimmed, \
               and [M] or [F] after it, with v for a name beginning with a vowel; a name ending in s \
               gets no s.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-NAME-TAGS"),
        station: "auberean-text::cpu::meta::a_player_name_carries_its_sex_and_a_leading_vowel_and_never_an_s",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.meta.render-errors",
        says: "A render error (two default alternatives, two variables with one number, two \
               unnumbered variables beside an unnumbered choice, a choice without its variable, \
               a syntax error) leaves the output as it was, or as written up to an error that \
               comes while writing; only the last error counts.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-META-RENDER-ERRORS"),
        station: "auberean-text::cpu::meta::an_error_leaves_the_output_as_it_was_or_written_up_to_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.meta.retrieve-flags",
        says: "A text splits into what comes before its flags, trimmed on the right, and the flags: \
               from its first [ (escapes not considered) to the next ] or the end; a [ first leaves \
               the text whole, a [ last leaves the flags as [.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-RETRIEVE-FLAGS"),
        station: "auberean-text::cpu::meta::a_text_splits_into_its_words_and_its_flags",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.meta.spaces",
        says: "A rendered text is trimmed on the left and each run of spaces and tabs becomes its \
               first, unless the text keeps its spaces.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-META-SPACES"),
        station: "auberean-text::cpu::meta::excess_spaces_collapse_unless_the_text_keeps_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.meta.value-flags",
        says: "A value is written without its flags; a value with none gets 1 when it is a number \
               the language takes as singular and + when it is a number not below zero, an empty \
               value counting as zero.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-META-VALUE-FLAGS"),
        station: "auberean-text::cpu::meta::a_value_is_written_without_its_flags_and_a_bare_number_gets_its_own",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.name.characters-by-language",
        says: "A character name may hold ASCII letters, apostrophes, hyphens and spaces, and its \
               language's extra letters: French è é à ç ù and their capitals (listing Ú, not Ù), \
               German ü ö ä and their capitals, Korean's syllables, Chinese's ideographs; Korean \
               also lets the input method's letters stand while a syllable is composed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NAME-CHARACTERS"),
        station: "auberean-text::cpu::names::a_name_holds_ascii_letters_and_marks_and_its_languages_extra_letters",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.name.check-censors",
        says: "A name is checked against its audience's banned patterns in their stored order \
               after being put in small letters and cut down to its letters, or to all but its \
               spaces, as the caller asks; the first pattern that matches refuses it and is named.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NAME-CENSOR"),
        station: "auberean-text::cpu::names::a_name_matching_an_audiences_pattern_is_refused_with_the_pattern",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.name.check-characters",
        says: "An empty name, or one with a character a name may not hold, is refused; when only \
               the markup is checked, such a character passes unless it is one of the percent \
               sign, the at sign or a square bracket.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NAME-CHECK"),
        station: "auberean-text::cpu::names::a_name_with_a_character_it_may_not_hold_is_refused_unless_only_markup_is_checked",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.name.filter-pattern",
        says: "A banned pattern's star stands for any run of characters, as the client matches it: \
               two stars together never match, and a failed try goes on from where it failed \
               rather than from just after where it began.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NAME-PATTERN"),
        station: "auberean-text::cpu::names::a_pattern_matches_as_the_client_matches_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.name.written-form",
        says: "A name is written trimmed of white space at both ends, with each run of spaces made \
               one only when there was nothing to trim, each letter capital after a character that \
               is not a letter and small after one that is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NAME-FORM"),
        station: "auberean-text::cpu::names::a_name_is_written_trimmed_with_a_capital_after_each_non_letter",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.numbers.format",
        says: "A number is written in its base with the language's numerals and separators, \
               rounded half up at its last decimal, grouped when asked, negative through the \
               language's pattern, padded with leading zeros when the table keeps them; nothing is \
               written for more than 20 decimals, a base below 2, or no language.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-NUMBERS"),
        station: "auberean-text::cpu::numbers::a_number_is_written_rounded_grouped_and_padded_as_the_table_says",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.place.map-coordinates-are-written-as-the-client-writes-them",
        says: "A land cell's map coordinates are written north or south first, then east or west, \
               each to a tenth with the C library's rounding (seventeen significant digits, then \
               a following 5 or more rounds up), the letter left out on the middle line; a cell \
               that is not a land cell of the world is <error>.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-MAP-COORDS"),
        station: "auberean-text::cpu::places::map_coordinates_are_written_as_the_client_writes_them",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.references.a-number-that-cannot-be-written-is-blank",
        says: "A number variable that cannot be written still sets its variable, to nothing, so the line around it is written without it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-NUMBER-FAILED-BLANK"),
        station: "auberean-text::cpu::references::a_number_that_cannot_be_written_is_blank_in_its_line",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.references.bucket",
        says: "A string reference's bucket among n is its literal text's hash (each unit added to the \
               hash times 16, the top four bits folded back in), else its table's and its token's \
               remainders added, modulo n.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-REFERENCE-BUCKET"),
        station: "auberean-text::cpu::references::a_references_bucket_is_its_literals_hash_or_its_ids_remainders",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.references.errors",
        says: "A string reference that cannot be written (no token or table, a missing table or \
               token, a variable its entry uses and it lacks) is the error message naming its \
               token and table, and its log line says why by number.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-REFERENCE-ERRORS"),
        station: "auberean-text::cpu::references::a_reference_that_cannot_be_written_is_its_error_message",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.references.literal-value",
        says: "A string reference's literal value is its literal text, flags cut off unless kept, \
               unescaped (empty for a reference that is no literal); a player's name literal takes \
               the player's flags.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-LITERAL-VALUE"),
        station: "auberean-text::cpu::references::a_literals_value_is_its_text_unescaped",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.references.private-tables",
        says: "A string reference names the private tables its entries send their tokens on to, for \
               the table asked about, its nested references included; an entry's text without \
               rendering is its non-empty strings joined.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-PRIVATE-TABLES"),
        station: "auberean-text::cpu::references::a_reference_names_the_private_tables_its_entries_use",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.references.valid",
        says: "A string reference is valid when it is a literal or its table holds its token, deeply \
               valid when every nested reference is valid too (a nested one naming no table is not); \
               one that writes without an error is on disk.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-REFERENCE-VALID"),
        station: "auberean-text::cpu::references::a_reference_is_valid_when_its_table_holds_its_token",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.references.variables",
        says: "A text variable is a nested literal holding the text escaped, narrow text read in the \
               English Windows code page; a later variable of an id replaces the earlier; a reference \
               can take another's variables or drop its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-REFERENCE-VARIABLES"),
        station: "auberean-text::cpu::references::a_text_variable_is_an_escaped_literal_and_replaces_one_of_its_id",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.references.write",
        says: "A string reference is written from its table entry (through the entry's private \
               table when it names one) with its variables, nested references taking and keeping \
               its table when they name none, flags cut off unless kept, then unescaped; a literal \
               is its own text.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TEXT-REFERENCES"),
        station: "auberean-text::cpu::references::a_reference_is_written_from_its_entry_with_its_variables",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.a-narrow-hash-ignoring-case-lowers-in-the-english-code-page",
        says: "A narrow string's hash ignoring case lowers each byte as the English locale lowers \
               it (the capitals of the English Windows code page, the multiplication sign kept), \
               then mixes it in as a signed character; it is never adjusted at all ones.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-HASH-FOLDED"),
        station: "auberean-primitives::cpu::strings::a_narrow_hash_ignoring_case_lowers_in_the_english_code_page",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.a-narrow-hash-takes-bytes-as-signed-characters",
        says: "A narrow string's hash shifts the hash four bits and adds each byte as a signed \
               character (a byte above 127 counts below zero), folding the top four bits into the \
               low byte; it ends at the first zero byte, and a key's bucket is its hash modulo the \
               bucket count.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-HASH"),
        station: "auberean-primitives::cpu::strings::a_narrow_hash_takes_bytes_as_signed_characters",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.a-packed-string-is-its-lengths-and-characters-padded-to-four",
        says: "A packed string takes a 16-bit length, a 32-bit length after it once it holds \
               65,535 characters or more, its characters (one byte or two each) and padding to a \
               multiple of four bytes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-PACK-SIZE"),
        station: "auberean-primitives::cpu::strings::a_packed_string_is_its_lengths_and_characters_padded_to_four",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.narrow-and-wide-text-convert-in-the-english-code-page",
        says: "Narrow text becomes wide in the English Windows code page, a byte a unit; wide text \
               becomes narrow in it, and when any unit has no byte there, the units below 128 are \
               kept and every other unit is written as its four lower-case hexadecimal digits in \
               angle brackets.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-CODE-PAGE"),
        station: "auberean-text::cpu::wide::narrow_and_wide_text_convert_in_the_english_code_page",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.narrow-comparison-by-bytes-or-lowered-bytes",
        says: "Comparing narrow strings keeping case answers -1, 0 or 1 by unsigned bytes; \
               ignoring case it answers the difference of the first two lowered bytes that differ; \
               both stop at a zero byte, and equality is a comparison of 0.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-COMPARE"),
        station: "auberean-primitives::cpu::strings::narrow_comparison_by_bytes_or_lowered_bytes",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.narrow-lowering-in-the-english-code-page",
        says: "Lowering narrow text lowers A to Z, the accented capitals but the multiplication \
               sign, and the code page's four other capitals, up to the first zero byte.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-LOWER"),
        station: "auberean-primitives::cpu::strings::narrow_lowering_in_the_english_code_page",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.narrow-replace-every-occurrence-from-the-left",
        says: "Replacing in narrow text replaces each occurrence of the pattern from the left, \
               searching on after each one replaced, and answers how many; an empty pattern never \
               ends in the client and is refused.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-REPLACE"),
        station: "auberean-primitives::cpu::strings::narrow_replace_every_occurrence_from_the_left",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.narrow-search-ignoring-case-forward-and-back",
        says: "Finding a part of narrow text ignoring case looks forward from the position to the \
               end, or backward from the position less the part's length to the start, each byte \
               lowered; an empty part is found where the search stands.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-SEARCH"),
        station: "auberean-primitives::cpu::strings::narrow_search_ignoring_case_forward_and_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.narrow-trimming-stops-at-a-zero-on-the-left-only",
        says: "Trimming narrow text drops the bytes of the set (space, line feed, carriage return, \
               tab and form feed unless the caller names others) from the left up to a zero byte, \
               and from the right, where a zero byte counts as one of the set.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-TRIM"),
        station: "auberean-primitives::cpu::strings::narrow_trimming_stops_at_a_zero_on_the_left_only",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.wide-equality-ignoring-case-lowers-every-letter",
        says: "Wide equality keeps case unit for unit, or lowers each unit as the wide C library \
               does, letters above 255 included; both stop at the first zero unit.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-WIDE-EQUAL"),
        station: "auberean-text::cpu::wide::wide_equality_ignoring_case_lowers_every_letter",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.wide-hexadecimal-and-float-appends",
        says: "A word is appended as its eight upper-case hexadecimal digits after a zero and an \
               x, a 64-bit word as sixteen, and a float with six decimals as the C library rounds \
               it, its infinities and not-a-numbers as that library writes them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-WIDE-APPEND"),
        station: "auberean-text::cpu::wide::wide_hexadecimal_and_float_appends",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.wide-search-ignoring-case-folds-the-low-byte",
        says: "The wide search ignoring case folds each unit by its low byte alone, lowered in the \
               English code page, so a letter above 255 matches whatever its low byte lowers to; \
               it looks forward to the end or backward to the start.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-WIDE-SEARCH"),
        station: "auberean-text::cpu::wide::wide_search_ignoring_case_folds_the_low_byte",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "text.strings.wide-whole-numbers-read-as-the-c-library-reads-them",
        says: "A wide whole number is read after white space and a sign, in base 16 after a zero \
               and an x, base 8 after a leading zero, else base 10, or in the base asked; signed \
               reads clamp out of range, unsigned ones saturate the magnitude and a minus sign \
               then counts back from all ones; with no digit nothing is read.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STR-WIDE-NUMBERS"),
        station: "auberean-text::cpu::wide::wide_whole_numbers_read_as_the_c_library_reads_them",
        tier: Tier::Cpu,
    },
];
