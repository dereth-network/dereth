// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Mutations/MutationCache.cs
//! Port of `Source/ACE.Server/Entity/Mutations/MutationCache.cs`.
//!
//! Compiles the mutation scripts (lootgen's `*.txt` and the tinkering recipes' `380000xx - *.txt`)
//! into [`MutationFilter`]s and caches them by name. ACE embeds the scripts in its assembly; here
//! they are compiled in (`scripts/`, copied byte-for-byte from ACE's `Entity/Mutations/**.txt` at
//! the harness commit, LF line endings) under the manifest resource names ACE gives them, in the
//! csproj's order, so `GetManifestResourceNames` and `GetManifestResourceStream` keep ACE's
//! behaviour ([`SCRIPTS`]).
//!
//! **Parsing notes.** ACE uses `Regex`, `Enum.TryParse`, `decimal` and `float.TryParse` (en-US).
//! The regexes here are small hand matchers with the same first-match semantics; `Enum.TryParse`
//! is [`enum_try_parse`] (case-sensitive names, a comma-separated name list, or an integer);
//! `decimal` is [`Decimal`], which is exact for the percentages the scripts hold, and converts to
//! `float` as .NET does (`(float)((double)mantissa / 10^scale)`).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use empyrean_common::dotnet::decimal::Decimal;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{
    AceEnum, EffectArgumentType, ImbuedEffectType, MutationEffectType, PropertyBool,
    PropertyDataId, PropertyFloat, PropertyInt, PropertyInt64, Skill, StatType, WieldRequirement,
};

use super::effect::Effect;
use super::effect_argument::EffectArgument;
use super::effect_list::EffectList;
use super::mutation::Mutation;
use super::mutation_filter::MutationFilter;
use super::mutation_outcome::MutationOutcome;
use crate::World;

/// The mutable static state of ACE's `MutationCache`, held as a field of `World`:
/// `tSysMutationFilters`, the compiled scripts by name (a `ConcurrentDictionary` in ACE). Compiling
/// is a pure function of the embedded scripts, so the cache is filled through a shared `&World`.
#[derive(Debug, Default)]
pub struct MutationCacheState {
    // ACE: MutationCache.tSysMutationFilters
    pub t_sys_mutation_filters: Mutex<HashMap<String, Arc<MutationFilter>>>,
}

/// For lootgen -- custom filenames (`"Casters.caster.txt"`).
// ACE: MutationCache.GetMutation
pub fn get_mutation(w: &World, filename: &str) -> Option<Arc<MutationFilter>> {
    let cache = &w.mutation_cache.t_sys_mutation_filters;

    if let Some(t_sys_mutation_filter) = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(filename)
    {
        return Some(Arc::clone(t_sys_mutation_filter));
    }

    let t_sys_mutation_filter = build_mutation(filename).map(Arc::new);

    if let Some(filter) = &t_sys_mutation_filter {
        // ConcurrentDictionary.TryAdd: the first one stored wins
        cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(filename.to_owned())
            .or_insert_with(|| Arc::clone(filter));
    }
    t_sys_mutation_filter
}

/// For recipes -- mutation script id + custom filename. Cached under the id's decimal string.
pub fn get_mutation_by_id(w: &World, mutation_id: u32) -> Option<Arc<MutationFilter>> {
    let mutation_id_str = mutation_id.to_string();
    let cache = &w.mutation_cache.t_sys_mutation_filters;

    if let Some(t_sys_mutation_filter) = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&mutation_id_str)
    {
        return Some(Arc::clone(t_sys_mutation_filter));
    }

    let t_sys_mutation_filter = build_mutation_by_id(mutation_id).map(Arc::new);

    if let Some(filter) = &t_sys_mutation_filter {
        cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(mutation_id_str)
            .or_insert_with(|| Arc::clone(filter));
    }
    t_sys_mutation_filter
}

/// `BuildMutation(uint mutationId)`: the script named by `mutationIdToFilename`.
// ACE: MutationCache.BuildMutation
fn build_mutation_by_id(mutation_id: u32) -> Option<MutationFilter> {
    let Some(filename) = MUTATION_ID_TO_FILENAME.get(&mutation_id) else {
        log::error!("MutationCache.BuildMutation({mutation_id:08X}) - embedded resource not found");
        return None;
    };

    build_mutation(filename)
}

