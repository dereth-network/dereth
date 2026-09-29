//! A small `System.DateTime` and `System.TimeSpan`: 100 ns ticks, the arithmetic ACE uses, and
//! `en-US` formatting. No external date crate is used.
//!
//! Source: dotnet/runtime `src/libraries/System.Private.CoreLib/src/System/DateTime.cs`,
//! `TimeSpan.cs`, `Globalization/DateTimeFormat.cs` and `Globalization/TimeSpanFormat.cs`.
//! Expected values in the tests come from the .NET 8.0.22 runtime
//! (`tests/fixtures/dotnet_oracle_misc.cs`).
//!
//! `DateTime.Kind` is not modelled: every `DotNetDateTime` in the server is UTC or a literal date
//! that ACE only compares and subtracts, which ignore the kind.

use std::fmt;
use std::ops::{Add, Sub};

/// Ticks (100 ns) per millisecond.
pub const TICKS_PER_MILLISECOND: i64 = 10_000;
/// Ticks per second.
pub const TICKS_PER_SECOND: i64 = TICKS_PER_MILLISECOND * 1000;
/// Ticks per minute.
pub const TICKS_PER_MINUTE: i64 = TICKS_PER_SECOND * 60;
/// Ticks per hour.
pub const TICKS_PER_HOUR: i64 = TICKS_PER_MINUTE * 60;
/// Ticks per day.
pub const TICKS_PER_DAY: i64 = TICKS_PER_HOUR * 24;

const DAYS_PER_YEAR: i64 = 365;
const DAYS_PER_4_YEARS: i64 = DAYS_PER_YEAR * 4 + 1;
const DAYS_PER_100_YEARS: i64 = DAYS_PER_4_YEARS * 25 - 1;
const DAYS_PER_400_YEARS: i64 = DAYS_PER_100_YEARS * 4 + 1;
const DAYS_TO_10000: i64 = DAYS_PER_400_YEARS * 25 - 366;
const MAX_TICKS: i64 = DAYS_TO_10000 * TICKS_PER_DAY - 1;
const DAYS_TO_1970: i64 =
    DAYS_PER_400_YEARS * 4 + DAYS_PER_100_YEARS * 3 + DAYS_PER_4_YEARS * 17 + DAYS_PER_YEAR;

const DAYS_TO_MONTH_365: [i64; 13] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365];
const DAYS_TO_MONTH_366: [i64; 13] = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335, 366];

/// `System.DateTime` (ticks since 0001-01-01 00:00:00).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DotNetDateTime {
    ticks: i64,
}

/// `System.TimeSpan` (signed ticks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TimeSpan {
    ticks: i64,
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn i32_of(v: i64) -> i32 {
    i32::try_from(v).expect("DateTime component fits in i32")
}

impl DotNetDateTime {
    /// `DateTime.MinValue`.
    pub const MIN_VALUE: Self = Self { ticks: 0 };
    /// `DateTime.MaxValue`.
    pub const MAX_VALUE: Self = Self { ticks: MAX_TICKS };
    /// `DateTime.UnixEpoch`.
    pub const UNIX_EPOCH: Self = Self {
        ticks: DAYS_TO_1970 * TICKS_PER_DAY,
    };

