// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/DerethDateTime.cs
//! `ACE.Common.DerethDateTime`: the Portal Year calendar.
//!
//! ACE implements the calendar through private property setters with side effects (setting `Day`
//! past 30 bumps `Month`, which can bump `Year`; setting `Ticks` to the minimum resets every field).
//! Those setters are ported as the private `set_*` methods and called in ACE's order, so every
//! quirk is kept, including the `Ticks` reset that makes `new DerethDateTime(10, 1, 1, 1)` report
//! Morntide-and-Half.
//!
//! DIVERGE: the `UtcNowTo*` properties read `DateTime.UtcNow`; here they take the injected
//! [`Clock`].

use crate::clock::Clock;
use crate::dotnet::datetime::DotNetDateTime;
use crate::dotnet::math::{self, MidpointRounding};
use crate::dotnet::CsCast;

// ACE: DerethDateTime.hoursInADay
const HOURS_IN_A_DAY: i32 = 16;
// ACE: DerethDateTime.daysInAMonth
const DAYS_IN_A_MONTH: i32 = 30;
// ACE: DerethDateTime.monthsInAYear
const MONTHS_IN_A_YEAR: i32 = 12;
// ACE: DerethDateTime.dayTicks
const DAY_TICKS: f64 = 7620.0;
// ACE: DerethDateTime.hourTicks
const HOUR_TICKS: f64 = DAY_TICKS / HOURS_IN_A_DAY as f64;
// ACE: DerethDateTime.minuteTicks
#[allow(dead_code)] // unused in ACE too
const MINUTE_TICKS: f64 = HOUR_TICKS / 60.0;
// ACE: DerethDateTime.secondTicks
#[allow(dead_code)] // unused in ACE too
const SECOND_TICKS: f64 = MINUTE_TICKS / 60.0;
// ACE: DerethDateTime.monthTicks
const MONTH_TICKS: f64 = DAY_TICKS * DAYS_IN_A_MONTH as f64;
// ACE: DerethDateTime.yearTicks
const YEAR_TICKS: f64 = MONTH_TICKS * MONTHS_IN_A_YEAR as f64;
// ACE: DerethDateTime.dayZeroTicks
const DAY_ZERO_TICKS: f64 = 0.0;
// ACE: DerethDateTime.hourOneTicks
const HOUR_ONE_TICKS: f64 = 210.0;
// ACE: DerethDateTime.dayOneTicks
const DAY_ONE_TICKS: f64 = DAY_ZERO_TICKS + HOUR_ONE_TICKS + (HOUR_TICKS * 8.0);

/// The Portal Year months (`DerethDateTime.Months`). The year begins at Morningthaw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)]
pub enum Months {
    Snowreap = -2,
    Coldeve = -1,
    Wintersebb = 0,
    Morningthaw = 1,
    Solclaim = 2,
    Seedsow = 3,
    Leafdawning = 4,
    Verdantine = 5,
    Thistledown = 6,
    HarvestGain = 7,
    Leafcull = 8,
    Frostfell = 9,
}

impl Months {
    /// The enum member with this value, if any.
    #[must_use]
    pub fn from_i32(v: i32) -> Option<Self> {
        use Months::*;
        Some(match v {
            -2 => Snowreap,
            -1 => Coldeve,
            0 => Wintersebb,
            1 => Morningthaw,
            2 => Solclaim,
            3 => Seedsow,
            4 => Leafdawning,
            5 => Verdantine,
            6 => Thistledown,
            7 => HarvestGain,
            8 => Leafcull,
            9 => Frostfell,
            _ => return None,
        })
    }

    /// `Enum.GetName`.
    #[must_use]
    pub fn name(self) -> &'static str {
        use Months::*;
        match self {
            Snowreap => "Snowreap",
            Coldeve => "Coldeve",
            Wintersebb => "Wintersebb",
            Morningthaw => "Morningthaw",
            Solclaim => "Solclaim",
            Seedsow => "Seedsow",
            Leafdawning => "Leafdawning",
            Verdantine => "Verdantine",
            Thistledown => "Thistledown",
            HarvestGain => "HarvestGain",
            Leafcull => "Leafcull",
            Frostfell => "Frostfell",
        }
    }
}

/// `DerethDateTime.Seasons`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Seasons {
    Winter,
    Spring,
    Summer,
    Autumn,
}

