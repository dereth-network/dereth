// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Adapter/Lifestoned/LifestonedConverter.cs
//! `LifestonedConverter`'s export direction (ACE weenie to Lifestoned JSON) and
//! `LifestonedLoader.AppendMetadata` (Source/ACE.Adapter/Lifestoned/LifestonedLoader.cs).

use empyrean_common::dotnet::{CsCast, DotNetDateTime};

use super::models::{
    ArmorValues, Attribute, AttributeSet, Body, BodyPart, BodyPartListing, Book, BoolStat,
    ChangelogEntry, CreateItem, DidStat, Emote, EmoteAction, EmoteCategoryListing, FloatStat,
    Frame, GeneratorTable, IidStat, Int64Stat, IntStat, LsdWeenie, Metadata, Page, Position,
    PositionListing, Quaternion, Skill, SkillListing, SpellCastingStats, SpellbookEntry,
    StringStat, Vital, Xyz, Zones,
};
use super::writer::{write_indented, JsonDateTime, SerializeError, ToJson, NEW_LINE};
use crate::import::json::{models as import_models, value};
use crate::models::world::{Weenie, WeeniePropertiesAttribute, WeeniePropertiesAttribute2nd};

/// `JsonSerializer.Serialize(value, LifestonedConverter.SerializerSettings)`.
///
/// ACE: LifestonedConverter.SerializerSettings (`WriteIndented`, `WhenWritingNull`,
/// `UnsafeRelaxedJsonEscaping`; see [`super::writer`]). Lines end with `Environment.NewLine`.
/// The static constructor is what sets those three options on `SerializerSettings`; here they are
/// fixed properties of [`write_indented`].
// ACE: LifestonedConverter.LifestonedConverter
pub fn serialize<T: ToJson>(value: &T) -> Result<String, SerializeError> {
    write_indented(&value.to_json(), NEW_LINE)
}

// PropertyAttribute / PropertyAttribute2nd values the converter looks up.
const STRENGTH: u16 = 1;
const ENDURANCE: u16 = 2;
const QUICKNESS: u16 = 3;
const COORDINATION: u16 = 4;
const FOCUS: u16 = 5;
const SELF: u16 = 6;
const MAX_HEALTH: u16 = 1;
const MAX_STAMINA: u16 = 3;
const MAX_MANA: u16 = 5;