    /// `new DateTime(ticks)`.
    ///
    /// # Panics
    /// Outside `MinValue..=MaxValue` (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn from_ticks(ticks: i64) -> Self {
        assert!(
            (0..=MAX_TICKS).contains(&ticks),
            "ArgumentOutOfRangeException: ticks {ticks}"
        );
        Self { ticks }
    }

    /// `new DateTime(year, month, day)`.
    ///
    /// # Panics
    /// On an invalid date (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn new(year: i32, month: i32, day: i32) -> Self {
        Self::new_hms_ms(year, month, day, 0, 0, 0, 0)
    }

    /// `new DateTime(year, month, day, hour, minute, second)`.
    ///
    /// # Panics
    /// On an invalid date or time (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn new_hms(year: i32, month: i32, day: i32, hour: i32, minute: i32, second: i32) -> Self {
        Self::new_hms_ms(year, month, day, hour, minute, second, 0)
    }

    /// `new DateTime(year, month, day, hour, minute, second, millisecond)`.
    ///
    /// # Panics
    /// On an invalid date or time (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn new_hms_ms(
        year: i32,
        month: i32,
        day: i32,
        hour: i32,
        minute: i32,
        second: i32,
        millisecond: i32,
    ) -> Self {
        let (y, m, d) = (i64::from(year), i64::from(month), i64::from(day));
        assert!(
            (1..=9999).contains(&y) && (1..=12).contains(&m),
            "ArgumentOutOfRangeException: year/month"
        );
        let table = if is_leap_year(y) {
            &DAYS_TO_MONTH_366
        } else {
            &DAYS_TO_MONTH_365
        };
        let mi = usize::try_from(m).unwrap_or(1);
        assert!(
            d >= 1 && d <= table[mi] - table[mi - 1],
            "ArgumentOutOfRangeException: day"
        );
        assert!(
            (0..24).contains(&hour)
                && (0..60).contains(&minute)
                && (0..60).contains(&second)
                && (0..1000).contains(&millisecond),
            "ArgumentOutOfRangeException: time"
        );
        let y1 = y - 1;
        let days = y1 * 365 + y1 / 4 - y1 / 100 + y1 / 400 + table[mi - 1] + d - 1;
        let time = i64::from(hour) * TICKS_PER_HOUR
            + i64::from(minute) * TICKS_PER_MINUTE
            + i64::from(second) * TICKS_PER_SECOND
            + i64::from(millisecond) * TICKS_PER_MILLISECOND;
        Self {
            ticks: days * TICKS_PER_DAY + time,
        }
    }

    /// `Ticks`.
    #[must_use]
    pub fn ticks(self) -> i64 {
        self.ticks
    }

    /// `(year, month, day)` by the 400/100/4/1-year cycle walk of `GetDate`.
    fn date_parts(self) -> (i64, i64, i64) {
        let mut n = self.ticks / TICKS_PER_DAY;
        let y400 = n / DAYS_PER_400_YEARS;
        n -= y400 * DAYS_PER_400_YEARS;
        let mut y100 = n / DAYS_PER_100_YEARS;
        if y100 == 4 {
            y100 = 3;
        }
        n -= y100 * DAYS_PER_100_YEARS;
        let y4 = n / DAYS_PER_4_YEARS;
        n -= y4 * DAYS_PER_4_YEARS;
        let mut y1 = n / DAYS_PER_YEAR;
        if y1 == 4 {
            y1 = 3;
        }
        let year = y400 * 400 + y100 * 100 + y4 * 4 + y1 + 1;
        n -= y1 * DAYS_PER_YEAR;
        let leap = y1 == 3 && (y4 != 24 || y100 == 3);
        let table = if leap {
            &DAYS_TO_MONTH_366
        } else {
            &DAYS_TO_MONTH_365
        };
        let mut m = 1usize;
        while n >= table[m] {
            m += 1;
        }
        let month = i64::try_from(m).unwrap_or(1);
        (year, month, n - table[m - 1] + 1)
    }

    /// `Year`.
    #[must_use]
    pub fn year(self) -> i32 {
        i32_of(self.date_parts().0)
    }

    /// `Month`.
    #[must_use]
    pub fn month(self) -> i32 {
        i32_of(self.date_parts().1)
    }

    /// `Day`.
    #[must_use]
    pub fn day(self) -> i32 {
        i32_of(self.date_parts().2)
    }

    /// `Hour`.
    #[must_use]
    pub fn hour(self) -> i32 {
        i32_of((self.ticks / TICKS_PER_HOUR) % 24)
    }

    /// `Minute`.
    #[must_use]
    pub fn minute(self) -> i32 {
        i32_of((self.ticks / TICKS_PER_MINUTE) % 60)
    }

    /// `Second`.
    #[must_use]
    pub fn second(self) -> i32 {
        i32_of((self.ticks / TICKS_PER_SECOND) % 60)
    }

    /// `Millisecond`.
    #[must_use]
    pub fn millisecond(self) -> i32 {
        i32_of((self.ticks / TICKS_PER_MILLISECOND) % 1000)
    }

    /// `DayOfWeek` (0 = Sunday); 0001-01-01 was a Monday.
    #[must_use]
    pub fn day_of_week(self) -> i32 {
        i32_of((self.ticks / TICKS_PER_DAY + 1) % 7)
    }

    /// `Date`: midnight of the same day.
    #[must_use]
    pub fn date(self) -> Self {
        Self {
            ticks: self.ticks - self.ticks % TICKS_PER_DAY,
        }
    }

    /// `AddTicks`.
    ///
    /// # Panics
    /// When the result leaves `MinValue..=MaxValue` (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn add_ticks(self, ticks: i64) -> Self {
        Self::from_ticks(
            self.ticks
                .checked_add(ticks)
                .expect("ArgumentOutOfRangeException: AddTicks overflow"),
        )
    }

    /// `AddUnits` (.NET 7+): the integral part and the fractional part are converted to ticks
    /// separately, each truncated toward zero. (Earlier runtimes rounded to whole milliseconds.)
    fn add_units(self, value: f64, max_unit_count: i64, ticks_per_unit: i64) -> Self {
        #[allow(clippy::cast_precision_loss)]
        let limit = max_unit_count as f64;
        assert!(
            value.abs() <= limit,
            "ArgumentOutOfRangeException: value is out of range for Add"
        );
        let integral = value.trunc();
        let fractional = value - integral;
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let ticks =
            (integral as i64) * ticks_per_unit + (fractional * ticks_per_unit as f64) as i64;
        self.add_ticks(ticks)
    }

    /// `AddMilliseconds(double)`.
    #[must_use]
    pub fn add_milliseconds(self, value: f64) -> Self {
        self.add_units(value, DAYS_TO_10000 * 86_400_000, TICKS_PER_MILLISECOND)
    }

    /// `AddSeconds(double)`.
    #[must_use]
    pub fn add_seconds(self, value: f64) -> Self {
        self.add_units(value, DAYS_TO_10000 * 86_400, TICKS_PER_SECOND)
    }

    /// `AddMinutes(double)`.
    #[must_use]
    pub fn add_minutes(self, value: f64) -> Self {
        self.add_units(value, DAYS_TO_10000 * 1440, TICKS_PER_MINUTE)
    }

    /// `AddHours(double)`.
    #[must_use]
    pub fn add_hours(self, value: f64) -> Self {
        self.add_units(value, DAYS_TO_10000 * 24, TICKS_PER_HOUR)
    }

    /// `AddDays(double)`.
    #[must_use]
    pub fn add_days(self, value: f64) -> Self {
        self.add_units(value, DAYS_TO_10000, TICKS_PER_DAY)
    }

    /// `ToString(format)` in `en-US`.
    ///
    /// Standard formats: `""`/`G`, `g`, `d`, `D`, `t`, `T`, `s`, `u`. Custom specifiers: `d`,
    /// `M`, `y`, `h`, `H`, `m`, `s`, `f`, `F`, `t`, `:`, `/`, quotes, `\` and `%`.
    ///
    /// # Panics
    /// On a specifier outside that set (for example `z` or `K`).
    #[must_use]
    pub fn format(self, format: &str) -> String {
        let pattern = match format {
            "" | "G" => "M/d/yyyy h:mm:ss tt",
            "g" => "M/d/yyyy h:mm tt",
            "d" => "M/d/yyyy",
            "D" => "dddd, MMMM d, yyyy",
            "t" => "h:mm tt",
            "T" => "h:mm:ss tt",
            "s" => "yyyy'-'MM'-'dd'T'HH':'mm':'ss",
            "u" => "yyyy'-'MM'-'dd HH':'mm':'ss'Z'",
            f if f.chars().count() == 1 => {
                panic!("FormatException: DateTime standard format {f:?} is not supported")
            }
            f => f,
        };
        self.format_custom(pattern)
    }

    fn format_custom(self, pattern: &str) -> String {
        const MONTHS: [&str; 12] = [
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        const DAYS: [&str; 7] = [
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
        ];
        let (year, month, day) = self.date_parts();
        let hour = i64::from(self.hour());
        let chars: Vec<char> = pattern.chars().collect();
        let mut out = String::new();
        let mut i = 0usize;
        while i < chars.len() {
            let ch = chars[i];
            let mut len = 1usize;
            while i + len < chars.len() && chars[i + len] == ch {
                len += 1;
            }
            let pad = |v: i64, width: usize| format!("{v:0width$}");
            match ch {
                'd' => {
                    out += &match len {
                        1 => day.to_string(),
                        2 => pad(day, 2),
                        3 => DAYS[usize::try_from(self.day_of_week()).unwrap_or(0)][..3].to_owned(),
                        _ => DAYS[usize::try_from(self.day_of_week()).unwrap_or(0)].to_owned(),
                    };
                }
                'M' => {
                    let name = MONTHS[usize::try_from(month - 1).unwrap_or(0)];
                    out += &match len {
                        1 => month.to_string(),
                        2 => pad(month, 2),
                        3 => name[..3].to_owned(),
                        _ => name.to_owned(),
                    };
                }
                'y' => {
                    out += &match len {
                        1 => (year % 100).to_string(),
                        2 => pad(year % 100, 2),
                        n => pad(year, n),
                    };
                }
                'h' => {
                    let h12 = if hour % 12 == 0 { 12 } else { hour % 12 };
                    out += &pad(h12, len.min(2));
                }
                'H' => out += &pad(hour, len.min(2)),
                'm' => out += &pad(i64::from(self.minute()), len.min(2)),
                's' => out += &pad(i64::from(self.second()), len.min(2)),
                'f' | 'F' => {
                    assert!(len <= 7, "FormatException: too many fraction digits");
                    let exp = u32::try_from(7 - len).unwrap_or(0);
                    let fraction = (self.ticks % TICKS_PER_SECOND) / 10_i64.pow(exp);
                    let mut s = pad(fraction, len);
                    if ch == 'F' {
                        while s.ends_with('0') {
                            s.pop();
                        }
                        if s.is_empty() && out.ends_with('.') {
                            out.pop();
                        }
                    }
                    out += &s;
                }
                't' => {
                    let designator = if hour < 12 { "AM" } else { "PM" };
                    out += if len == 1 {
                        &designator[..1]
                    } else {
                        designator
                    };
                }
                ':' | '/' => {
                    len = 1;
                    out.push(ch);
                }
                '\'' | '"' => {
                    let mut j = i + 1;
                    while j < chars.len() && chars[j] != ch {
                        out.push(chars[j]);
                        j += 1;
                    }
                    i = j + 1;
                    continue;
                }
                '\\' => {
                    if let Some(&next) = chars.get(i + 1) {
                        out.push(next);
                    }
                    i += 2;
                    continue;
                }
                '%' => {
                    len = 1;
                }
                'z' | 'K' | 'g' => {
                    panic!("FormatException: DateTime specifier {ch:?} is not supported")
                }
                _ => {
                    len = 1;
                    out.push(ch);
                }
            }
            i += len;
        }
        out
    }
}