/// The sixteen hours of a Derethian day (`DerethDateTime.Hours`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs, non_camel_case_types)]
pub enum Hours {
    Darktide = 1,
    Darktide_and_Half,
    Foredawn,
    Foredawn_and_Half,
    Dawnsong,
    Dawnsong_and_Half,
    Morntide,
    Morntide_and_Half,
    Midsong,
    Midsong_and_Half,
    Warmtide,
    Warmtide_and_Half,
    Evensong,
    Evensong_and_Half,
    Gloaming,
    Gloaming_and_Half,
}

const HOUR_NAMES: [&str; 16] = [
    "Darktide",
    "Darktide_and_Half",
    "Foredawn",
    "Foredawn_and_Half",
    "Dawnsong",
    "Dawnsong_and_Half",
    "Morntide",
    "Morntide_and_Half",
    "Midsong",
    "Midsong_and_Half",
    "Warmtide",
    "Warmtide_and_Half",
    "Evensong",
    "Evensong_and_Half",
    "Gloaming",
    "Gloaming_and_Half",
];

impl Hours {
    /// The enum member with this value, if any.
    #[must_use]
    pub fn from_i32(v: i32) -> Option<Self> {
        use Hours::*;
        const ALL: [Hours; 16] = [
            Darktide,
            Darktide_and_Half,
            Foredawn,
            Foredawn_and_Half,
            Dawnsong,
            Dawnsong_and_Half,
            Morntide,
            Morntide_and_Half,
            Midsong,
            Midsong_and_Half,
            Warmtide,
            Warmtide_and_Half,
            Evensong,
            Evensong_and_Half,
            Gloaming,
            Gloaming_and_Half,
        ];
        usize::try_from(v - 1)
            .ok()
            .and_then(|i| ALL.get(i).copied())
    }

    /// `Enum.GetName` (with the underscores ACE later replaces).
    #[must_use]
    pub fn name(self) -> &'static str {
        HOUR_NAMES[self as usize - 1]
    }
}

/// `DerethDateTime.Days`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Days {
    FirstDay = 1,
    StarDay,
    EarthDay,
    MoonsDay,
    ElderDay,
    FreeDay,
}

/// `DerethDateTime.Daytime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Daytime {
    Day,
    Night,
}

/// `DerethDateTime`: a date and time in the Portal Year calendar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DerethDateTime {
    ticks: f64,
    day: i32,
    month: i32,
    year: i32,
    hour: i32,
}

/// `Enum.GetName(typeof(Months), v)`; `null` concatenates as "".
fn month_name(v: i32) -> &'static str {
    Months::from_i32(v).map_or("", Months::name)
}

/// `Enum.GetName(typeof(Hours), v).Replace("_", "-")`.
fn hour_name_dashed(v: i32) -> String {
    Hours::from_i32(v).map_or(String::new(), |h| h.name().replace('_', "-"))
}

impl DerethDateTime {
    // ACE: DerethDateTime.MinValue
    /// Morningthaw 1, 10 P.Y., Morntide-and-Half.
    pub const MIN_VALUE: f64 = 0.0;

    // ACE: DerethDateTime.MaxValue
    /// Thistledown 2, 401 P.Y., Morntide-and-Half: 1073741824 (ACE's comment says 1073741828).
    pub const MAX_VALUE: f64 = (YEAR_TICKS * 391.0) + (MONTH_TICKS * 5.0) + (DAY_TICKS * 1.0) + 4.0;

    /// The field initialisers.
    fn blank() -> Self {
        Self {
            ticks: Self::MIN_VALUE,
            day: 1,
            month: Months::Morningthaw as i32,
            year: 10,
            hour: Hours::Morntide_and_Half as i32,
        }
    }

    // ---- property setters (private in ACE) ----

    // ACE: DerethDateTime.Ticks
    fn set_ticks(&mut self, value: f64) {
        if (Self::MIN_VALUE..=Self::MAX_VALUE).contains(&value) {
            self.ticks = value;
        }
        if value <= Self::MIN_VALUE {
            self.ticks = Self::MIN_VALUE;
            self.day = 1;
            self.month = Months::Morningthaw as i32;
            self.year = 10;
            self.hour = Hours::Morntide_and_Half as i32;
        }
        if value >= Self::MAX_VALUE {
            self.ticks = Self::MAX_VALUE;
            self.day = 2;
            self.month = Months::Thistledown as i32;
            self.year = 401;
            self.hour = Hours::Morntide_and_Half as i32;
        }
    }