/// Compiles one script. `pub` so tests and tools can compile a script without a `World`.
pub fn build_mutation(filename: &str) -> Option<MutationFilter> {
    let Some(lines) = read_script(filename) else {
        log::error!("MutationCache.BuildMutation({filename}) - embedded resource not found");
        return None;
    };

    let mut prev_mutation_line: Option<String> = None;
    let mut mutation_line: Option<String> = None;

    let mut mutation_filter = MutationFilter::default();
    // ACE's `mutation`, `outcome` and `effectList` locals are references into the filter; here
    // they are index paths to the object each last pointed at.
    let mut mutation: Option<usize> = None;
    let mut outcome: Option<(usize, usize)> = None;
    let mut effect_list: Option<(usize, usize, usize)> = None;

    let mut total_chance = Decimal::ZERO;

    for line in lines {
        let mut line = line;

        if let Some(comment_idx) = line.find("//") {
            line.truncate(comment_idx);
        }

        if contains_ignore_case(&line, "Mutation #") {
            prev_mutation_line = mutation_line.take();
            mutation_line = Some(line);
            continue;
        }

        if contains_ignore_case(&line, "Tier chances") {
            if let Some(o) = outcome {
                let last = outcome_at(&mutation_filter, o)
                    .effect_lists
                    .last()
                    .map(|l| l.chance);
                if last.expect("InvalidOperationException: Sequence contains no elements") != 1.0 {
                    log::error!(
                        "MutationCache.BuildMutation({filename}) - {} total {}, expected 1.0",
                        prev_mutation_line.as_deref().unwrap_or_default(),
                        last.unwrap_or_default()
                    );
                }
            }

            mutation_filter.mutations.push(Mutation::default());
            let m = mutation_filter.mutations.len() - 1;
            mutation = Some(m);

            let tier_pieces = line.split(',');

            for tier_piece in tier_pieces {
                match first_run(tier_piece, |c| c.is_ascii_digit() || c == '.')
                    .and_then(|m| m.parse::<f32>().ok())
                {
                    Some(tier_chance) => mutation_filter.mutations[m].chances.push(tier_chance),
                    None => {
                        log::error!(
                            "MutationCache.BuildMutation({filename}) - couldn't parse tier chances for {}: {tier_piece}",
                            mutation_line.as_deref().unwrap_or_default()
                        );
                        mutation_filter.mutations[m].chances.push(0.0);
                    }
                }
            }

            mutation_filter.mutations[m]
                .outcomes
                .push(MutationOutcome::default());
            outcome = Some((m, mutation_filter.mutations[m].outcomes.len() - 1));

            total_chance = Decimal::ZERO;

            continue;
        }

        if contains_ignore_case(&line, "- Chance") {
            if total_chance.cmp_value(Decimal::ONE) != std::cmp::Ordering::Less {
                if total_chance.cmp_value(Decimal::ONE) == std::cmp::Ordering::Greater {
                    log::error!(
                        "MutationCache.BuildMutation({filename}) - {} total {}, expected 1.0",
                        mutation_line.as_deref().unwrap_or_default(),
                        total_chance.dotnet_string()
                    );
                }

                let m = mutation.expect("NullReferenceException: Mutation is null");
                mutation_filter.mutations[m]
                    .outcomes
                    .push(MutationOutcome::default());
                outcome = Some((m, mutation_filter.mutations[m].outcomes.len() - 1));

                total_chance = Decimal::ZERO;
            }

            let mut new_effect_list = EffectList::default();

            match first_run(&line, |c| c.is_ascii_digit() || c == '.').and_then(Decimal::parse) {
                Some(chance) => {
                    total_chance = total_chance + chance.div_100();

                    new_effect_list.chance = total_chance.to_f32();
                }
                None => {
                    log::error!(
                        "MutationCache.BuildMutation({filename}) - couldn't parse {line} for {}",
                        mutation_line.as_deref().unwrap_or_default()
                    );
                }
            }

            let o = outcome.expect("NullReferenceException: MutationOutcome is null");
            let lists = &mut outcome_at_mut(&mut mutation_filter, o).effect_lists;
            lists.push(new_effect_list);
            effect_list = Some((o.0, o.1, lists.len() - 1));
            continue;
        }

        if !line.contains('=') {
            continue;
        }

        let mut effect = Effect {
            r#type: get_mutation_effect_type(&line),
            ..Effect::default()
        };

        let first_operator = get_first_operator(effect.r#type).unwrap_or_default();

        let pieces: Vec<&str> = line.splitn(2, first_operator).collect();

        if pieces.len() != 2 {
            log::error!("MutationCache.BuildMutation({filename}) - couldn't parse {line}");
            continue;
        }

        let piece0 = trim(pieces[0]);
        let piece1 = trim(pieces[1]);

        let _first_stat_type = get_stat_type(piece0);

        /*if (firstStatType == StatType.Undef)
        {
            log.Error($"MutationCache.BuildMutation({filename}) - couldn't determine StatType for {pieces[0]} in {line}");
            continue;
        }*/

        effect.quality = Some(parse_effect_argument(filename, &effect, piece0));

        let has_second_operator = has_second_operator(effect.r#type);

        if has_second_operator {
            let second_operator = get_second_operator(effect.r#type).unwrap_or_default();

            let subpieces: Vec<&str> = piece1.splitn(2, second_operator).collect();

            if subpieces.len() != 2 {
                log::error!("MutationCache.BuildMutation({filename}) - couldn't parse {line}");
                continue;
            }

            let subpiece0 = trim(subpieces[0]);
            let subpiece1 = trim(subpieces[1]);

            effect.arg1 = Some(parse_effect_argument(filename, &effect, subpiece0));
            effect.arg2 = Some(parse_effect_argument(filename, &effect, subpiece1));
        } else {
            effect.arg1 = Some(parse_effect_argument(filename, &effect, piece1));
        }

        let (m, o, l) = effect_list.expect("NullReferenceException: EffectList is null");
        mutation_filter.mutations[m].outcomes[o].effect_lists[l]
            .effects
            .push(effect);
    }

    if let Some(o) = outcome {
        let last = outcome_at(&mutation_filter, o)
            .effect_lists
            .last()
            .map(|l| l.chance);
        if last.expect("InvalidOperationException: Sequence contains no elements") != 1.0 {
            log::error!(
                "MutationCache.BuildMutation({filename}) - {} total {}, expected 1.0",
                mutation_line.as_deref().unwrap_or_default(),
                last.unwrap_or_default()
            );
        }
    }

    // scripts take about ~2ms to compile
    //Console.WriteLine($"Compiled {filename} in {timer.Elapsed.TotalMilliseconds}ms");

    Some(mutation_filter)
}

fn outcome_at(filter: &MutationFilter, (m, o): (usize, usize)) -> &MutationOutcome {
    &filter.mutations[m].outcomes[o]
}

fn outcome_at_mut(filter: &mut MutationFilter, (m, o): (usize, usize)) -> &mut MutationOutcome {
    &mut filter.mutations[m].outcomes[o]
}

/// Parses one operand into an argument whose type [`get_effect_argument_type`] decides.
// ACE: MutationCache.ParseEffectArgument
fn parse_effect_argument(filename: &str, effect: &Effect, operand: &str) -> EffectArgument {
    let mut effect_argument = EffectArgument {
        r#type: get_effect_argument_type(effect, operand),
        ..EffectArgument::default()
    };

    match effect_argument.r#type {
        EffectArgumentType::Int => {
            if let Ok(v) = operand.parse::<i32>() {
                effect_argument.int_val = v;
            } else {
                // int.TryParse sets the out value to 0 on failure
                effect_argument.int_val = 0;

                let quality_is_imbued_effect = effect.quality.as_ref().is_some_and(|q| {
                    q.stat_type == StatType::Int
                        && q.stat_idx == i32::from(PropertyInt::ImbuedEffect.0)
                });

                if let Some(imbued_effect_type) = quality_is_imbued_effect
                    .then(|| enum_try_parse::<ImbuedEffectType>(operand))
                    .flatten()
                {
                    effect_argument.int_val = imbued_effect_type.cs_cast();
                } else if let Some(wield_requirement) = enum_try_parse::<WieldRequirement>(operand)
                {
                    effect_argument.int_val = wield_requirement.cs_cast();
                } else if let Some(skill) = enum_try_parse::<Skill>(operand) {
                    effect_argument.int_val = skill.cs_cast();
                } else {
                    log::error!(
                        "MutationCache.BuildMutation({filename}) - couldn't parse IntVal {operand}"
                    );
                }
            }
        }

        EffectArgumentType::Int64 => {
            if let Ok(v) = operand.parse::<i64>() {
                effect_argument.long_val = v;
            } else {
                log::error!(
                    "MutationCache.BuildMutation({filename}) - couldn't parse Int64Val {operand}"
                );
            }
        }

        EffectArgumentType::Double => {
            if let Ok(v) = operand.parse::<f64>() {
                effect_argument.double_val = v;
            } else {
                log::error!(
                    "MutationCache.BuildMutation({filename}) - couldn't parse DoubleVal {operand}"
                );
            }
        }

        EffectArgumentType::Quality => {
            effect_argument.stat_type = get_stat_type(operand);

            match effect_argument.stat_type {
                StatType::Int => match enum_try_parse::<PropertyInt>(operand) {
                    Some(prop_int) => effect_argument.stat_idx = prop_int.cs_cast(),
                    None => log::error!("MutationCache.BuildMutation({filename}) - couldn't parse PropertyInt.{operand}"),
                },

                StatType::Int64 => match enum_try_parse::<PropertyInt64>(operand) {
                    Some(prop_int64) => effect_argument.stat_idx = prop_int64.cs_cast(),
                    None => log::error!("MutationCache.BuildMutation({filename}) - couldn't parse PropertyInt64.{operand}"),
                },

                StatType::Float => match enum_try_parse::<PropertyFloat>(operand) {
                    Some(prop_float) => effect_argument.stat_idx = prop_float.cs_cast(),
                    None => log::error!("MutationCache.BuildMutation({filename}) - couldn't parse PropertyFloat.{operand}"),
                },

                StatType::Bool => match enum_try_parse::<PropertyBool>(operand) {
                    Some(prop_bool) => effect_argument.stat_idx = prop_bool.cs_cast(),
                    None => log::error!("MutationCache.BuildMutation({filename}) - couldn't parse PropertyBool.{operand}"),
                },

                StatType::DataID => match enum_try_parse::<PropertyDataId>(operand) {
                    Some(prop_did) => effect_argument.stat_idx = prop_did.cs_cast(),
                    None => log::error!("MutationCache.BuildMutation({filename}) - couldn't parse PropertyBool.{operand}"),
                },

                _ => log::error!("MutationCache.BuildMutation({filename}) - unknown PropertyType.{operand}"),
            }
        }

        EffectArgumentType::Random => match match_random(operand) {
            Some((min, max)) => match (min.parse::<f32>(), max.parse::<f32>()) {
                (Ok(min), Ok(max)) => {
                    effect_argument.min_val = min;
                    effect_argument.max_val = max;
                }
                (min, _) => {
                    // float.TryParse sets the out value to 0 on failure; the second is only
                    // attempted when the first succeeded
                    effect_argument.min_val = min.unwrap_or(0.0);
                    effect_argument.max_val = 0.0;
                    log::error!(
                        "MutationCache.BuildMutation({filename}) - couldn't parse {operand}"
                    );
                }
            },
            None => {
                log::error!("MutationCache.BuildMutation({filename}) - couldn't parse {operand}")
            }
        },

        EffectArgumentType::Variable => match match_variable(operand).map(str::parse::<i32>) {
            Some(Ok(v)) => effect_argument.int_val = v,
            _ => log::error!("MutationCache.BuildMutation({filename}) - couldn't parse {operand}"),
        },

        _ => log::error!(
            "MutationCache.BuildMutation({filename}) - unknown EffectArgumentType from {operand}"
        ),
    }
    effect_argument
}

/// The effect type from the operators on the line, tested in ACE's order.
// ACE: MutationCache.GetMutationEffectType
#[must_use]
pub fn get_mutation_effect_type(line: &str) -> MutationEffectType {
    if line.contains("+=") {
        if line.contains('*') {
            MutationEffectType::AddMultiply
        } else if line.contains('/') {
            MutationEffectType::AddDivide
        } else {
            MutationEffectType::Add
        }
    } else if line.contains("-=") {
        if line.contains('*') {
            MutationEffectType::SubtractMultiply
        } else if line.contains('/') {
            MutationEffectType::SubtractDivide
        } else {
            MutationEffectType::Subtract
        }
    } else if line.contains("*=") {
        MutationEffectType::Multiply
    } else if line.contains("/=") {
        MutationEffectType::Divide
    } else if line.contains('+') {
        MutationEffectType::AssignAdd
    } else if line.contains(" - ") {
        MutationEffectType::AssignSubtract
    } else if line.contains('*') {
        MutationEffectType::AssignMultiply
    } else if line.contains('/') {
        MutationEffectType::AssignDivide
    } else if line.contains("(>=") {
        MutationEffectType::AtLeastAdd
    } else if line.contains("(<=") {
        MutationEffectType::AtMostSubtract
    } else {
        MutationEffectType::Assign
    }
}

// ACE: MutationCache.GetFirstOperator
fn get_first_operator(effect_type: MutationEffectType) -> Option<&'static str> {
    match effect_type {
        MutationEffectType::Assign
        | MutationEffectType::AssignAdd
        | MutationEffectType::AssignSubtract
        | MutationEffectType::AssignMultiply
        | MutationEffectType::AssignDivide => Some("="),
        MutationEffectType::Add
        | MutationEffectType::AddMultiply
        | MutationEffectType::AddDivide => Some("+="),
        MutationEffectType::Subtract
        | MutationEffectType::SubtractMultiply
        | MutationEffectType::SubtractDivide => Some("-="),
        MutationEffectType::Multiply => Some("*="),
        MutationEffectType::Divide => Some("/="),
        MutationEffectType::AtLeastAdd => Some("(>="),
        MutationEffectType::AtMostSubtract => Some("(<="),
        _ => None,
    }
}

/// The property type an operand names: the first of PropertyInt, PropertyInt64, PropertyFloat,
/// PropertyBool and PropertyDataId that `Enum.TryParse` accepts it for.
// ACE: MutationCache.GetStatType
#[must_use]
pub fn get_stat_type(operand: &str) -> StatType {
    if enum_try_parse::<PropertyInt>(operand).is_some() {
        StatType::Int
    } else if enum_try_parse::<PropertyInt64>(operand).is_some() {
        StatType::Int64
    } else if enum_try_parse::<PropertyFloat>(operand).is_some() {
        StatType::Float
    } else if enum_try_parse::<PropertyBool>(operand).is_some() {
        StatType::Bool
    } else if enum_try_parse::<PropertyDataId>(operand).is_some() {
        StatType::DataID
    } else {
        StatType::Undef
    }
}

// ACE: MutationCache.GetEffectArgumentType
#[must_use]
pub fn get_effect_argument_type(effect: &Effect, operand: &str) -> EffectArgumentType {
    if is_number(operand)
        || enum_try_parse::<WieldRequirement>(operand).is_some()
        || enum_try_parse::<Skill>(operand).is_some()
        || enum_try_parse::<ImbuedEffectType>(operand).is_some()
    {
        if operand.contains('.') {
            EffectArgumentType::Double
        } else if effect
            .quality
            .as_ref()
            .is_some_and(|q| q.stat_type == StatType::Int64)
        {
            EffectArgumentType::Int64
        } else {
            EffectArgumentType::Int
        }
    } else if get_stat_type(operand) != StatType::Undef {
        EffectArgumentType::Quality
    } else if starts_with_ignore_case(operand, "Random") {
        EffectArgumentType::Random
    } else if starts_with_ignore_case(operand, "Variable") {
        EffectArgumentType::Variable
    } else {
        EffectArgumentType::Invalid
    }
}

/// `Regex.Match(operand, @"([\d.-]+)")` matched the whole operand: it is non-empty and made only
/// of digits, `.` and `-`.
// ACE: MutationCache.IsNumber
#[must_use]
pub fn is_number(operand: &str) -> bool {
    first_run(operand, is_number_char).is_some_and(|m| m == operand)
}

fn is_number_char(c: char) -> bool {
    c.is_ascii_digit() || c == '.' || c == '-'
}

// ACE: MutationCache.HasSecondOperator
#[must_use]
pub fn has_second_operator(effect_type: MutationEffectType) -> bool {
    matches!(
        effect_type,
        MutationEffectType::AssignAdd
            | MutationEffectType::AssignSubtract
            | MutationEffectType::AssignMultiply
            | MutationEffectType::AssignDivide
            | MutationEffectType::AddMultiply
            | MutationEffectType::AddDivide
            | MutationEffectType::SubtractMultiply
            | MutationEffectType::SubtractDivide
            | MutationEffectType::AtLeastAdd
            | MutationEffectType::AtMostSubtract
    )
}

// ACE: MutationCache.GetSecondOperator
#[must_use]
pub fn get_second_operator(effect_type: MutationEffectType) -> Option<&'static str> {
    match effect_type {
        MutationEffectType::AssignAdd => Some("+"),
        MutationEffectType::AssignSubtract => Some("-"),
        MutationEffectType::AssignMultiply
        | MutationEffectType::AddMultiply
        | MutationEffectType::SubtractMultiply => Some("*"),
        MutationEffectType::AssignDivide
        | MutationEffectType::AddDivide
        | MutationEffectType::SubtractDivide => Some("/"),
        MutationEffectType::AtLeastAdd => Some("? add : set)"),
        MutationEffectType::AtMostSubtract => Some("? sub : set)"),
        _ => None,
    }
}