/// `input.WeeniePropertiesAttribute.FirstOrDefault(a => a.Type == type)`, then a member of it:
/// `None` where ACE dereferences a null (`NullReferenceException`, caught by `TryConvert`).
fn attribute(input: &Weenie, r#type: u16) -> Option<Attribute> {
    let a: &WeeniePropertiesAttribute = input
        .weenie_properties_attribute
        .iter()
        .find(|a| a.r#type == r#type)?;
    Some(Attribute {
        ranks: Some(a.init_level),
        level_from_cp: a.level_from_cp,
        xp_spent: Some(a.cp_spent),
    })
}

fn vital(input: &Weenie, r#type: u16) -> Option<Vital> {
    let a: &WeeniePropertiesAttribute2nd = input
        .weenie_properties_attribute_2nd
        .iter()
        .find(|a| a.r#type == r#type)?;
    Some(Vital {
        ranks: Some(a.init_level),
        level_from_cp: Some(a.level_from_cp),
        xp_spent: Some(a.cp_spent),
        current: Some(a.current_level),
    })
}

fn frame(x: f32, y: f32, z: f32, w: f32, qx: f32, qy: f32, qz: f32) -> Frame {
    Frame {
        position: Some(Xyz { x, y, z }),
        rotations: Some(Quaternion {
            w,
            x: qx,
            y: qy,
            z: qz,
        }),
    }
}

/// Converts ACE weenie to LSD weenie. `now` stands in for `DateTime.UtcNow` (the changelog entry's
/// `Created`). `None` where ACE returns `false`: a class id of 0.
///
/// Not ACE's (a fix): a weenie with some but not all primary attributes
/// (or vitals) is exported with the ones it has (the missing ones left out, as the import expects);
/// ACE's lookup of a missing one threw, so such a weenie could not be exported.
// ACE: LifestonedConverter.TryConvert(Weenie, out LSDWeenie, bool)
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn try_convert(input: &Weenie, now: DotNetDateTime) -> Option<LsdWeenie> {
    if input.class_id == 0 {
        return None;
    }

    let mut result = LsdWeenie {
        weenie_id: input.class_id,
        ..LsdWeenie::default()
    };

    result.weenie_type_id = input.r#type;

    if let Some(book) = &input.weenie_properties_book {
        let mut b = Book {
            max_number_pages: book.max_num_pages,
            max_characters_per_page: book.max_num_chars_per_page,
            ..Book::default()
        };

        if !input.weenie_properties_book_page_data.is_empty() {
            let mut pages = Vec::new();
            let mut sorted: Vec<_> = input.weenie_properties_book_page_data.iter().collect();
            sorted.sort_by_key(|p| p.page_id); // OrderBy: stable
            for value in sorted {
                let mut page = Page {
                    author_id: Some(value.author_id),
                    author_name: Some(value.author_name.clone()),
                    author_account: Some(value.author_account.clone()),
                    ..Page::default()
                };
                page.set_ignore_author(Some(value.ignore_author));
                page.set_page_text(&value.page_text);
                pages.push(page);
            }
            b.pages = Some(pages);
        }
        result.book = Some(b);
    }

    // LandblockInstance

    // PointsOfInterest

    // WeeniePropertiesAnimPart

    if !input.weenie_properties_attribute.is_empty() {
        let attributes = result.attributes.get_or_insert_with(AttributeSet::default);
        attributes.strength = attribute(input, STRENGTH);
        attributes.endurance = attribute(input, ENDURANCE);
        attributes.quickness = attribute(input, QUICKNESS);
        attributes.coordination = attribute(input, COORDINATION);
        attributes.focus = attribute(input, FOCUS);
        attributes.self_ = attribute(input, SELF);
    }

    if !input.weenie_properties_attribute_2nd.is_empty() {
        let attributes = result.attributes.get_or_insert_with(AttributeSet::default);
        attributes.health = vital(input, MAX_HEALTH);
        attributes.stamina = vital(input, MAX_STAMINA);
        attributes.mana = vital(input, MAX_MANA);
    }

    if !input.weenie_properties_body_part.is_empty() {
        let mut body_parts = Vec::new();
        for value in &input.weenie_properties_body_part {
            body_parts.push(BodyPartListing {
                key: i32::from(value.key),
                body_part: Some(BodyPart {
                    d_type: value.d_type,
                    d_val: value.d_val,
                    d_var: value.d_var,
                    armor_values: Some(ArmorValues {
                        base_armor: value.base_armor,
                        armor_vs_slash: value.armor_vs_slash,
                        armor_vs_pierce: value.armor_vs_pierce,
                        armor_vs_bludgeon: value.armor_vs_bludgeon,
                        armor_vs_cold: value.armor_vs_cold,
                        armor_vs_fire: value.armor_vs_fire,
                        armor_vs_acid: value.armor_vs_acid,
                        armor_vs_electric: value.armor_vs_electric,
                        armor_vs_nether: value.armor_vs_nether,
                    }),
                    bh: value.bh,
                    sd: Some(Zones {
                        hlf: Some(f64::from(value.hlf)),
                        mlf: Some(f64::from(value.mlf)),
                        llf: Some(f64::from(value.llf)),

                        hrf: Some(f64::from(value.hrf)),
                        mrf: Some(f64::from(value.mrf)),
                        lrf: Some(f64::from(value.lrf)),

                        hlb: Some(f64::from(value.hlb)),
                        mlb: Some(f64::from(value.mlb)),
                        llb: Some(f64::from(value.llb)),

                        hrb: Some(f64::from(value.hrb)),
                        mrb: Some(f64::from(value.mrb)),
                        lrb: Some(f64::from(value.lrb)),
                    }),
                }),
            });
        }
        result.body = Some(Body {
            body_parts: Some(body_parts),
        });
    }

    if !input.weenie_properties_bool.is_empty() {
        let mut stats: Vec<BoolStat> = Vec::new();
        for value in &input.weenie_properties_bool {
            let key = i32::from(value.r#type);
            if !stats.iter().any(|x| x.key == key) {
                stats.push(BoolStat {
                    key,
                    value: i32::from(value.value),
                });
            }
        }
        result.bool_stats = Some(stats);
    }

    if !input.weenie_properties_create_list.is_empty() {
        let mut list = Vec::new();
        for value in &input.weenie_properties_create_list {
            list.push(CreateItem {
                weenie_class_id: Some(value.weenie_class_id),
                palette: Some(value.palette.cs_cast()),
                shade: Some(f64::from(value.shade)),
                destination: Some(value.destination_type.cs_cast()),
                stack_size: Some(value.stack_size),
                try_to_bond: Some(u8::from(value.try_to_bond)),
            });
        }
        result.create_list = Some(list);
    }

    if !input.weenie_properties_did.is_empty() {
        let mut stats = Vec::new();
        for value in &input.weenie_properties_did {
            stats.push(DidStat {
                key: i32::from(value.r#type),
                value: value.value,
            });
        }
        result.did_stats = Some(stats);
    }

    if !input.weenie_properties_emote.is_empty() {
        let mut table: Vec<EmoteCategoryListing> = Vec::new();

        // ACE numbers emotes and actions (`SortOrder`), a [JsonIgnore]d member.
        for emote in &input.weenie_properties_emote {
            let category_id: i32 = emote.category.cs_cast();
            let mut em = Emote {
                category: emote.category,
                class_id: emote.weenie_class_id,
                max_health: emote.max_health,
                min_health: emote.min_health,
                probability: Some(emote.probability),
                quest: emote.quest.clone(),
                style: emote.style,
                sub_style: emote.substyle,
                vendor_type: emote.vendor_type.map(CsCast::cs_cast),
                actions: Some(Vec::new()),
            };

            let mut actions: Vec<_> = emote.weenie_properties_emote_action.iter().collect();
            actions.sort_by_key(|e| e.order); // OrderBy: stable
            for action in actions {
                let mut ea = EmoteAction {
                    amount: action.amount.map(CsCast::cs_cast),
                    amount64: action.amount_64,
                    delay: Some(action.delay),
                    emote_action_type: action.r#type,
                    extent: Some(action.extent),
                    f_max: action.max_dbl.map(CsCast::cs_cast),
                    f_min: action.min_dbl.map(CsCast::cs_cast),
                    hero_xp64: action.hero_xp_64.map(CsCast::cs_cast),
                    max: action.max.map(CsCast::cs_cast),
                    min: action.min.map(CsCast::cs_cast),
                    maximum64: action.max_64,
                    minimum64: action.min_64,
                    message: action.message.clone(),
                    motion: action.motion,
                    percent: action.percent.map(CsCast::cs_cast),
                    p_script: action.p_script.map(CsCast::cs_cast),
                    sound: action.sound.map(CsCast::cs_cast),
                    spell_id: action.spell_id.map(CsCast::cs_cast),
                    stat: action.stat.map(CsCast::cs_cast),
                    test_string: action.test_string.clone(),
                    treasure_class: action.treasure_class.map(CsCast::cs_cast),
                    wealth_rating: action.wealth_rating.map(CsCast::cs_cast),
                    treasure_type: action.treasure_type,
                    ..EmoteAction::default()
                };
                ea.set_display(action.display);

                let f = frame(
                    action.origin_x.unwrap_or(0.0),
                    action.origin_y.unwrap_or(0.0),
                    action.origin_z.unwrap_or(0.0),
                    action.angles_w.unwrap_or(0.0),
                    action.angles_x.unwrap_or(0.0),
                    action.angles_y.unwrap_or(0.0),
                    action.angles_z.unwrap_or(0.0),
                );
                if action.obj_cell_id.is_some() {
                    ea.m_position = Some(Position {
                        land_cell_id: action.obj_cell_id.unwrap_or(0),
                        frame: Some(f),
                    });
                } else if action.origin_x.is_some()
                    || action.origin_y.is_some()
                    || action.origin_z.is_some()
                    || action.angles_w.is_some()
                    || action.angles_x.is_some()
                    || action.angles_y.is_some()
                    || action.angles_z.is_some()
                {
                    ea.frame = Some(f);
                }

                if action.destination_type.is_some() {
                    ea.item = Some(CreateItem {
                        destination: action.destination_type.map(CsCast::cs_cast),
                        palette: action.palette.map(CsCast::cs_cast),
                        shade: action.shade.map(f64::from),
                        stack_size: action.stack_size,
                        // Convert.ToByte(bool?): a null boxes to null, which converts to 0.
                        try_to_bond: Some(action.try_to_bond.map_or(0, u8::from)),
                        weenie_class_id: action.weenie_class_id,
                    });
                }

                if let Some(list) = em.actions.as_mut() {
                    list.push(ea);
                }
            }

            if let Some(existing) = table
                .iter_mut()
                .find(|e| e.emote_category_id == category_id)
            {
                if let Some(list) = existing.emotes.as_mut() {
                    list.push(em);
                }
            } else {
                table.push(EmoteCategoryListing {
                    emote_category_id: category_id,
                    emotes: Some(vec![em]),
                });
            }
        }
        result.emote_table = Some(table);
    }

    // WeeniePropertiesEventFilter

    if !input.weenie_properties_float.is_empty() {
        let stats = result.float_stats.get_or_insert_with(Vec::new);
        for value in &input.weenie_properties_float {
            let key = i32::from(value.r#type);
            if !stats.iter().any(|x: &FloatStat| x.key == key) {
                stats.push(FloatStat {
                    key,
                    value: value.value.cs_cast(),
                });
            }
        }
    }

    if !input.weenie_properties_generator.is_empty() {
        let mut table = Vec::new();
        let mut slot: u32 = 0;
        for value in &input.weenie_properties_generator {
            table.push(GeneratorTable {
                slot,
                probability: f64::from(value.probability),
                weenie_class_id: value.weenie_class_id,
                delay: value.delay.unwrap_or(0.0),

                init_create: value.init_create.cs_cast(),
                max_number: value.max_create.cs_cast(),

                when_create: value.when_create,
                where_create: value.where_create,

                stack_size: value.stack_size.unwrap_or(0),

                palette_id: value.palette_id.unwrap_or(0),
                shade: value.shade.unwrap_or(0.0),

                object_cell: value.obj_cell_id.unwrap_or(0),
                frame: Some(frame(
                    value.origin_x.unwrap_or(0.0),
                    value.origin_y.unwrap_or(0.0),
                    value.origin_z.unwrap_or(0.0),
                    value.angles_w.unwrap_or(0.0),
                    value.angles_x.unwrap_or(0.0),
                    value.angles_y.unwrap_or(0.0),
                    value.angles_z.unwrap_or(0.0),
                )),
            });
            slot = slot.wrapping_add(1);
        }
        result.generator_table = Some(table);
    }

    if !input.weenie_properties_iid.is_empty() {
        let mut stats: Vec<IidStat> = Vec::new();
        for value in &input.weenie_properties_iid {
            let key = i32::from(value.r#type);
            if !stats.iter().any(|x| x.key == key) {
                stats.push(IidStat {
                    key,
                    value: value.value.cs_cast(),
                });
            }
        }
        result.iid_stats = Some(stats);
    }

    if !input.weenie_properties_int.is_empty() {
        let mut stats: Vec<IntStat> = Vec::new();
        for value in &input.weenie_properties_int {
            let key = i32::from(value.r#type);
            if !stats.iter().any(|x| x.key == key) {
                stats.push(IntStat {
                    key,
                    value: value.value,
                });
            }
        }
        result.int_stats = Some(stats);
    }

    if !input.weenie_properties_int64.is_empty() {
        let mut stats: Vec<Int64Stat> = Vec::new();
        for value in &input.weenie_properties_int64 {
            let key = i32::from(value.r#type);
            if !stats.iter().any(|x| x.key == key) {
                stats.push(Int64Stat {
                    key,
                    value: value.value,
                });
            }
        }
        result.int64_stats = Some(stats);
    }

    // WeeniePropertiesPalette

    if !input.weenie_properties_position.is_empty() {
        let mut positions: Vec<PositionListing> = Vec::new();
        for value in &input.weenie_properties_position {
            let position_type = i32::from(value.position_type);
            if !positions.iter().any(|x| x.position_type == position_type) {
                positions.push(PositionListing {
                    position_type,
                    position: Some(Position {
                        land_cell_id: value.obj_cell_id,
                        frame: Some(frame(
                            value.origin_x,
                            value.origin_y,
                            value.origin_z,
                            value.angles_w,
                            value.angles_x,
                            value.angles_y,
                            value.angles_z,
                        )),
                    }),
                });
            }
        }
        result.positions = Some(positions);
    }

    if !input.weenie_properties_skill.is_empty() {
        let mut skills: Vec<SkillListing> = Vec::new();
        for value in &input.weenie_properties_skill {
            let skill_id = i32::from(value.r#type);
            if !skills.iter().any(|x| x.skill_id == Some(skill_id)) {
                skills.push(SkillListing {
                    skill_id: Some(skill_id),
                    skill: Some(Skill {
                        level_from_pp: Some(u32::from(value.level_from_pp)),
                        trained_level: Some(value.sac.cs_cast()),
                        xp_invested: Some(value.pp),
                        ranks: Some(value.init_level),
                        resistance_of_last_check: Some(value.resistance_at_last_check),
                        last_used: Some(value.last_used_time.cs_cast()),
                    }),
                });
            }
        }
        result.skills = Some(skills);
    }

    if !input.weenie_properties_spell_book.is_empty() {
        let mut spells: Vec<SpellbookEntry> = Vec::new();
        for value in &input.weenie_properties_spell_book {
            if !spells.iter().any(|x| x.spell_id == value.spell) {
                spells.push(SpellbookEntry {
                    spell_id: value.spell,
                    stats: Some(SpellCastingStats {
                        casting_chance: Some(f64::from(value.probability)),
                    }),
                });
            }
        }
        result.spells = Some(spells);
    }

    if !input.weenie_properties_string.is_empty() {
        let mut stats: Vec<StringStat> = Vec::new();
        for value in &input.weenie_properties_string {
            let key = i32::from(value.r#type);
            if !stats.iter().any(|x| x.key == key) {
                stats.push(StringStat {
                    key,
                    value: Some(value.value.clone()),
                });
            }
        }
        result.string_stats = Some(stats);
    }

    // WeeniePropertiesTextureMap

    // DIVERGE: the export names Empyrean where ACE's names ACE.Adapter and ACEmulator (brand).
    result.changelog = Some(vec![Some(ChangelogEntry {
        author: Some(empyrean_common::brand::EXPORT_AUTHOR.to_owned()),
        comment: Some(empyrean_common::brand::EXPORT_COMMENT.to_owned()),
        created: JsonDateTime::utc(now),
    })]);

    Some(result)
}

/// The converted weenie and its JSON text; `None` where ACE returns `false` ("try convert failed",
/// or "serialize failed" when a `float` is out of range). `now` stands in for `DateTime.UtcNow`.
// ACE: LifestonedConverter.TryConvertACEWeenieToLSDJSON
#[must_use]
pub fn try_convert_ace_weenie_to_lsd_json(
    weenie: &Weenie,
    now: DotNetDateTime,
) -> Option<(String, LsdWeenie)> {
    let lsd_weenie = try_convert(weenie, now)?;
    let result = serialize(&lsd_weenie).ok()?;
    Some((result, lsd_weenie))
}

/// Every weenie that converts, as Lifestoned JSON; `None` where ACE returns `false`: a weenie
/// that converts but does not serialize (a `float` out of range) throws out of the loop.
/// `now` stands in for `DateTime.UtcNow`.
// ACE: LifestonedConverter.TryConvertACEWeeniesToLSDJSON
#[must_use]
pub fn try_convert_ace_weenies_to_lsd_json(
    weenies: &[Weenie],
    now: DotNetDateTime,
) -> Option<Vec<String>> {
    let mut results = Vec::new();

    for weenie in weenies {
        if let Some(result) = try_convert(weenie, now) {
            results.push(serialize(&result).ok()?);
        }
    }

    Some(results)
}

/// Carry an existing file's metadata over to a freshly exported weenie. `existing_json` is the
/// file's contents (the caller reads it); `now` stands in for `DateTime.UtcNow`.
///
/// `Ok(false)` where ACE returns `false`: the file does not load as an `LSDWeenie`, or it holds no
/// metadata (`Metadata.HasInfo`). `Err` where ACE throws out of the command: a file whose document
/// is `null` (`TryLoadWeenie` succeeds with a null weenie, and `new Metadata(null)` throws
/// `NullReferenceException`).
// ACE: LifestonedLoader.AppendMetadata
pub fn append_metadata(
    existing_json: &str,
    weenie: &mut LsdWeenie,
    now: DotNetDateTime,
) -> Result<bool, String> {
    // read existing json weenie
    let json_weenie = match try_load_weenie_metadata(existing_json) {
        Err(_) => return Ok(false),
        Ok(None) => {
            return Err("NullReferenceException: the existing file's document is null".to_owned())
        }
        Ok(Some(w)) => w,
    };

    let mut metadata = Metadata::new(&json_weenie);

    if !metadata.has_info() {
        return Ok(false);
    }

    weenie.last_modified = Some(JsonDateTime::utc(now));
    // DIVERGE: the export names Empyrean where ACE's names ACE.Adapter and ACEmulator (brand).
    weenie.modified_by = Some(empyrean_common::brand::EXPORT_AUTHOR.to_owned());

    if let Some(changelog) = metadata.changelog.as_mut().filter(|c| !c.is_empty()) {
        changelog.extend(weenie.changelog.take().unwrap_or_default());
        weenie.changelog = metadata.changelog;
    }

    weenie.user_change_summary = Some(empyrean_common::brand::EXPORT_COMMENT.to_owned());
    weenie.is_done = metadata.is_done;

    Ok(true)
}

/// `LifestonedLoader.TryLoadWeenie` as far as `AppendMetadata` reads it: the whole document must
/// deserialize as an `LSDWeenie` (the `import-json` reader), and only the metadata members are
/// kept.
// ACE: LifestonedLoader.TryLoadWeenie
fn try_load_weenie_metadata(text: &str) -> Result<Option<LsdWeenie>, String> {
    // File.ReadAllText drops a byte order mark.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let doc = value::parse(text)?;
    if import_models::LsdWeenie::read(&doc)?.is_none() {
        return Ok(None);
    }
    let value::Json::Obj(members) = &doc else {
        return Ok(None);
    };
    let mut w = LsdWeenie::default();
    for (k, v) in members {
        match k.as_str() {
            "lastModified" => w.last_modified = opt_datetime(v)?,
            "modifiedBy" => w.modified_by = value::string(v, k)?,
            "userChangeSummary" => w.user_change_summary = value::string(v, k)?,
            "isDone" => w.is_done = value::bool_(v, k)?,
            "changelog" => {
                w.changelog = value::list(v, k, |e, what| {
                    let Some(members) = value::object(e, what)? else {
                        return Ok(None);
                    };
                    let mut entry = ChangelogEntry::default();
                    for (k, v) in members {
                        match k.as_str() {
                            "created" => entry.created = datetime(v)?,
                            "author" => entry.author = value::string(v, k)?,
                            "comment" => entry.comment = value::string(v, k)?,
                            _ => {}
                        }
                    }
                    Ok(Some(entry))
                })?;
            }
            _ => {}
        }
    }
    Ok(Some(w))
}

fn opt_datetime(j: &value::Json) -> Result<Option<JsonDateTime>, String> {
    match j {
        value::Json::Null => Ok(None),
        _ => datetime(j).map(Some),
    }
}

/// A `DateTime` as `Utf8JsonReader.GetDateTime` reads it: `yyyy-MM-dd`, optionally
/// `THH:mm[:ss[.fraction]]` (fraction digits past the seventh are dropped), then nothing
/// (`Unspecified`), `Z` (`Utc`) or `±HH[:mm]` (converted to local time, `Local`).
fn datetime(j: &value::Json) -> Result<JsonDateTime, String> {
    use super::writer::DateTimeKind;
    use empyrean_common::dotnet::datetime::{TICKS_PER_HOUR, TICKS_PER_MINUTE};

    let value::Json::Str(s) = j else {
        return Err("expected a date string".to_owned());
    };
    let bad = || format!("{s:?} is not an ISO 8601 date");
    value::datetime(j, "DateTime")?;
    let b = s.as_bytes();
    let num = |r: core::ops::Range<usize>| -> Result<i32, String> {
        let t = s.get(r).ok_or_else(bad)?;
        t.parse::<i32>().map_err(|_| bad())
    };
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (mut h, mut mi, mut sec, mut ticks) = (0, 0, 0, 0i64);
    let mut i = 10;
    if b.get(10) == Some(&b'T') {
        h = num(11..13)?;
        mi = num(14..16)?;
        i = 16;
        if b.get(i) == Some(&b':') {
            sec = num(i + 1..i + 3)?;
            i += 3;
            if b.get(i) == Some(&b'.') {
                i += 1;
                let mut digits = 0;
                while i < b.len() && b[i].is_ascii_digit() {
                    if digits < 7 {
                        ticks = ticks * 10 + i64::from(b[i] - b'0');
                    }
                    digits += 1;
                    i += 1;
                }
                for _ in digits..7 {
                    ticks *= 10;
                }
            }
        }
    }
    let base = DotNetDateTime::new_hms(y, mo, d, h, mi, sec).add_ticks(ticks);
    match b.get(i) {
        None => Ok(JsonDateTime {
            value: base,
            kind: DateTimeKind::Unspecified,
        }),
        Some(b'Z') => Ok(JsonDateTime {
            value: base,
            kind: DateTimeKind::Utc,
        }),
        Some(sign) => {
            let oh = i64::from(num(i + 1..i + 3)?);
            let om = if b.len() - i == 6 {
                i64::from(num(i + 4..i + 6)?)
            } else {
                0
            };
            let offset =
                (oh * TICKS_PER_HOUR + om * TICKS_PER_MINUTE) * if *sign == b'-' { -1 } else { 1 };
            // DIVERGE: `DateTimeOffset.LocalDateTime` converts to the host's zone; the host is taken
            // to be UTC (see `DateTimeKind::Local`).
            let utc = base.ticks() - offset;
            if !(0..=DotNetDateTime::MAX_VALUE.ticks()).contains(&utc) {
                return Err(bad());
            }
            Ok(JsonDateTime {
                value: DotNetDateTime::from_ticks(utc),
                kind: DateTimeKind::Local,
            })
        }
    }
}