impl fmt::Display for DotNetDateTime {
    /// `ToString()`: the `en-US` general pattern `M/d/yyyy h:mm:ss tt`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format(""))
    }
}

impl Sub for DotNetDateTime {
    type Output = TimeSpan;
    fn sub(self, rhs: Self) -> TimeSpan {
        TimeSpan::from_ticks(self.ticks - rhs.ticks)
    }
}

impl Add<TimeSpan> for DotNetDateTime {
    type Output = Self;
    fn add(self, rhs: TimeSpan) -> Self {
        self.add_ticks(rhs.ticks)
    }
}

impl Sub<TimeSpan> for DotNetDateTime {
    type Output = Self;
    fn sub(self, rhs: TimeSpan) -> Self {
        self.add_ticks(-rhs.ticks)
    }
}

/// `TimeSpan.MaxValue.TotalMilliseconds` clamps here (`MaxMilliSeconds`).
const MAX_MILLISECONDS: i64 = i64::MAX / TICKS_PER_MILLISECOND;

impl TimeSpan {
    /// `TimeSpan.Zero`.
    pub const ZERO: Self = Self { ticks: 0 };
    /// `TimeSpan.MaxValue`.
    pub const MAX_VALUE: Self = Self { ticks: i64::MAX };
    /// `TimeSpan.MinValue`.
    pub const MIN_VALUE: Self = Self { ticks: i64::MIN };

