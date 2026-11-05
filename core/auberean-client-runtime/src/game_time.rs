//! The world's clock as the scripts reach it: the calendar's lengths, where the world is in it, the
//! date and time written as string references, and an admin's two commands. Asked with no clock (no
//! region yet), a number answers 0 and a request answers that it was not done.

use auberean_assets::region::{Region, SkyDesc};
use auberean_glue::text::{table_or_none, TABLE_ENUM_MAPPING};
use auberean_script::native::{Call, NativeFn, Status, Stop};
use auberean_script::native_object::{NativeObject, TextInfo, Variable, VariableValue};
use auberean_script::value::{tag, NativeId, NULL};
use auberean_world::calendar::{DateTime, GameClock};

use crate::cache::DataCache;
use crate::host::host;
use crate::repository::write_long;

/// The clock's native package.
const GAME_TIME: u32 = 0x2C;

/// The calendar's strings' table, by enumeration value.
const CALENDAR_TABLE: u32 = 0x4000_0002;

/// The clock a region runs by, when the world has just gone into it: its sky's calendar; failing
/// that the clock it had; failing that one with no calendar.
// D2-07492, D2-13059
#[must_use]
pub fn region_clock(cache: &DataCache, region: u32, had: Option<GameClock>) -> GameClock {
    if cache.is_empty() {
        return had.unwrap_or_default();
    }
    let db = cache.database();
    let record = db.data_id_by_enum(region, crate::world::REGION_MAPPING);
    let calendar = db
        .get::<Region>(record)
        .and_then(|r| r.skies.first().map(|s| s.record.0))
        .filter(|id| *id != 0)
        .and_then(|id| db.get::<SkyDesc>(id))
        .and_then(|sky| db.get::<auberean_assets::region::GameTime>(sky.game_time.0));
    match calendar {
        Some(c) => GameClock::new((*c.arc()).clone()),
        None => had.unwrap_or_default(),
    }
}

fn clock<'a>(c: &'a mut Call<'_>) -> Result<Option<&'a mut GameClock>, Stop> {
    Ok(host(c)?.game_clock.as_mut())
}

/// A word answered below the receiver: a null receiver crashes the call.
fn word(c: &mut Call<'_>, read: fn(&GameClock) -> u32) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let v = clock(c)?.map_or(0, |g| read(g));
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// Whether it is night now: the word the part of the day holds.
// D2-08598
fn is_night(c: &mut Call<'_>) -> Result<Status, Stop> {
    let v = clock(c)?.map_or(0, |g| g.night());
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// How far through the day, as a 64-bit float below the receiver.
// D2-08593
fn time_of_day(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let v = clock(c)?.map_or(0.0, |g| f64::from(g.time_of_day()));
    write_long(c, -3, v.to_bits())
}

// D2-08592
fn day_length(c: &mut Call<'_>) -> Result<Status, Stop> {
    word(c, |g| g.day_length_seconds().to_bits())
}

// D2-08591
fn day_in_year(c: &mut Call<'_>) -> Result<Status, Stop> {
    word(c, |g| g.day().cast_unsigned())
}

// D2-08595
fn year_length(c: &mut Call<'_>) -> Result<Status, Stop> {
    word(c, |g| g.days_per_year().cast_unsigned())
}

// D2-08594
fn year(c: &mut Call<'_>) -> Result<Status, Stop> {
    word(c, |g| g.year().cast_unsigned())
}

/// Put a date and a time in two string references: their strings in the calendar's table with the
/// year, its name, the day and the season, and the part of the day.
// D2-13062
fn write_strings(c: &mut Call<'_>, date: u32, time: u32, d: &DateTime) -> Result<(), Stop> {
    let table = table_or_none(c.host.data_id_by_enum(CALENDAR_TABLE, TABLE_ENUM_MAPPING));
    let token = |s: &str| auberean_assets::strings::token(s);
    let named = |id: u32| {
        VariableValue::Nested(Box::new(TextInfo {
            string_id: id,
            ..TextInfo::default()
        }))
    };
    let mut set =
        |r: u32, string: &str, vars: Vec<(&str, u16, VariableValue)>| -> Result<(), Stop> {
            let NativeObject::TextInfo(t) = c.vm.heap.native_mut(r)? else {
                return Err(Stop::Host("not a string reference".into()));
            };
            t.string_id = token(string);
            t.table = table;
            for (name, kind, value) in vars {
                t.set_variable(Variable {
                    id: token(name),
                    kind,
                    value,
                });
            }
            Ok(())
        };
    set(
        date,
        "ID_DateString",
        vec![
            ("YEAR", 2, VariableValue::Word(d.year.cast_unsigned())),
            ("YEARSPEC", 9, named(d.year_name)),
            ("DAY", 2, VariableValue::Word(d.day.cast_unsigned())),
            ("SEASON", 9, named(d.season_name)),
        ],
    )?;
    set(time, "ID_TimeString", vec![("TIME", 9, named(d.part_name))])
}

/// The present date and time into two string references (`fp[-2]`, `fp[-3]`); whether done, below.
// D2-08596
fn current_date_time(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (date, time) = (c.cx.at(-2)?, c.cx.at(-3)?);
    let d = clock(c)?.and_then(|g| g.current_date_time());
    if let Some(d) = &d {
        write_strings(c, date, time, d)?;
    }
    c.write(-4, u32::from(d.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// The date and time at a stamp (a 64-bit float at `fp[-5]`) into two string references (`fp[-2]`,
/// `fp[-3]`); whether done, below.
// D2-08597
fn date_time_at(c: &mut Call<'_>) -> Result<Status, Stop> {
    let (date, time) = (c.cx.at(-2)?, c.cx.at(-3)?);
    let stamp = f64::from_bits(c.long_at(-5)?);
    let d = clock(c)?.and_then(|g| g.date_time_at(stamp));
    if let Some(d) = &d {
        write_strings(c, date, time, d)?;
    }
    c.write(-6, u32::from(d.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// Move the time of day to the float at `fp[-2]`; whether done, below.
// D2-08590
fn alter_time_of_day(c: &mut Call<'_>) -> Result<Status, Stop> {
    let to = f32::from_bits(c.cx.at(-2)?);
    let h = host(c)?;
    let t = h.clock;
    let done = h.game_clock.as_mut().is_some_and(|g| {
        g.alter_time_of_day(to, t);
        true
    });
    c.write(-3, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// Run the world's time at the rate at `fp[-2]`; whether done, below.
// D2-08599
fn dilate(c: &mut Call<'_>) -> Result<Status, Stop> {
    let rate = f32::from_bits(c.cx.at(-2)?);
    let h = host(c)?;
    let t = h.clock;
    let done = h.game_clock.as_mut().is_some_and(|g| g.dilate(rate, t));
    c.write(-3, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// The clock's natives, by id.
pub(crate) fn natives() -> impl Iterator<Item = (NativeId, NativeFn)> {
    const NATIVES: &[(u32, NativeFn)] = &[
        (0x01, is_night),
        (0x02, time_of_day),
        (0x03, day_length),
        (0x04, day_in_year),
        (0x05, year_length),
        (0x06, year),
        (0x07, date_time_at),
        (0x08, current_date_time),
        (0x09, alter_time_of_day),
        (0x0A, dilate),
    ];
    NATIVES
        .iter()
        .map(|(n, f)| (NativeId::new(GAME_TIME, *n), *f))
}
