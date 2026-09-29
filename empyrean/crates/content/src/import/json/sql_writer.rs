// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/SQLWriter.cs, Source/ACE.Database/SQLFormatters/World/WeenieSQLWriter.cs, Source/ACE.Database/SQLFormatters/World/RecipeSQLWriter.cs, Source/ACE.Database/SQLFormatters/World/CookBookSQLWriter.cs, Source/ACE.Database/SQLFormatters/World/LandblockInstanceWriter.cs, Source/ACE.Database/SQLFormatters/World/QuestSQLWriter.cs
//! ACE's World SQL writers, as `json2sql_*` drives them: the `DELETE` and `INSERT` text that
//! `import-json` then executes.
//!
//! Every value is printed exactly as the C# prints it (column lists, `OrderBy` sorts, padding,
//! `0.###`/`0.######` custom float formats in `en-US` or the invariant culture, `X8` hex,
//! `TrimNegativeZero`, `True`/`False`, `''` quoting, `FixNullFields`). The `/* ... */` labels
//! the writers append (enum, weenie and spell names, `@teleloc` lines) are **not** written:
//! MySQL ignores comments, so they never reach the database, and without ACE's name
//! dictionaries they would differ anyway. Lines end with `\n` (`ImportSQL` converts `\r\n`).

use empyrean_common::dotnet::{format as dn, DotNetDateTime, Num};
use empyrean_entity::enums::PropertyDataId;

use super::world::*;

/// A `StreamWriter` over a string.
#[derive(Debug, Default)]
pub struct Out(pub String);

impl Out {
    fn line(&mut self, s: &str) {
        self.0.push_str(s);
        self.0.push('\n');
    }

    fn blank(&mut self) {
        self.0.push('\n');
    }
}

// ---- SQLWriter ------------------------------------------------------------------------------

// ACE: SQLWriter.ValuesWriter
fn values_writer(count: usize, line: impl Fn(usize) -> String, w: &mut Out) {
    for i in 0..count {
        let mut output = if i == 0 {
            "VALUES (".to_owned()
        } else {
            "     , (".to_owned()
        };
        output += &line(i);
        if i == count - 1 {
            output += ";";
        }
        w.line(&fix_null_fields(&output));
    }
}

// ACE: SQLWriter.GetSQLString
/// `'` + text with `'` doubled + `'`, or `None` (printed as nothing) for null. Backslashes are
/// not escaped, so MySQL reads them as escapes.
fn sql_string(input: Option<&str>) -> String {
    input.map_or_else(String::new, |s| format!("'{}'", s.replace('\'', "''")))
}

// ACE: SQLWriter.FixNullFields
/// Not ACE's (a fix): the export writer's form, which leaves string
/// values and comments alone; ACE's replacements ran over the whole line, so text containing
/// ", ," or ", )" was stored with "NULL" spliced into it.
fn fix_null_fields(input: &str) -> String {
    crate::export::sql::sql_writer::SQLWriter::fix_null_fields(input)
}

// ACE: SQLWriter.TrimNegativeZero
/// Round-trips through `0.######` text so `-0` becomes `0`.
fn trim_negative_zero(input: Option<f32>) -> Option<f32> {
    let v = input?;
    let s = dn(v, "0.######");
    let s = if s == "-0.######" || s == "-0" {
        &s[1..]
    } else {
        s.as_str()
    };
    Some(float_parse(s))
}

/// `float.Parse(s)` in `en-US` for the text `ToString` produced.
fn float_parse(s: &str) -> f32 {
    match s {
        "∞" => f32::INFINITY,
        "-∞" => f32::NEG_INFINITY,
        "NaN" => f32::NAN,
        _ => s.parse().unwrap_or(0.0),
    }
}

/// `ToString(format, CultureInfo.InvariantCulture)`: as `en-US` but for the infinity symbols.
fn inv(v: impl Into<Num>, format: &str) -> String {
    let s = dn(v, format);
    match s.as_str() {
        "∞" => "Infinity".into(),
        "-∞" => "-Infinity".into(),
        _ => s,
    }
}