    /// `new TimeSpan(ticks)`.
    #[must_use]
    pub const fn from_ticks(ticks: i64) -> Self {
        Self { ticks }
    }

    /// `Ticks`.
    #[must_use]
    pub const fn ticks(self) -> i64 {
        self.ticks
    }

    /// `Interval` + `IntervalFromDoubleTicks` (.NET Core 3.0+): `value * scale` truncated to
    /// ticks, no rounding to milliseconds.
    fn interval(value: f64, scale: f64) -> Self {
        assert!(
            !value.is_nan(),
            "ArgumentException: TimeSpan does not accept floating point Not-a-Number values."
        );
        let ticks = value * scale;
        #[allow(clippy::cast_precision_loss)]
        let (max, min) = (i64::MAX as f64, i64::MIN as f64);
        assert!(
            ticks <= max && ticks >= min,
            "OverflowException: TimeSpan overflowed"
        );
        if ticks == max {
            return Self::MAX_VALUE;
        }
        #[allow(clippy::cast_possible_truncation)]
        Self {
            ticks: ticks as i64,
        }
    }

    /// `TimeSpan.FromMilliseconds(double)`.
    #[must_use]
    pub fn from_milliseconds(value: f64) -> Self {
        Self::interval(value, 10_000.0)
    }

    /// `TimeSpan.FromSeconds(double)`.
    #[must_use]
    pub fn from_seconds(value: f64) -> Self {
        Self::interval(value, 10_000_000.0)
    }