const PREFIX: &str = "ACE.Server.Entity.Mutations.";

/// The script's lines as `StreamReader.ReadLine` returns them (`\r\n`, `\n` or `\r` ends a
/// line), or `None` when no embedded resource has that name.
// ACE: MutationCache.ReadScript
fn read_script(filename: &str) -> Option<Vec<String>> {
    let resource_name = format!("{PREFIX}{}", filename.replace('/', "."));

    let stream = SCRIPTS
        .iter()
        .find(|(name, _)| *name == resource_name)
        .map(|(_, text)| *text)?;

    Some(read_lines(stream))
}

/// `while (!reader.EndOfStream) lines.Add(reader.ReadLine());`
fn read_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        match rest.find(['\r', '\n']) {
            Some(i) => {
                lines.push(rest[..i].to_owned());
                let skip = if rest[i..].starts_with("\r\n") { 2 } else { 1 };
                rest = &rest[i + skip..];
            }
            None => {
                lines.push(rest.to_owned());
                rest = "";
            }
        }
    }
    lines
}

/// `mutationIdToFilename`, filled by the static constructor.
static MUTATION_ID_TO_FILENAME: LazyLock<HashMap<u32, String>> =
    LazyLock::new(cache_resource_names);

/// The static constructor's `CacheResourceNames`: every resource whose short name (the piece
/// before the extension) holds eight upper-case hex digits maps that id to its filename.
// ACE: MutationCache.MutationCache, MutationCache.CacheResourceNames
fn cache_resource_names() -> HashMap<u32, String> {
    let mut mutation_id_to_filename = HashMap::new();

    for (resource_name, _) in SCRIPTS {
        let pieces: Vec<&str> = resource_name.split('.').collect();

        if pieces.len() < 2 {
            log::error!(
                "MutationCache.CacheResourceNames() - unknown resource format {resource_name}"
            );
            continue;
        }
        let short_name = pieces[pieces.len() - 2];

        if let Some(mutation_id) = match_hex8(short_name) {
            mutation_id_to_filename.insert(mutation_id, resource_name.replace(PREFIX, ""));
        }
    }
    mutation_id_to_filename
}