/// `{value:format}` of a nullable: nothing for null.
fn opt<T: Into<Num> + Copy>(v: Option<T>, format: &str) -> String {
    v.map_or_else(String::new, |v| dn(v, format))
}

/// `{value}` of a nullable integer or bool.
fn opt_s<T: ToString>(v: Option<T>) -> String {
    v.map_or_else(String::new, |v| v.to_string())
}

/// C# `bool.ToString()`.
fn b(v: bool) -> &'static str {
    if v {
        "True"
    } else {
        "False"
    }
}

fn opt_b(v: Option<bool>) -> &'static str {
    v.map_or("", b)
}

fn pad_left(s: &str, n: usize) -> String {
    format!("{s:>n$}")
}

fn pad_right(s: &str, n: usize) -> String {
    format!("{s:<n$}")
}

/// `{x:X8}` with the `0x` prefix the writers add when the value is present.
fn hex_opt(v: Option<u32>) -> String {
    v.map_or_else(String::new, |v| format!("0x{v:08X}"))
}

fn date(d: DotNetDateTime) -> String {
    d.format("yyyy-MM-dd HH:mm:ss")
}

/// `{TrimNegativeZero(x):0.######}`.
fn tnz(v: f32) -> String {
    opt(trim_negative_zero(Some(v)), "0.######")
}

fn tnz_opt(v: Option<f32>) -> String {
    opt(trim_negative_zero(v), "0.######")
}

// ---- WeenieSQLWriter ------------------------------------------------------------------------

// ACE: WeenieSQLWriter.CreateSQLDELETEStatement
pub fn weenie_delete(input: &Weenie, w: &mut Out) {
    w.line(&format!(
        "DELETE FROM `weenie` WHERE `class_Id` = {};",
        input.class_id
    ));
}

/// A stable sort by key (`Enumerable.OrderBy`).
fn ordered<T: Clone, K: Ord>(v: &[T], key: impl Fn(&T) -> K) -> Vec<T> {
    let mut v = v.to_vec();
    v.sort_by_key(key);
    v
}