    /// `TimeSpan.FromMinutes(double)`.
    #[must_use]
    pub fn from_minutes(value: f64) -> Self {
        Self::interval(value, 600_000_000.0)
    }

    /// `TimeSpan.FromHours(double)`.
    #[must_use]
    pub fn from_hours(value: f64) -> Self {
        Self::interval(value, 36_000_000_000.0)
    }

    /// `TimeSpan.FromDays(double)`.
    #[must_use]
    pub fn from_days(value: f64) -> Self {
        Self::interval(value, 864_000_000_000.0)
    }

    /// `Days`.
    #[must_use]
    pub fn days(self) -> i32 {
        i32_of(self.ticks / TICKS_PER_DAY)
    }

    /// `Hours`.
    #[must_use]
    pub fn hours(self) -> i32 {
        i32_of((self.ticks / TICKS_PER_HOUR) % 24)
    }

    /// `Minutes`.
    #[must_use]
    pub fn minutes(self) -> i32 {
        i32_of((self.ticks / TICKS_PER_MINUTE) % 60)
    }

    /// `Seconds`.
    #[must_use]
    pub fn seconds(self) -> i32 {
        i32_of((self.ticks / TICKS_PER_SECOND) % 60)
    }

    /// `Milliseconds`.
    #[must_use]
    pub fn milliseconds(self) -> i32 {
        i32_of((self.ticks / TICKS_PER_MILLISECOND) % 1000)
    }