/// The manifest resource names, in ACE's order (`Assembly.GetManifestResourceNames`).
pub fn manifest_resource_names() -> impl Iterator<Item = &'static str> {
    SCRIPTS.iter().map(|(name, _)| *name)
}

/// The id -> filename map the static constructor builds (for tests).
#[must_use]
pub fn mutation_id_to_filename(mutation_id: u32) -> Option<&'static str> {
    MUTATION_ID_TO_FILENAME
        .get(&mutation_id)
        .map(String::as_str)
}

// ---- .NET helpers (not ported) ----

/// `string.Contains(value, StringComparison.OrdinalIgnoreCase)` for ASCII needles.
fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

/// `string.StartsWith(value, StringComparison.OrdinalIgnoreCase)`.
fn starts_with_ignore_case(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// `string.Trim()`: .NET trims Unicode white space, as `str::trim` does.
fn trim(s: &str) -> &str {
    s.trim()
}

/// The first `([class]+)` match: the leftmost run of characters in the class.
fn first_run(s: &str, class: impl Fn(char) -> bool) -> Option<&str> {
    let start = s.find(&class)?;
    let len = s[start..]
        .find(|c: char| !class(c))
        .unwrap_or(s.len() - start);
    Some(&s[start..start + len])
}

/// `Regex.Match(operand, @"Random\(([\d.-]+), ([\d.-]+)\)")`: the two captures of the first match.
fn match_random(operand: &str) -> Option<(&str, &str)> {
    let mut from = 0;
    while let Some(i) = operand[from..].find("Random(") {
        let at = from + i + "Random(".len();
        let rest = &operand[at..];
        let a_len = rest
            .find(|c: char| !is_number_char(c))
            .unwrap_or(rest.len());
        if a_len > 0 {
            if let Some(after) = rest[a_len..].strip_prefix(", ") {
                let b_len = after
                    .find(|c: char| !is_number_char(c))
                    .unwrap_or(after.len());
                if b_len > 0 && after[b_len..].starts_with(')') {
                    return Some((&rest[..a_len], &after[..b_len]));
                }
            }
        }
        from = from + i + 1;
    }
    None
}

/// `Regex.Match(operand, @"\[(\d+)\]")`: the capture of the first match.
fn match_variable(operand: &str) -> Option<&str> {
    let mut from = 0;
    while let Some(i) = operand[from..].find('[') {
        let rest = &operand[from + i + 1..];
        let len = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if len > 0 && rest[len..].starts_with(']') {
            return Some(&rest[..len]);
        }
        from = from + i + 1;
    }
    None
}

/// `Regex.Match(shortName, @"([0-9A-F]{8})")` then `uint.TryParse(.., HexNumber)`.
fn match_hex8(s: &str) -> Option<u32> {
    let bytes = s.as_bytes();
    (0..bytes.len().saturating_sub(7)).find_map(|i| {
        let window = &bytes[i..i + 8];
        window
            .iter()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(b))
            .then(|| u32::from_str_radix(std::str::from_utf8(window).ok()?, 16).ok())
            .flatten()
    })
}

