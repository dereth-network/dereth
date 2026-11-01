//! A time left written into a text element, kept current by the clock: in months and days while
//! more months are left than the display's least, else in days and hours, else hours and minutes,
//! else minutes and seconds; written again only when the words change, and looked at again when
//! the smallest unit shown runs out. Panels that show how long something lasts (the examination's
//! and the journal's) hang one on a text element.

use auberean_ui::manager::global;
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::flow;

/// Seconds in a month (thirty-one days), a day, an hour, a minute.
const MONTH: i32 = 2_678_400;
const DAY: i32 = 86_400;
const HOUR: i32 = 3_600;
const MINUTE: i32 = 60;

/// A time left shown in a text element.
#[derive(Debug)]
pub struct TimeLeft {
    listener: auberean_ui::ListenerId,
    text: Option<Handle>,
    /// The words last written.
    old: Option<String>,
    /// When the time runs out, and when to look again.
    end: f64,
    next: f64,
    /// Above how many months, days and minutes each unit pair is used.
    month_min: u32,
    day_min: u32,
    minute_min: u32,
    listening: bool,
}

impl TimeLeft {
    /// A display writing nowhere yet.
    // D2-03194
    pub fn new(ui: &mut Ui) -> Self {
        Self {
            listener: ui.new_listener(),
            text: None,
            old: None,
            end: 0.0,
            next: 0.0,
            month_min: 0,
            day_min: 0,
            minute_min: 0,
            listening: false,
        }
    }

    /// Show the time left until `end` in `text`, with the least months, days and minutes for each
    /// unit pair; the clock heard from now on.
    // D2-03198
    pub fn init(
        &mut self,
        ui: &mut Ui,
        text: Option<Handle>,
        end: f64,
        month_min: u32,
        day_min: u32,
        minute_min: u32,
    ) {
        self.old = None;
        self.text = text;
        self.end = end;
        self.month_min = month_min;
        self.day_min = day_min;
        self.minute_min = minute_min;
        if !self.listening {
            ui.listen_for_global_message(self.listener, global::TIME);
            self.listening = true;
        }
        let now = ui.now();
        self.compute_time_left(ui, now);
    }

    /// The time left written (when the words change), and when to look again: once the smallest
    /// unit shown has run out. Once the time is out, the clock is no longer heard.
    // D2-03196
    pub fn compute_time_left(&mut self, ui: &mut Ui, now: f64) {
        let t = dereth_primitives::num::to_i32_f64(self.end - now);
        let months = t / MONTH;
        let days = t / DAY;
        let minutes = t / MINUTE;
        let seconds = t % MINUTE;
        let hours_of_day = (t % DAY) / HOUR;
        let hours = t / HOUR;
        let minutes_of_hour = (t % HOUR) / MINUTE;
        let unsigned = |v: i32| u32::from_ne_bytes(v.to_ne_bytes());
        let (string, vars, wait): (&str, Vec<(Token, auberean_text::Value)>, i32) =
            if unsigned(months) > self.month_min {
                (
                    "ID_Time_Months",
                    vec![
                        (
                            Token::of("months"),
                            auberean_text::Value::UInt(unsigned(months)),
                        ),
                        (
                            Token::of("days"),
                            auberean_text::Value::UInt(unsigned((t % MONTH) / DAY)),
                        ),
                    ],
                    (hours_of_day * 60 + minutes_of_hour) * 60 + seconds,
                )
            } else if unsigned(days) > self.day_min {
                (
                    "ID_Time_Days",
                    vec![
                        (
                            Token::of("days"),
                            auberean_text::Value::UInt(unsigned(days)),
                        ),
                        (
                            Token::of("hours"),
                            auberean_text::Value::UInt(unsigned(hours_of_day)),
                        ),
                    ],
                    minutes_of_hour * 60 + seconds,
                )
            } else if unsigned(minutes) > self.minute_min {
                (
                    "ID_Time_Hours",
                    vec![
                        (Token::of("hours"), auberean_text::Value::Int(hours)),
                        (Token::of("minutes"), two_digits(minutes_of_hour)),
                    ],
                    seconds,
                )
            } else {
                (
                    "ID_Time_Minutes",
                    vec![
                        (Token::of("minutes"), auberean_text::Value::Int(minutes)),
                        (Token::of("seconds"), two_digits(seconds)),
                    ],
                    1,
                )
            };
        let table = ui
            .assets()
            .data_id_by_enum(flow::MESSAGES, flow::TABLE_MAPPING);
        let mut info = auberean_text::StringInfo::entry(table, Token::of(string).raw());
        for (id, value) in vars {
            info.variables.push(auberean_text::Variable {
                id: id.raw(),
                value,
            });
        }
        let words = ui.assets().write(&info).unwrap_or_default();
        if self.old.as_deref() != Some(words.as_str()) {
            if let Some(h) = self.text {
                ui.set_text(h, &words);
            }
            self.old = Some(words);
        }
        self.next = f64::from(unsigned(wait)) + now;
        if t <= 0 {
            ui.unregister_for_global_message(self.listener, global::TIME);
            self.listening = false;
        }
    }

    /// The clock: the time left written again once it is time to look.
    // D2-03197
    pub fn tick(&mut self, ui: &mut Ui) {
        for m in ui.take_mail(self.listener) {
            if let Mail::Global {
                what: global::TIME, ..
            } = m
            {
                let now = ui.now();
                if self.next <= now {
                    self.compute_time_left(ui, now);
                }
            }
        }
    }

    /// The words last written.
    #[must_use]
    pub fn shown(&self) -> Option<&str> {
        self.old.as_deref()
    }

    /// The display goes: the clock no longer heard.
    // D2-03195
    pub fn close(self, ui: &mut Ui) {
        ui.drop_listener(self.listener);
    }
}

/// A number written with at least two digits.
fn two_digits(v: i32) -> auberean_text::Value {
    let s = format!("{v:02}");
    auberean_text::Value::Text(Box::new(auberean_text::StringInfo::literal(&s)))
}