    // ACE: DerethDateTime.Day
    fn set_day(&mut self, value: i32) {
        if (1..=30).contains(&value) {
            self.day = value;
        }
        if value < 1 {
            self.set_month(self.month - 1);
            self.day = 30;
        }
        if value > 30 {
            self.set_month(self.month + 1);
            self.day = 1;
        }
    }

    // ACE: DerethDateTime.Month
    fn set_month(&mut self, value: i32) {
        if (Months::Snowreap as i32..=Months::Frostfell as i32).contains(&value) {
            if self.month == Months::Morningthaw as i32 && value == Months::Wintersebb as i32 {
                self.set_year(self.year - 1);
            }
            if self.month == Months::Wintersebb as i32 && value == Months::Morningthaw as i32 {
                self.set_year(self.year + 1);
            }
            self.month = value;
        }
        if value < Months::Snowreap as i32 {
            self.month = Months::Snowreap as i32;
        }
        if value > Months::Frostfell as i32 {
            self.month = Months::Snowreap as i32;
        }
    }

    // ACE: DerethDateTime.Year
    #[allow(clippy::manual_clamp)]
    fn set_year(&mut self, value: i32) {
        if (10..=401).contains(&value) {
            self.year = value;
        }
        if value < 10 {
            self.year = 10;
        }
        if value > 401 {
            self.year = 401;
        }
    }

    // ACE: DerethDateTime.Hour
    fn set_hour(&mut self, value: i32) {
        if (Hours::Darktide as i32..=Hours::Gloaming_and_Half as i32).contains(&value) {
            self.hour = value;
        }
        if value < Hours::Darktide as i32 {
            self.set_day(self.day - 1);
            self.hour = Hours::Gloaming_and_Half as i32;
        }
        if value > Hours::Gloaming_and_Half as i32 {
            self.set_day(self.day + 1);
            self.hour = Hours::Darktide as i32;
        }
    }

    // ---- getters ----

    /// `Ticks`.
    #[must_use]
    pub fn ticks(&self) -> f64 {
        self.ticks
    }

    /// `Day`.
    #[must_use]
    pub fn day(&self) -> i32 {
        self.day
    }

    /// `Month`.
    #[must_use]
    pub fn month(&self) -> i32 {
        self.month
    }

    // ACE: DerethDateTime.MonthName
    /// `MonthName`.
    ///
    /// # Panics
    /// Never: the month setter keeps the value in range.
    #[must_use]
    pub fn month_name(&self) -> Months {
        Months::from_i32(self.month).expect("month is always a Months value")
    }

    /// `Year`.
    #[must_use]
    pub fn year(&self) -> i32 {
        self.year
    }

    // ACE: DerethDateTime.PY
    /// `PY`.
    #[must_use]
    pub fn py(&self) -> i32 {
        self.year
    }

    // ACE: DerethDateTime.PortalYear
    /// `PortalYear`.
    #[must_use]
    pub fn portal_year(&self) -> i32 {
        self.year
    }

    /// `Hour`.
    #[must_use]
    pub fn hour(&self) -> i32 {
        self.hour
    }

    // ACE: DerethDateTime.HourName
    /// `HourName`.
    ///
    /// # Panics
    /// Never: the hour setter keeps the value in range.
    #[must_use]
    pub fn hour_name(&self) -> Hours {
        Hours::from_i32(self.hour).expect("hour is always an Hours value")
    }

    // ACE: DerethDateTime.Time
    /// `Time`.
    #[must_use]
    pub fn time(&self) -> i32 {
        self.hour
    }

    // ACE: DerethDateTime.TimeName
    /// `TimeName`.
    #[must_use]
    pub fn time_name(&self) -> Hours {
        self.hour_name()
    }

    // ACE: DerethDateTime.TimeOfDay
    /// `TimeOfDay`: Dawnsong through Warmtide-and-Half is day.
    #[must_use]
    pub fn time_of_day(&self) -> Daytime {
        if self.hour >= Hours::Dawnsong as i32 && self.hour <= Hours::Warmtide_and_Half as i32 {
            return Daytime::Day;
        }
        Daytime::Night
    }

    // ACE: DerethDateTime.IsDaytime
    /// `IsDaytime`.
    #[must_use]
    pub fn is_daytime(&self) -> bool {
        self.time_of_day() == Daytime::Day
    }

    // ACE: DerethDateTime.IsNighttime
    /// `IsNighttime`.
    #[must_use]
    pub fn is_nighttime(&self) -> bool {
        self.time_of_day() == Daytime::Night
    }