/// `Enum.TryParse<TEnum>(value, out _)` (case-sensitive): an integer (a leading digit, `-` or `+`),
/// or one or more comma-separated member names, OR-ed. Returns the value, sign-extended.
#[must_use]
pub fn enum_try_parse<E: AceEnum>(value: &str) -> Option<i64> {
    enum_try_parse_by(value, |name| {
        E::from_name(name).map(|e| e.key().cast_signed())
    })
}

/// [`enum_try_parse`] with the member lookup supplied (for enums outside empyrean-entity).
pub fn enum_try_parse_by(value: &str, from_name: impl Fn(&str) -> Option<i64>) -> Option<i64> {
    let value = value.trim();
    let first = value.chars().next()?;
    if first.is_ascii_digit() || first == '-' || first == '+' {
        return value.parse::<i64>().ok();
    }
    let mut result: i64 = 0;
    for name in value.split(',') {
        result |= from_name(name.trim())?;
    }
    Some(result)
}

/// The embedded scripts: (manifest resource name, text), in the csproj's `EmbeddedResource` order.
static SCRIPTS: &[(&str, &str)] = &[
    ("ACE.Server.Entity.Mutations.ArmorLevel.armor_level_extremity.txt", include_str!("scripts/ArmorLevel/armor_level_extremity.txt")),
    ("ACE.Server.Entity.Mutations.ArmorLevel.armor_level_non_extremity.txt", include_str!("scripts/ArmorLevel/armor_level_non_extremity.txt")),
    ("ACE.Server.Entity.Mutations.ArmorLevel.covenant_armor.txt", include_str!("scripts/ArmorLevel/covenant_armor.txt")),
    ("ACE.Server.Entity.Mutations.ArmorLevel.covenant_shield.txt", include_str!("scripts/ArmorLevel/covenant_shield.txt")),
    ("ACE.Server.Entity.Mutations.ArmorLevel.olthoi_armor.txt", include_str!("scripts/ArmorLevel/olthoi_armor.txt")),
    ("ACE.Server.Entity.Mutations.ArmorLevel.olthoi_shield.txt", include_str!("scripts/ArmorLevel/olthoi_shield.txt")),
    ("ACE.Server.Entity.Mutations.ArmorLevel.shield_level.txt", include_str!("scripts/ArmorLevel/shield_level.txt")),
    ("ACE.Server.Entity.Mutations.Casters.caster.txt", include_str!("scripts/Casters/caster.txt")),
    ("ACE.Server.Entity.Mutations.Casters.caster_elemental.txt", include_str!("scripts/Casters/caster_elemental.txt")),
    ("ACE.Server.Entity.Mutations.Casters.caster_non_elemental.txt", include_str!("scripts/Casters/caster_non_elemental.txt")),
    ("ACE.Server.Entity.Mutations.Casters.weapon_defense.txt", include_str!("scripts/Casters/weapon_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.heavy_axe.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/heavy_axe.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.heavy_dagger.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/heavy_dagger.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.heavy_dagger_ms.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/heavy_dagger_ms.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.heavy_mace.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/heavy_mace.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.heavy_spear.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/heavy_spear.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.heavy_staff.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/heavy_staff.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.heavy_sword.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/heavy_sword.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.heavy_sword_ms.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/heavy_sword_ms.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.heavy_unarmed.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/heavy_unarmed.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.light_finesse_axe.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/light_finesse_axe.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.light_finesse_dagger.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/light_finesse_dagger.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.light_finesse_dagger_ms.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/light_finesse_dagger_ms.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.light_finesse_mace.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/light_finesse_mace.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.light_finesse_mace_jitte.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/light_finesse_mace_jitte.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.light_finesse_spear.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/light_finesse_spear.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.light_finesse_staff.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/light_finesse_staff.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.light_finesse_sword.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/light_finesse_sword.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.light_finesse_sword_ms.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/light_finesse_sword_ms.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.light_finesse_unarmed.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/light_finesse_unarmed.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.two_handed_cleaver.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/two_handed_cleaver.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.Damage_WieldDifficulty_DamageVariance.two_handed_spear.txt", include_str!("scripts/MeleeWeapons/Damage_WieldDifficulty_DamageVariance/two_handed_spear.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.axe_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/axe_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.dagger_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/dagger_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.mace_jitte_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/mace_jitte_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.mace_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/mace_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.spear_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/spear_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.staff_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/staff_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.sword_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/sword_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.two_handed_axe_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/two_handed_axe_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.two_handed_mace_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/two_handed_mace_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.two_handed_spear_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/two_handed_spear_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.two_handed_sword_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/two_handed_sword_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MeleeWeapons.WeaponOffense_WeaponDefense.unarmed_offense_defense.txt", include_str!("scripts/MeleeWeapons/WeaponOffense_WeaponDefense/unarmed_offense_defense.txt")),
    ("ACE.Server.Entity.Mutations.MissileWeapons.atlatl_elemental.txt", include_str!("scripts/MissileWeapons/atlatl_elemental.txt")),
    ("ACE.Server.Entity.Mutations.MissileWeapons.atlatl_non_elemental.txt", include_str!("scripts/MissileWeapons/atlatl_non_elemental.txt")),
    ("ACE.Server.Entity.Mutations.MissileWeapons.bow_elemental.txt", include_str!("scripts/MissileWeapons/bow_elemental.txt")),
    ("ACE.Server.Entity.Mutations.MissileWeapons.bow_non_elemental.txt", include_str!("scripts/MissileWeapons/bow_non_elemental.txt")),
    ("ACE.Server.Entity.Mutations.MissileWeapons.crossbow_elemental.txt", include_str!("scripts/MissileWeapons/crossbow_elemental.txt")),
    ("ACE.Server.Entity.Mutations.MissileWeapons.crossbow_non_elemental.txt", include_str!("scripts/MissileWeapons/crossbow_non_elemental.txt")),
    ("ACE.Server.Entity.Mutations.MissileWeapons.weapon_defense.txt", include_str!("scripts/MissileWeapons/weapon_defense.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800000F - Stamp.txt", include_str!("scripts/Recipes/3800000F - Stamp.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000011 - Steel.txt", include_str!("scripts/Recipes/38000011 - Steel.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000012 - Armoredillo Hide.txt", include_str!("scripts/Recipes/38000012 - Armoredillo Hide.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000013 - Marble.txt", include_str!("scripts/Recipes/38000013 - Marble.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000014 - Wool.txt", include_str!("scripts/Recipes/38000014 - Wool.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000015 - Reedshark Hide.txt", include_str!("scripts/Recipes/38000015 - Reedshark Hide.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000016 - Ceramic.txt", include_str!("scripts/Recipes/38000016 - Ceramic.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000017 - Alabaster.txt", include_str!("scripts/Recipes/38000017 - Alabaster.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000018 - Bronze.txt", include_str!("scripts/Recipes/38000018 - Bronze.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000019 - Linen.txt", include_str!("scripts/Recipes/38000019 - Linen.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800001A - Iron.txt", include_str!("scripts/Recipes/3800001A - Iron.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800001B - Mahogany.txt", include_str!("scripts/Recipes/3800001B - Mahogany.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800001C - Granite.txt", include_str!("scripts/Recipes/3800001C - Granite.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800001D - Oak.txt", include_str!("scripts/Recipes/3800001D - Oak.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800001E - Pine.txt", include_str!("scripts/Recipes/3800001E - Pine.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800001F - Gold.txt", include_str!("scripts/Recipes/3800001F - Gold.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000020 - Brass.txt", include_str!("scripts/Recipes/38000020 - Brass.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000021 - Velvet.txt", include_str!("scripts/Recipes/38000021 - Velvet.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000023 - Black Opal.txt", include_str!("scripts/Recipes/38000023 - Black Opal.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000024 - Fire Opal.txt", include_str!("scripts/Recipes/38000024 - Fire Opal.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000025 - Sunstone.txt", include_str!("scripts/Recipes/38000025 - Sunstone.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800002E - Opal.txt", include_str!("scripts/Recipes/3800002E - Opal.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800002F - Moonstone.txt", include_str!("scripts/Recipes/3800002F - Moonstone.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000034 - Silver.txt", include_str!("scripts/Recipes/38000034 - Silver.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000035 - Copper.txt", include_str!("scripts/Recipes/38000035 - Copper.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000036 - Silk.txt", include_str!("scripts/Recipes/38000036 - Silk.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000037 - Zircon.txt", include_str!("scripts/Recipes/38000037 - Zircon.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000038 - Peridot.txt", include_str!("scripts/Recipes/38000038 - Peridot.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000039 - Yellow Topaz.txt", include_str!("scripts/Recipes/38000039 - Yellow Topaz.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800003A - Emerald.txt", include_str!("scripts/Recipes/3800003A - Emerald.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800003B - White Sapphire.txt", include_str!("scripts/Recipes/3800003B - White Sapphire.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800003C - Aquamarine.txt", include_str!("scripts/Recipes/3800003C - Aquamarine.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800003D - Jet.txt", include_str!("scripts/Recipes/3800003D - Jet.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800003E - Red Garnet.txt", include_str!("scripts/Recipes/3800003E - Red Garnet.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800003F - Black Garnet.txt", include_str!("scripts/Recipes/3800003F - Black Garnet.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000040 - Imperial Topaz.txt", include_str!("scripts/Recipes/38000040 - Imperial Topaz.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000041 - Cantrip.txt", include_str!("scripts/Recipes/38000041 - Cantrip.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000042 - HeritageGroup.txt", include_str!("scripts/Recipes/38000042 - HeritageGroup.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000043 - Leather.txt", include_str!("scripts/Recipes/38000043 - Leather.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.38000046 - Fetish of the Dark Idols.txt", include_str!("scripts/Recipes/38000046 - Fetish of the Dark Idols.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800004B - Green Garnet.txt", include_str!("scripts/Recipes/3800004B - Green Garnet.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.3800004E - Sandstone.txt", include_str!("scripts/Recipes/3800004E - Sandstone.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.39000000 - Paragon Weapons.txt", include_str!("scripts/Recipes/39000000 - Paragon Weapons.txt")),
    ("ACE.Server.Entity.Mutations.Recipes.39000001 - Lucky White Rabbit's Foot.txt", include_str!("scripts/Recipes/39000001 - Lucky White Rabbit's Foot.txt")),
];