    /// `TotalDays` (`(double)ticks / TicksPerDay`).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn total_days(self) -> f64 {
        self.ticks as f64 / TICKS_PER_DAY as f64
    }

    /// `TotalHours`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn total_hours(self) -> f64 {
        self.ticks as f64 / TICKS_PER_HOUR as f64
    }

    /// `TotalMinutes`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn total_minutes(self) -> f64 {
        self.ticks as f64 / TICKS_PER_MINUTE as f64
    }

    /// `TotalSeconds`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn total_seconds(self) -> f64 {
        self.ticks as f64 / TICKS_PER_SECOND as f64
    }

    /// `TotalMilliseconds`, clamped to `±(long.MaxValue / TicksPerMillisecond)`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn total_milliseconds(self) -> f64 {
        let temp = self.ticks as f64 / TICKS_PER_MILLISECOND as f64;
        let max = MAX_MILLISECONDS as f64;
        if temp > max {
            return max;
        }
        if temp < -max {
            return -max;
        }
        temp
    }

    /// `Duration()`.
    ///
    /// # Panics
    /// For `MinValue` (`OverflowException`).
    #[must_use]
    pub fn duration(self) -> Self {
        Self {
            ticks: self
                .ticks
                .checked_abs()
                .expect("OverflowException: Duration of MinValue"),
        }
    }

    /// `ToString(format)`: `""`/`c` is the constant format `[-][d.]hh:mm:ss[.fffffff]`; custom
    /// formats support `d`, `h`, `m`, `s`, `f` and `F` (absolute component values), quotes, `\` and
    /// `%`.
    ///
    /// # Panics
    /// On any other specifier.
    #[must_use]
    pub fn format(self, format: &str) -> String {
        if format.is_empty() || format == "c" || format == "t" || format == "T" {
            return self.constant();
        }
        let ticks = self.ticks.unsigned_abs();
        let per = |t: i64| u64::try_from(t).unwrap_or(1);
        let days = ticks / per(TICKS_PER_DAY);
        let hours = (ticks / per(TICKS_PER_HOUR)) % 24;
        let minutes = (ticks / per(TICKS_PER_MINUTE)) % 60;
        let seconds = (ticks / per(TICKS_PER_SECOND)) % 60;
        let fraction = ticks % per(TICKS_PER_SECOND);
        let chars: Vec<char> = format.chars().collect();
        let mut out = String::new();
        let mut i = 0usize;
        while i < chars.len() {
            let ch = chars[i];
            let mut len = 1usize;
            while i + len < chars.len() && chars[i + len] == ch {
                len += 1;
            }
            match ch {
                'd' => out += &format!("{days:0len$}"),
                'h' => out += &format!("{hours:0w$}", w = len.min(2)),
                'm' => out += &format!("{minutes:0w$}", w = len.min(2)),
                's' => out += &format!("{seconds:0w$}", w = len.min(2)),
                'f' | 'F' => {
                    let exp = u32::try_from(7 - len.min(7)).unwrap_or(0);
                    let mut s = format!("{:0len$}", fraction / 10_u64.pow(exp));
                    if ch == 'F' {
                        while s.ends_with('0') {
                            s.pop();
                        }
                    }
                    out += &s;
                }
                '%' => len = 1,
                '\'' | '"' => {
                    let mut j = i + 1;
                    while j < chars.len() && chars[j] != ch {
                        out.push(chars[j]);
                        j += 1;
                    }
                    i = j + 1;
                    continue;
                }
                '\\' => {
                    if let Some(&next) = chars.get(i + 1) {
                        out.push(next);
                    }
                    i += 2;
                    continue;
                }
                other => {
                    panic!("FormatException: TimeSpan format character {other:?} is not supported")
                }
            }
            i += len;
        }
        out
    }

    fn constant(self) -> String {
        let ticks = self.ticks.unsigned_abs();
        let per = |t: i64| u64::try_from(t).unwrap_or(1);
        let days = ticks / per(TICKS_PER_DAY);
        let hours = (ticks / per(TICKS_PER_HOUR)) % 24;
        let minutes = (ticks / per(TICKS_PER_MINUTE)) % 60;
        let seconds = (ticks / per(TICKS_PER_SECOND)) % 60;
        let fraction = ticks % per(TICKS_PER_SECOND);
        let mut s = String::new();
        if self.ticks < 0 {
            s.push('-');
        }
        if days != 0 {
            s += &format!("{days}.");
        }
        s += &format!("{hours:02}:{minutes:02}:{seconds:02}");
        if fraction != 0 {
            s += &format!(".{fraction:07}");
        }
        s
    }
}

impl fmt::Display for TimeSpan {
    /// `ToString()`: the constant (`c`) format.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.constant())
    }
}

impl Add for TimeSpan {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            ticks: self
                .ticks
                .checked_add(rhs.ticks)
                .expect("OverflowException: TimeSpan overflowed"),
        }
    }
}

impl Sub for TimeSpan {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            ticks: self
                .ticks
                .checked_sub(rhs.ticks)
                .expect("OverflowException: TimeSpan overflowed"),
        }
    }
}

impl std::ops::Neg for TimeSpan {
    type Output = Self;
    fn neg(self) -> Self {
        Self { ticks: self.ticks.checked_neg().expect("OverflowException: Negating the minimum value of a twos complement number is invalid.") }
    }
}