    // ACE: DerethDateTime.IsDay
    /// `IsDay`.
    #[must_use]
    pub fn is_day(&self) -> bool {
        self.is_daytime()
    }

    // ACE: DerethDateTime.IsNight
    /// `IsNight`.
    #[must_use]
    pub fn is_night(&self) -> bool {
        self.is_nighttime()
    }

    // ACE: DerethDateTime.Season
    /// `Season`.
    #[must_use]
    pub fn season(&self) -> Seasons {
        let m = self.month;
        if m >= Months::Snowreap as i32 && m <= Months::Wintersebb as i32 {
            return Seasons::Winter;
        }
        if m >= Months::Morningthaw as i32 && m <= Months::Seedsow as i32 {
            return Seasons::Spring;
        }
        if m >= Months::Leafdawning as i32 && m <= Months::Thistledown as i32 {
            return Seasons::Summer;
        }
        Seasons::Autumn
    }

    // ACE: DerethDateTime.IsSeason
    /// `IsSeason(seasonToCheckFor)`.
    #[must_use]
    pub fn is_season(&self, season_to_check_for: Seasons) -> bool {
        self.season() == season_to_check_for
    }

    // ACE: DerethDateTime.IsWinter
    /// `IsWinter`.
    #[must_use]
    pub fn is_winter(&self) -> bool {
        self.season() == Seasons::Winter
    }

    // ACE: DerethDateTime.IsSpring
    /// `IsSpring`.
    #[must_use]
    pub fn is_spring(&self) -> bool {
        self.season() == Seasons::Spring
    }

    // ACE: DerethDateTime.IsSummer
    /// `IsSummer`.
    #[must_use]
    pub fn is_summer(&self) -> bool {
        self.season() == Seasons::Summer
    }

    // ACE: DerethDateTime.IsAutumn
    /// `IsAutumn`.
    #[must_use]
    pub fn is_autumn(&self) -> bool {
        self.season() == Seasons::Autumn
    }

    // ACE: DerethDateTime.IsFall
    /// `IsFall`.
    #[must_use]
    pub fn is_fall(&self) -> bool {
        self.is_autumn()
    }

    // ---- constructors ----

    // ACE: DerethDateTime.DerethDateTime()
    /// `new DerethDateTime()`: Morningthaw 1, 10 P.Y., Morntide-and-Half.
    #[must_use]
    pub fn new() -> Self {
        let mut d = Self::blank();
        d.set_ticks(Self::MIN_VALUE);
        d.set_year(10);
        d.set_month(Months::Morningthaw as i32);
        d.set_day(1);
        d.set_hour(Hours::Morntide_and_Half as i32);
        d
    }