// ACE: WeenieSQLWriter.CreateSQLINSERTStatement
#[allow(clippy::too_many_lines)]
pub fn weenie_insert(input: &Weenie, w: &mut Out) {
    let id = input.class_id;
    w.line("INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)");
    // The class name is quoted without GetSQLString.
    let output = format!(
        "VALUES ({id}, '{}', {}, '{}');",
        input.class_name,
        input.r#type,
        date(input.last_modified)
    );
    w.line(&fix_null_fields(&output));

    if !input.int.is_empty() {
        w.blank();
        let v = ordered(&input.int, |r| r.r#type);
        w.line("INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)");
        values_writer(
            v.len(),
            |i| {
                format!(
                    "{id}, {}, {})",
                    pad_left(&v[i].r#type.to_string(), 3),
                    pad_left(&v[i].value.to_string(), 10)
                )
            },
            w,
        );
    }
    if !input.int64.is_empty() {
        w.blank();
        let v = ordered(&input.int64, |r| r.r#type);
        w.line("INSERT INTO `weenie_properties_int64` (`object_Id`, `type`, `value`)");
        values_writer(
            v.len(),
            |i| {
                format!(
                    "{id}, {}, {})",
                    pad_left(&v[i].r#type.to_string(), 3),
                    pad_left(&v[i].value.to_string(), 10)
                )
            },
            w,
        );
    }
    if !input.bool_.is_empty() {
        w.blank();
        let v = ordered(&input.bool_, |r| r.r#type);
        w.line("INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)");
        values_writer(
            v.len(),
            |i| {
                format!(
                    "{id}, {}, {})",
                    pad_left(&v[i].r#type.to_string(), 3),
                    pad_right(b(v[i].value), 5)
                )
            },
            w,
        );
    }
    if !input.float.is_empty() {
        w.blank();
        let v = ordered(&input.float, |r| r.r#type);
        w.line("INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)");
        values_writer(
            v.len(),
            |i| {
                format!(
                    "{id}, {}, {})",
                    pad_left(&v[i].r#type.to_string(), 3),
                    pad_left(&inv(v[i].value, "0.###"), 7)
                )
            },
            w,
        );
    }
    if !input.string.is_empty() {
        w.blank();
        let v = ordered(&input.string, |r| r.r#type);
        w.line("INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)");
        values_writer(
            v.len(),
            |i| {
                format!(
                    "{id}, {}, {})",
                    pad_left(&v[i].r#type.to_string(), 3),
                    sql_string(v[i].value.as_deref())
                )
            },
            w,
        );
    }
    if !input.did.is_empty() {
        w.blank();
        let v = ordered(&input.did, |r| r.r#type);
        w.line("INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)");
        values_writer(
            v.len(),
            |i| {
                let value = if PropertyDataId(v[i].r#type).is_hex_data() {
                    pad_left(&format!("0x{:08X}", v[i].value), 10)
                } else {
                    pad_left(&v[i].value.to_string(), 10)
                };
                format!("{id}, {}, {value})", pad_left(&v[i].r#type.to_string(), 3))
            },
            w,
        );
    }
    if !input.position.is_empty() {
        w.blank();
        let v = ordered(&input.position, |r| r.position_type);
        w.line("INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)");
        values_writer(
            v.len(),
            |i| {
                let p = &v[i];
                format!(
                    // The line ends where ACE starts its `@teleloc` comment line, so the `;` falls on the next line.
                    "{id}, {}, 0x{:08X}, {}, {}, {}, {}, {}, {}, {})
",
                    p.position_type,
                    p.obj_cell_id,
                    tnz(p.origin_x),
                    tnz(p.origin_y),
                    tnz(p.origin_z),
                    tnz(p.angles_w),
                    tnz(p.angles_x),
                    tnz(p.angles_y),
                    tnz(p.angles_z)
                )
            },
            w,
        );
    }
    if !input.iid.is_empty() {
        w.blank();
        let v = ordered(&input.iid, |r| r.r#type);
        w.line("INSERT INTO `weenie_properties_i_i_d` (`object_Id`, `type`, `value`)");
        values_writer(
            v.len(),
            |i| {
                format!(
                    "{id}, {}, {})",
                    pad_left(&v[i].r#type.to_string(), 3),
                    pad_left(&format!("0x{:08X}", v[i].value), 10)
                )
            },
            w,
        );
    }
    if !input.attribute.is_empty() {
        w.blank();
        let v = ordered(&input.attribute, |r| r.r#type);
        w.line("INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)");
        values_writer(
            v.len(),
            |i| {
                let a = &v[i];
                format!(
                    "{id}, {}, {}, {}, {})",
                    pad_left(&a.r#type.to_string(), 3),
                    pad_left(&a.init_level.to_string(), 3),
                    a.level_from_cp,
                    a.cp_spent
                )
            },
            w,
        );
    }
    if !input.attribute_2nd.is_empty() {
        w.blank();
        let v = ordered(&input.attribute_2nd, |r| r.r#type);
        w.line("INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)");
        values_writer(
            v.len(),
            |i| {
                let a = &v[i];
                format!(
                    "{id}, {}, {}, {}, {}, {})",
                    pad_left(&a.r#type.to_string(), 3),
                    pad_left(&a.init_level.to_string(), 5),
                    a.level_from_cp,
                    a.cp_spent,
                    a.current_level
                )
            },
            w,
        );
    }
    if !input.skill.is_empty() {
        w.blank();
        let v = ordered(&input.skill, |r| r.r#type);
        w.line("INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)");
        values_writer(
            v.len(),
            |i| {
                let s = &v[i];
                format!(
                    "{id}, {}, {}, {}, {}, {}, {}, {})",
                    pad_left(&s.r#type.to_string(), 2),
                    s.level_from_pp,
                    s.sac,
                    s.pp,
                    pad_left(&s.init_level.to_string(), 3),
                    s.resistance_at_last_check,
                    dn(s.last_used_time, "")
                )
            },
            w,
        );
    }
    if !input.body_part.is_empty() {
        w.blank();
        let v = ordered(&input.body_part, |r| r.key);
        w.line(
            "INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, \
             `d_Type`, `d_Val`, `d_Var`, \
             `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, \
             `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)",
        );
        values_writer(
            v.len(),
            |i| {
                let p = &v[i];
                let armor = [
                    p.base_armor,
                    p.armor_vs_slash,
                    p.armor_vs_pierce,
                    p.armor_vs_bludgeon,
                    p.armor_vs_cold,
                    p.armor_vs_fire,
                    p.armor_vs_acid,
                    p.armor_vs_electric,
                    p.armor_vs_nether,
                ];
                let mut s = format!(
                    "{id}, {}, {}, {}, {}, ",
                    pad_left(&p.key.to_string(), 2),
                    pad_left(&p.d_type.to_string(), 2),
                    pad_left(&p.d_val.to_string(), 2),
                    pad_left(&inv(p.d_var, ""), 4)
                );
                for a in armor {
                    s += &pad_left(&a.to_string(), 4);
                    s += ", ";
                }
                s += &format!("{}, ", p.bh);
                let zones: Vec<String> =
                    p.zones.iter().map(|z| pad_left(&inv(*z, ""), 4)).collect();
                s += &zones.join(", ");
                s += ")";
                s
            },
            w,
        );
    }
    if !input.spell_book.is_empty() {
        w.blank();
        let v = &input.spell_book;
        w.line("INSERT INTO `weenie_properties_spell_book` (`object_Id`, `spell`, `probability`)");
        values_writer(
            v.len(),
            |i| {
                format!(
                    "{id}, {}, {})",
                    pad_left(&v[i].spell.to_string(), 5),
                    pad_left(&inv(v[i].probability, "0.######"), 6)
                )
            },
            w,
        );
    }
    if !input.emote.is_empty() {
        emote_insert(id, &ordered(&input.emote, |r| r.category), w);
    }
    if !input.create_list.is_empty() {
        w.blank();
        let v = ordered(&input.create_list, |r| r.destination_type);
        w.line("INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)");
        values_writer(
            v.len(),
            |i| {
                let c = &v[i];
                format!(
                    "{id}, {}, {}, {}, {}, {}, {})",
                    c.destination_type,
                    pad_left(&c.weenie_class_id.to_string(), 5),
                    pad_left(&c.stack_size.to_string(), 2),
                    c.palette,
                    dn(c.shade, "0.######"),
                    b(c.try_to_bond)
                )
            },
            w,
        );
    }
    if let Some(book) = &input.book {
        w.blank();
        w.line("INSERT INTO `weenie_properties_book` (`object_Id`, `max_Num_Pages`, `max_Num_Chars_Per_Page`)");
        w.line(&format!(
            "VALUES ({id}, {}, {});",
            book.max_num_pages, book.max_num_chars_per_page
        ));
    }
    if !input.book_page_data.is_empty() {
        w.blank();
        let v = ordered(&input.book_page_data, |r| r.page_id);
        w.line("INSERT INTO `weenie_properties_book_page_data` (`object_Id`, `page_Id`, `author_Id`, `author_Name`, `author_Account`, `ignore_Author`, `page_Text`)");
        values_writer(
            v.len(),
            |i| {
                let p = &v[i];
                format!(
                    "{id}, {}, 0x{:08X}, {}, {}, {}, {})",
                    p.page_id,
                    p.author_id,
                    sql_string(p.author_name.as_deref()),
                    sql_string(p.author_account.as_deref()),
                    b(p.ignore_author),
                    sql_string(p.page_text.as_deref())
                )
            },
            w,
        );
    }
    if !input.generator.is_empty() {
        w.blank();
        let v = &input.generator;
        w.line(
            "INSERT INTO `weenie_properties_generator` (`object_Id`, `probability`, `weenie_Class_Id`, \
             `delay`, `init_Create`, `max_Create`, `when_Create`, `where_Create`, `stack_Size`, `palette_Id`, `shade`, \
             `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)",
        );
        values_writer(
            v.len(),
            |i| {
                let g = &v[i];
                let cell = match g.obj_cell_id {
                    Some(c) if c > 0 => format!("0x{c:08X}"),
                    other => opt_s(other),
                };
                format!(
                    "{id}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {cell}, {}, {}, {}, {}, {}, {}, {})",
                    dn(g.probability, "0.######"),
                    g.weenie_class_id,
                    opt(g.delay, "0.######"),
                    g.init_create,
                    g.max_create,
                    g.when_create,
                    g.where_create,
                    opt_s(g.stack_size),
                    opt_s(g.palette_id),
                    opt(g.shade, "0.######"),
                    tnz_opt(g.origin_x),
                    tnz_opt(g.origin_y),
                    tnz_opt(g.origin_z),
                    tnz_opt(g.angles_w),
                    tnz_opt(g.angles_x),
                    tnz_opt(g.angles_y),
                    tnz_opt(g.angles_z)
                )
            },
            w,
        );
    }
}

fn emote_insert(id: u32, input: &[Emote], w: &mut Out) {
    for value in input {
        w.blank();
        w.line("INSERT INTO `weenie_properties_emote` (`object_Id`, `category`, `probability`, `weenie_Class_Id`, `style`, `substyle`, `quest`, `vendor_Type`, `min_Health`, `max_Health`)");
        let output = format!(
            "VALUES ({id}, {}, {}, {}, {}, {}, {}, {}, {}, {});",
            pad_left(&value.category.to_string(), 2),
            pad_left(&inv(value.probability, "0.######"), 6),
            opt_s(value.weenie_class_id),
            hex_opt(value.style),
            hex_opt(value.substyle),
            sql_string(value.quest.as_deref()),
            opt_s(value.vendor_type),
            opt(value.min_health, "0.######"),
            opt(value.max_health, "0.######")
        );
        w.line(&fix_null_fields(&output));
        if !value.actions.is_empty() {
            w.blank();
            w.line("SET @parent_id = LAST_INSERT_ID();");
            w.blank();
            emote_action_insert(&ordered(&value.actions, |r| r.order), w);
        }
    }
}

fn emote_action_insert(input: &[EmoteAction], w: &mut Out) {
    w.line(
        "INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `delay`, `extent`, `motion`, `message`, `test_String`, `min`, `max`, `min_64`, `max_64`, `min_Dbl`, `max_Dbl`, \
         `stat`, `display`, `amount`, `amount_64`, `hero_X_P_64`, `percent`, `spell_Id`, `wealth_Rating`, `treasure_Class`, `treasure_Type`, `p_Script`, `sound`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`, \
         `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)",
    );
    values_writer(
        input.len(),
        |i| {
            let a = &input[i];
            let fields = [
                "@parent_id".to_owned(),
                pad_left(&a.order.to_string(), 2),
                pad_left(&a.r#type.to_string(), 3),
                dn(a.delay, "0.######"),
                dn(a.extent, "0.######"),
                hex_opt(a.motion),
                sql_string(a.message.as_deref()),
                sql_string(a.test_string.as_deref()),
                opt_s(a.min),
                opt_s(a.max),
                opt_s(a.min_64),
                opt_s(a.max_64),
                opt(a.min_dbl, ""),
                opt(a.max_dbl, ""),
                opt_s(a.stat),
                opt_b(a.display).to_owned(),
                opt_s(a.amount),
                opt_s(a.amount_64),
                opt_s(a.hero_xp_64),
                opt(a.percent, ""),
                opt_s(a.spell_id),
                opt_s(a.wealth_rating),
                opt_s(a.treasure_class),
                opt_s(a.treasure_type),
                opt_s(a.p_script),
                opt_s(a.sound),
                opt_s(a.destination_type),
                opt_s(a.weenie_class_id),
                opt_s(a.stack_size),
                opt_s(a.palette),
                opt(a.shade, "0.######"),
                opt_b(a.try_to_bond).to_owned(),
                hex_opt(a.obj_cell_id),
                tnz_opt(a.origin_x),
                tnz_opt(a.origin_y),
                tnz_opt(a.origin_z),
                tnz_opt(a.angles_w),
                tnz_opt(a.angles_x),
                tnz_opt(a.angles_y),
                tnz_opt(a.angles_z),
            ];
            fields.join(", ") + ")"
        },
        w,
    );
}

// ---- RecipeSQLWriter / CookBookSQLWriter ------------------------------------------------------

// ACE: RecipeSQLWriter.CreateSQLDELETEStatement
pub fn recipe_delete(input: &Recipe, w: &mut Out) {
    w.line(&format!("DELETE FROM `recipe` WHERE `id` = {};", input.id));
}

// ACE: RecipeSQLWriter.CreateSQLINSERTStatement
pub fn recipe_insert(input: &Recipe, w: &mut Out) {
    w.line(
        "INSERT INTO `recipe` (`id`, `unknown_1`, `skill`, `difficulty`, `salvage_Type`, `success_W_C_I_D`, `success_Amount`, `success_Message`, `fail_W_C_I_D`, `fail_Amount`, `fail_Message`, \
         `success_Destroy_Source_Chance`, `success_Destroy_Source_Amount`, `success_Destroy_Source_Message`, `success_Destroy_Target_Chance`, `success_Destroy_Target_Amount`, `success_Destroy_Target_Message`, \
         `fail_Destroy_Source_Chance`, `fail_Destroy_Source_Amount`, `fail_Destroy_Source_Message`, `fail_Destroy_Target_Chance`, `fail_Destroy_Target_Amount`, `fail_Destroy_Target_Message`, \
         `data_Id`, `last_Modified`)",
    );
    let r = input;
    let fields = [
        r.id.to_string(),
        r.unknown_1.to_string(),
        r.skill.to_string(),
        r.difficulty.to_string(),
        r.salvage_type.to_string(),
        r.success_wcid.to_string(),
        r.success_amount.to_string(),
        sql_string(r.success_message.as_deref()),
        r.fail_wcid.to_string(),
        r.fail_amount.to_string(),
        sql_string(r.fail_message.as_deref()),
        dn(r.success_destroy_source_chance, ""),
        r.success_destroy_source_amount.to_string(),
        sql_string(r.success_destroy_source_message.as_deref()),
        dn(r.success_destroy_target_chance, ""),
        r.success_destroy_target_amount.to_string(),
        sql_string(r.success_destroy_target_message.as_deref()),
        dn(r.fail_destroy_source_chance, ""),
        r.fail_destroy_source_amount.to_string(),
        sql_string(r.fail_destroy_source_message.as_deref()),
        dn(r.fail_destroy_target_chance, ""),
        r.fail_destroy_target_amount.to_string(),
        sql_string(r.fail_destroy_target_message.as_deref()),
        r.data_id.to_string(),
        format!("'{}'", date(r.last_modified)),
    ];
    w.line(&fix_null_fields(&format!(
        "VALUES ({});",
        fields.join(", ")
    )));

    let id = r.id;
    requirement_rows(
        id,
        "recipe_requirements_int",
        &r.requirements_int,
        |v| v.to_string(),
        w,
    );
    requirement_rows(
        id,
        "recipe_requirements_d_i_d",
        &r.requirements_did,
        |v| v.to_string(),
        w,
    );
    requirement_rows(
        id,
        "recipe_requirements_i_i_d",
        &r.requirements_iid,
        |v| v.to_string(),
        w,
    );
    requirement_rows(
        id,
        "recipe_requirements_float",
        &r.requirements_float,
        |v| dn(*v, ""),
        w,
    );
    requirement_rows(
        id,
        "recipe_requirements_string",
        &r.requirements_string,
        |v| sql_string(v.as_deref()),
        w,
    );
    requirement_rows(
        id,
        "recipe_requirements_bool",
        &r.requirements_bool,
        |v| b(*v).to_owned(),
        w,
    );

    for value in &r.recipe_mod {
        w.blank();
        w.line("INSERT INTO `recipe_mod` (`recipe_Id`, `executes_On_Success`, `health`, `stamina`, `mana`, `unknown_7`, `data_Id`, `unknown_9`, `instance_Id`)");
        let data_id = if value.data_id > 0 {
            format!("0x{:08X}", value.data_id)
        } else {
            value.data_id.to_string()
        };
        let output = format!(
            "VALUES ({id}, {}, {}, {}, {}, {}, {data_id}, {}, {});",
            b(value.executes_on_success),
            value.health,
            value.stamina,
            value.mana,
            b(value.unknown_7),
            value.unknown_9,
            value.instance_id
        );
        w.line(&fix_null_fields(&output));
        let any = !value.mods_int.is_empty()
            || !value.mods_did.is_empty()
            || !value.mods_iid.is_empty()
            || !value.mods_float.is_empty()
            || !value.mods_string.is_empty()
            || !value.mods_bool.is_empty();
        if any {
            w.blank();
            w.line("SET @parent_id = LAST_INSERT_ID();");
        }
        mod_rows("recipe_mods_int", &value.mods_int, |v| v.to_string(), w);
        mod_rows("recipe_mods_d_i_d", &value.mods_did, |v| v.to_string(), w);
        mod_rows("recipe_mods_i_i_d", &value.mods_iid, |v| v.to_string(), w);
        mod_rows("recipe_mods_float", &value.mods_float, |v| dn(*v, ""), w);
        mod_rows(
            "recipe_mods_string",
            &value.mods_string,
            |v| sql_string(v.as_deref()),
            w,
        );
        mod_rows(
            "recipe_mods_bool",
            &value.mods_bool,
            |v| b(*v).to_owned(),
            w,
        );
    }
}

fn requirement_rows<V>(
    recipe_id: u32,
    table: &str,
    rows: &[RecipeRow<V>],
    value: impl Fn(&V) -> String,
    w: &mut Out,
) {
    if rows.is_empty() {
        return;
    }
    w.blank();
    w.line(&format!(
        "INSERT INTO `{table}` (`recipe_Id`, `index`, `stat`, `value`, `enum`, `message`)"
    ));
    values_writer(
        rows.len(),
        |i| {
            let r = &rows[i];
            format!(
                "{recipe_id}, {}, {}, {}, {}, {})",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                value(&r.value),
                r.r#enum,
                sql_string(r.message.as_deref())
            )
        },
        w,
    );
}

fn mod_rows<V>(table: &str, rows: &[RecipeRow<V>], value: impl Fn(&V) -> String, w: &mut Out) {
    if rows.is_empty() {
        return;
    }
    w.blank();
    w.line(&format!(
        "INSERT INTO `{table}` (`recipe_Mod_Id`, `index`, `stat`, `value`, `enum`, `source`)"
    ));
    values_writer(
        rows.len(),
        |i| {
            let r = &rows[i];
            format!(
                "@parent_id, {}, {}, {}, {}, {})",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                value(&r.value),
                r.r#enum,
                r.source
            )
        },
        w,
    );
}

// ACE: CookBookSQLWriter.CreateSQLDELETEStatement
/// `input[0]` throws on an empty list.
pub fn cook_book_delete(input: &[CookBook], w: &mut Out) -> Result<(), String> {
    let first = input.first().ok_or("no cook books")?;
    w.line(&format!(
        "DELETE FROM `cook_book` WHERE `recipe_Id` = {};",
        first.recipe_id
    ));
    Ok(())
}

// ACE: CookBookSQLWriter.CreateSQLINSERTStatement
pub fn cook_book_insert(input: &[CookBook], w: &mut Out) {
    w.line("INSERT INTO `cook_book` (`recipe_Id`, `source_W_C_I_D`, `target_W_C_I_D`, `last_Modified`)");
    values_writer(
        input.len(),
        |i| {
            let c = &input[i];
            format!(
                "{}, {}, {}, '{}')",
                c.recipe_id,
                c.source_wcid,
                pad_left(&c.target_wcid.to_string(), 5),
                date(c.last_modified)
            )
        },
        w,
    );
}

// ---- LandblockInstanceWriter ----------------------------------------------------------------

// ACE: LandblockInstanceWriter.CreateSQLDELETEStatement
/// `input[0]` throws on an empty list.
pub fn landblock_delete(input: &[LandblockInstance], w: &mut Out) -> Result<(), String> {
    let first = input.first().ok_or("no landblock instances")?;
    w.line(&format!(
        "DELETE FROM `landblock_instance` WHERE `landblock` = 0x{:04X};",
        first.obj_cell_id >> 16
    ));
    Ok(())
}

// ACE: LandblockInstanceWriter.CreateSQLINSERTStatement
/// `input.ToDictionary(i => i.Guid, ...)` throws on a repeated guid.
pub fn landblock_insert(
    input: &[LandblockInstance],
    links: &[LandblockInstanceLink],
    w: &mut Out,
) -> Result<(), String> {
    for (i, a) in input.iter().enumerate() {
        if input[..i].iter().any(|b| b.guid == a.guid) {
            return Err(format!("guid {:#X} appears twice", a.guid));
        }
    }
    let input = ordered(input, |r| r.guid);
    for (n, value) in input.iter().enumerate() {
        if n != 0 {
            w.blank();
        }
        w.line("INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)");
        let output = format!(
            "VALUES (0x{:08X}, {}, 0x{:08X}, {}, {}, {}, {}, {}, {}, {}, {}, '{}');",
            value.guid,
            pad_left(&value.weenie_class_id.to_string(), 5),
            value.obj_cell_id,
            tnz(value.origin_x),
            tnz(value.origin_y),
            tnz(value.origin_z),
            tnz(value.angles_w),
            tnz(value.angles_x),
            tnz(value.angles_y),
            tnz(value.angles_z),
            pad_left(b(value.is_link_child), 5),
            date(value.last_modified)
        );
        w.line(&fix_null_fields(&output));
        if !value.links.is_empty() {
            w.blank();
            let mut own: Vec<&LandblockInstanceLink> =
                value.links.iter().map(|&i| &links[i]).collect();
            own.sort_by_key(|l| l.child_guid);
            w.line("INSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)");
            values_writer(
                own.len(),
                |i| {
                    format!(
                        "0x{:08X}, 0x{:08X}, '{}')",
                        own[i].parent_guid,
                        own[i].child_guid,
                        date(own[i].last_modified)
                    )
                },
                w,
            );
        }
    }
    Ok(())
}

// ---- QuestSQLWriter -------------------------------------------------------------------------

// ACE: QuestSQLWriter.CreateSQLDELETEStatement
pub fn quest_delete(input: &Quest, w: &mut Out) {
    w.line(&format!(
        "DELETE FROM `quest` WHERE `name` = {};",
        sql_string(input.name.as_deref())
    ));
}

// ACE: QuestSQLWriter.CreateSQLINSERTStatement
pub fn quest_insert(input: &Quest, w: &mut Out) {
    w.line("INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`, `last_Modified`)");
    let output = format!(
        "VALUES ({}, {}, {}, {}, '{}');",
        sql_string(input.name.as_deref()),
        input.min_delta,
        input.max_solves,
        sql_string(input.message.as_deref()),
        date(input.last_modified)
    );
    w.line(&fix_null_fields(&output));
}