    // ACE: DerethDateTime.DerethDateTime(double)
    /// `new DerethDateTime(ticks)`.
    ///
    /// # Panics
    /// When `ticks` is outside `MinValue..=MaxValue` (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn from_ticks(ticks: f64) -> Self {
        // `ticks < MinValue | ticks > MaxValue` is false for NaN, as in C#.
        assert!(
            !(ticks < Self::MIN_VALUE || ticks > Self::MAX_VALUE),
            "ArgumentOutOfRangeException: ticks is less than DerethDateTime.MinValue or greater than DerethDateTime.MaxValue"
        );
        let mut d = Self::blank();
        d.set_ticks(ticks);
        if d.ticks < Self::MAX_VALUE {
            d.set_date_time_from_ticks();
        }
        d
    }

    // ACE: DerethDateTime.SetDateTimeFromTicks
    fn set_date_time_from_ticks(&mut self) {
        let hours_in_ticks = self.ticks / HOUR_TICKS;
        let round = math::round_mode(hours_in_ticks * 4.0, MidpointRounding::ToEven) / 4.0;
        let round_multiplier = 100;
        let whole: i32 = round.cs_cast();
        let round_decimals: i32 =
            ((round - f64::from(whole)) * f64::from(round_multiplier)).cs_cast();

        let mut i = 0.0;
        while i < round {
            self.set_hour(self.hour + 1);
            i += 1.0;
        }
        if round_decimals == 25 {
            self.set_hour(self.hour - 1);
        }
    }

    // ACE: DerethDateTime.DerethDateTime(int, int, int, int)
    /// `new DerethDateTime(year, month, day, hour)` with integer month and hour.
    ///
    /// # Panics
    /// When a component is out of range (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn from_ymdh(year: i32, month: i32, day: i32, hour: i32) -> Self {
        assert!(
            (10..=401).contains(&year),
            "ArgumentOutOfRangeException: year is less than 10 or greater than 401"
        );
        assert!(
            (Months::Snowreap as i32..=Months::Frostfell as i32).contains(&month),
            "ArgumentOutOfRangeException: month is less than Snowreap or greater than Frostfell"
        );
        assert!(
            (1..=30).contains(&day),
            "ArgumentOutOfRangeException: day is less than 1 or greater than 30"
        );
        assert!(
            (Hours::Darktide as i32..=Hours::Gloaming_and_Half as i32).contains(&hour),
            "ArgumentOutOfRangeException: time is less than Darktide or greater than Gloaming_and_Half"
        );

        let mut d = Self::blank();
        d.set_year(year);
        d.set_month(month);
        d.set_day(day);

        d.set_hour(hour);

        let ticks = if month == Months::Wintersebb as i32 && year > 10 {
            d.get_ticks_from_date_time(true)
        } else {
            d.get_ticks_from_date_time(false)
        };
        d.set_ticks(ticks);
        d
    }

    // ACE: DerethDateTime.GetTicksFromDateTime
    fn get_ticks_from_date_time(&mut self, fix_wintersebb_year_roll_under: bool) -> f64 {
        let mut ticks = 0.0;

        if self.year == 10
            && self.month == Months::Morningthaw as i32
            && self.day == 1
            && self.hour < Hours::Morntide_and_Half as i32
        {
            return ticks;
        }

        if self.year == 10
            && self.month == Months::Morningthaw as i32
            && self.day == 1
            && self.hour == Hours::Midsong as i32
        {
            return HOUR_ONE_TICKS;
        }

        ticks += HOUR_ONE_TICKS;

        if self.year == 10
            && self.month == Months::Morningthaw as i32
            && self.day == 1
            && self.hour > Hours::Midsong as i32
        {
            return ticks + (f64::from(self.hour - 9) * HOUR_TICKS);
        }

        ticks += DAY_ONE_TICKS;

        let years_to_add = if self.month > Months::Wintersebb as i32 {
            f64::from(self.year - 10) * YEAR_TICKS
        } else {
            if fix_wintersebb_year_roll_under {
                self.set_year(self.year + 1);
            }
            f64::from(self.year - 9) * YEAR_TICKS
        };

        let months_to_add = f64::from(self.month - 1) * MONTH_TICKS;
        let days_to_add = f64::from(self.day - 2) * DAY_TICKS;
        let hours_to_add = f64::from(self.hour - 1) * HOUR_TICKS;

        ticks + years_to_add + months_to_add + days_to_add + hours_to_add
    }

    // ACE: DerethDateTime.DerethDateTime(int, Months, int, Hours)
    /// `new DerethDateTime(year, month, day, time)` with the enum overload.
    ///
    /// # Panics
    /// When a component is out of range (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn from_ymdh_named(year: i32, month: Months, day: i32, time: Hours) -> Self {
        // The enum overload differs from the integer one only in its parameter types; the body is
        // the same statement for statement.
        Self::from_ymdh(year, month as i32, day, time as i32)
    }

    // ---- arithmetic ----

    // ACE: DerethDateTime.AddTicks
    /// `AddTicks(numOfTicksToAdd)`.
    ///
    /// # Panics
    /// When the result leaves the calendar (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn add_ticks(&self, num_of_ticks_to_add: f64) -> Self {
        let t = self.ticks + num_of_ticks_to_add;
        assert!(
            !(t < Self::MIN_VALUE || t > Self::MAX_VALUE),
            "ArgumentOutOfRangeException: numOfTicksToAdd results in less than DerethDateTime.MinValue or greater than DerethDateTime.MaxValue"
        );
        Self::from_ticks(t)
    }

    // ACE: DerethDateTime.SubtractTicks
    /// `SubtractTicks(numOfTicksToSubtract)`.
    ///
    /// # Panics
    /// When the range check or the constructor rejects the value (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn subtract_ticks(&self, num_of_ticks_to_subtract: f64) -> Self {
        // ACE-BUG: the range check adds the ticks, but the result subtracts them.
        let t = self.ticks + num_of_ticks_to_subtract;
        assert!(
            !(t < Self::MIN_VALUE || t > Self::MAX_VALUE),
            "ArgumentOutOfRangeException: numOfTicksToSubtract results in less than DerethDateTime.MinValue or greater than DerethDateTime.MaxValue"
        );
        Self::from_ticks(self.ticks - num_of_ticks_to_subtract)
    }

    // ACE: DerethDateTime.AddYears
    /// `AddYears(numOfYearsToAdd)`.
    ///
    /// # Panics
    /// When the year leaves 10..=401 (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn add_years(&self, num_of_years_to_add: i32) -> Self {
        let y = self.year + num_of_years_to_add;
        assert!((10..=401).contains(&y), "ArgumentOutOfRangeException: numOfYearsToAdd results in a portal year less than 10 or greater than 401");
        Self::from_ymdh(y, self.month, self.day, self.hour)
    }

    // ACE: DerethDateTime.SubtractYears
    /// `SubtractYears(numOfYearsToSubtract)`.
    ///
    /// # Panics
    /// When the year leaves 10..=401 (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn subtract_years(&self, num_of_years_to_subtract: i32) -> Self {
        let y = self.year - num_of_years_to_subtract;
        assert!(
            (10..=401).contains(&y),
            "ArgumentOutOfRangeException: numOfYearsToSubtract results in a portal year less than 10 or greater than 401"
        );
        Self::from_ymdh(y, self.month, self.day, self.hour)
    }

    /// One month forward in ACE's month walk.
    fn month_forward(new_year: &mut i32, new_month: &mut i32) {
        if *new_month == Months::Wintersebb as i32 {
            *new_year += 1;
            *new_month += 1;
        } else if *new_month == Months::Frostfell as i32 {
            *new_month = Months::Snowreap as i32;
        } else {
            *new_month += 1;
        }
    }

    /// One month back in ACE's month walk.
    fn month_back(new_year: &mut i32, new_month: &mut i32) {
        if *new_month == Months::Morningthaw as i32 {
            *new_year -= 1;
            *new_month -= 1;
        } else if *new_month == Months::Snowreap as i32 {
            *new_month = Months::Frostfell as i32;
        } else {
            *new_month -= 1;
        }
    }

    // ACE: DerethDateTime.AddMonths
    /// `AddMonths(numOfMonthsToAdd)`.
    ///
    /// # Panics
    /// When the year leaves 10..=401 (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn add_months(&self, num_of_months_to_add: i32) -> Self {
        let mut new_year = self.year;
        let mut new_month = self.month;

        if num_of_months_to_add > 0 {
            for _ in 0..num_of_months_to_add {
                Self::month_forward(&mut new_year, &mut new_month);
            }
        } else {
            for _ in num_of_months_to_add..0 {
                Self::month_back(&mut new_year, &mut new_month);
            }
        }

        assert!(
            (10..=401).contains(&new_year),
            "ArgumentOutOfRangeException: numOfMonthsToAdd results in a portal year less than 10 or greater than 401"
        );
        Self::from_ymdh(new_year, new_month, self.day, self.hour)
    }

    // ACE: DerethDateTime.SubtractMonths
    /// `SubtractMonths(numOfMonthsToSubtract)`: rebuilds from `Ticks` first, then adds the
    /// negated count.
    #[must_use]
    pub fn subtract_months(&self, num_of_months_to_subtract: i32) -> Self {
        let opposite = num_of_months_to_subtract.wrapping_mul(-1);
        Self::from_ticks(self.ticks).add_months(opposite)
    }

    // ACE: DerethDateTime.AddDays
    /// `AddDays(numOfDaysToAdd)`.
    ///
    /// # Panics
    /// When the year leaves 10..=401 (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn add_days(&self, num_of_days_to_add: i32) -> Self {
        let mut new_year = self.year;
        let mut new_month = self.month;
        let mut new_day = self.day;

        if num_of_days_to_add > 0 {
            for _ in 0..num_of_days_to_add {
                if new_day == 30 {
                    Self::month_forward(&mut new_year, &mut new_month);
                    new_day = 1;
                } else {
                    new_day += 1;
                }
            }
        } else {
            for _ in num_of_days_to_add..0 {
                if new_day == 1 {
                    Self::month_back(&mut new_year, &mut new_month);
                    new_day = 30;
                } else {
                    new_day -= 1;
                }
            }
        }

        assert!(
            (10..=401).contains(&new_year),
            "ArgumentOutOfRangeException: numOfDaysToAdd results in a portal year less than 10 or greater than 401"
        );
        Self::from_ymdh(new_year, new_month, new_day, self.hour)
    }

    // ACE: DerethDateTime.SubtractDays
    /// `SubtractDays(numOfDaysToSubtract)`: rebuilds from `Ticks` first.
    #[must_use]
    pub fn subtract_days(&self, num_of_days_to_subtract: i32) -> Self {
        let opposite = num_of_days_to_subtract.wrapping_mul(-1);
        Self::from_ticks(self.ticks).add_days(opposite)
    }

    // ACE: DerethDateTime.AddHours
    /// `AddHours(numOfHoursToAdd)`.
    ///
    /// # Panics
    /// When the year leaves 10..=401 (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn add_hours(&self, num_of_hours_to_add: i32) -> Self {
        let mut new_year = self.year;
        let mut new_month = self.month;
        let mut new_day = self.day;
        let mut new_hour = self.hour;

        if num_of_hours_to_add > 0 {
            for _ in 0..num_of_hours_to_add {
                if new_hour == Hours::Gloaming_and_Half as i32 {
                    if new_day == 30 {
                        Self::month_forward(&mut new_year, &mut new_month);
                        new_day = 1;
                    } else {
                        new_day += 1;
                    }
                    new_hour = Hours::Darktide as i32;
                } else {
                    new_hour += 1;
                }
            }
        } else {
            for _ in num_of_hours_to_add..0 {
                if new_hour == Hours::Darktide as i32 {
                    if new_day == 1 {
                        Self::month_back(&mut new_year, &mut new_month);
                        new_day = 30;
                    } else {
                        new_day -= 1;
                    }
                    new_hour = Hours::Gloaming_and_Half as i32;
                } else {
                    new_hour -= 1;
                }
            }
        }

        assert!(
            (10..=401).contains(&new_year),
            "ArgumentOutOfRangeException: numOfDaysToAdd results in a portal year less than 10 or greater than 401"
        );
        Self::from_ymdh(new_year, new_month, new_day, new_hour)
    }

    // ACE: DerethDateTime.SubtractHours
    /// `SubtractHours(numOfHoursToSubtract)`: rebuilds from `Ticks` first.
    #[must_use]
    pub fn subtract_hours(&self, num_of_hours_to_subtract: i32) -> Self {
        let opposite = num_of_hours_to_subtract.wrapping_mul(-1);
        Self::from_ticks(self.ticks).add_hours(opposite)
    }

    // ---- strings ----

    // ACE: DerethDateTime.MonthToString
    /// `MonthToString()`.
    #[must_use]
    pub fn month_to_string(&self) -> String {
        month_name(self.month).to_owned()
    }

    // ACE: DerethDateTime.MonthNameToString
    /// `MonthNameToString()`.
    #[must_use]
    pub fn month_name_to_string(&self) -> String {
        month_name(self.month).to_owned()
    }

    // ACE: DerethDateTime.HourToString
    /// `HourToString()`.
    #[must_use]
    pub fn hour_to_string(&self) -> String {
        hour_name_dashed(self.hour)
    }

    // ACE: DerethDateTime.HourNameToString
    /// `HourNameToString()`.
    #[must_use]
    pub fn hour_name_to_string(&self) -> String {
        hour_name_dashed(self.hour)
    }

    // ACE: DerethDateTime.TimeToString
    /// `TimeToString()`.
    #[must_use]
    pub fn time_to_string(&self) -> String {
        hour_name_dashed(self.hour)
    }

    // ACE: DerethDateTime.DateToString
    /// `DateToString()`: `"Wintersebb 1, 10 P.Y."`.
    #[must_use]
    pub fn date_to_string(&self) -> String {
        format!(
            "{} {}, {} P.Y.",
            month_name(self.month),
            self.day,
            self.year
        )
    }

    // ACE: DerethDateTime.PYToString
    /// `PYToString()`.
    #[must_use]
    pub fn py_to_string(&self) -> String {
        format!("{} P.Y.", self.year)
    }

    // ACE: DerethDateTime.PortalYearString
    /// `PortalYearString()`.
    #[must_use]
    pub fn portal_year_string(&self) -> String {
        format!("{} P.Y.", self.year)
    }

    // ACE: DerethDateTime.YearToString
    /// `YearToString()`.
    #[must_use]
    pub fn year_to_string(&self) -> String {
        format!("{} P.Y.", self.year)
    }

    // ---- real-world conversions ----

    // ACE: DerethDateTime.ConvertFrom_RealWorld_to_Derethian_PY
    fn convert_from_real_world_to_derethian_py(date_to_be_converted: DotNetDateTime) -> Self {
        let mut converted_year = 10;
        let converted_month = date_to_be_converted.month() - 3;
        let mut converted_day = date_to_be_converted.day();

        // ACE reads retailDayOne_RealWorld.Year (1999) here.
        let years_to_add = date_to_be_converted.year() - 1999;

        converted_year += years_to_add;

        if converted_day > 30 {
            converted_day = 30;
        }

        let h = date_to_be_converted.hour();
        let minute = date_to_be_converted.minute();
        // Each three-hour band maps to an hour and its half; the first 1h30 is the whole hour.
        let pick = |base: i32, whole: Hours, half: Hours| -> i32 {
            if h == base || (h == base + 1 && minute <= 29) {
                whole as i32
            } else {
                half as i32
            }
        };
        let converted_hour = if (0..=2).contains(&h) {
            pick(0, Hours::Darktide, Hours::Darktide_and_Half)
        } else if (3..=5).contains(&h) {
            pick(3, Hours::Foredawn, Hours::Foredawn_and_Half)
        } else if (6..=8).contains(&h) {
            pick(6, Hours::Dawnsong, Hours::Dawnsong_and_Half)
        } else if (9..=11).contains(&h) {
            pick(9, Hours::Morntide, Hours::Morntide_and_Half)
        } else if (12..=14).contains(&h) {
            pick(12, Hours::Midsong, Hours::Midsong_and_Half)
        } else if (15..=17).contains(&h) {
            pick(15, Hours::Warmtide, Hours::Warmtide_and_Half)
        } else if (18..=20).contains(&h) {
            pick(18, Hours::Evensong, Hours::Evensong_and_Half)
        } else if (21..=23).contains(&h) {
            pick(21, Hours::Gloaming, Hours::Gloaming_and_Half)
        } else {
            Hours::Darktide as i32
        };

        Self::from_ymdh(
            converted_year,
            converted_month,
            converted_day,
            converted_hour,
        )
    }

    // ACE: DerethDateTime.ConvertRealWorldToLoreDateTime
    /// `ConvertRealWorldToLoreDateTime(dateTime)`.
    ///
    /// # Panics
    /// For real years outside 1999..=2390 (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn convert_real_world_to_lore_date_time(date_time: DotNetDateTime) -> Self {
        Self::convert_from_real_world_to_derethian_py(date_time)
    }

    // ACE: DerethDateTime.UtcNowToLoreTime
    /// `UtcNowToLoreTime`.
    #[must_use]
    pub fn utc_now_to_lore_time(clock: &dyn Clock) -> Self {
        Self::convert_real_world_to_lore_date_time(clock.utc_now())
    }

    // ACE: DerethDateTime.UtcNowToGDLETime
    /// `UtcNowToGDLETime`: seconds since 1999-09-01 as ticks.
    ///
    /// # Panics
    /// When the result leaves the calendar (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn utc_now_to_gdle_time(clock: &dyn Clock) -> Self {
        Self::from_ticks((clock.utc_now() - DotNetDateTime::new(1999, 9, 1)).total_seconds())
    }

    // ACE: DerethDateTime.UtcNowToEMUTime
    /// `UtcNowToEMUTime`: seconds since the retail servers closed (2017-01-31 12:00 Eastern, with
    /// UTC shifted by -5 hours) as ticks. This seeds `Timers.PortalYearTicks`.
    ///
    /// # Panics
    /// Before 2017-01-31 17:00 UTC (`ArgumentOutOfRangeException`).
    #[must_use]
    pub fn utc_now_to_emu_time(clock: &dyn Clock) -> Self {
        // ACE: DerethDateTime.retailDayLast_RealWorld
        let retail_day_last_real_world = DotNetDateTime::new_hms(2017, 1, 31, 12, 0, 0);
        Self::from_ticks(
            (clock.utc_now().add_hours(-5.0) - retail_day_last_real_world).total_seconds(),
        )
    }
}

impl Default for DerethDateTime {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DerethDateTime {
    // ACE: DerethDateTime.ToString
    /// `"Date: Wintersebb 1, 10 P.Y.  Time: Morntide-and-Half"`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Date: {} {}, {} P.Y.  Time: {}",
            month_name(self.month),
            self.day,
            self.year,
            hour_name_dashed(self.hour)
        )
    }
}
