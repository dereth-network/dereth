//! General mechanisms a world's rows can call on: an effect that holds back what others give a
//! quality, an event a quest's completion ends, lines said as an event starts, a step that answers
//! a use out of turn, and a quest whose time runs on the calendar.

mod calendar;
mod common;
mod event_lines;
mod held_back;
mod out_of_turn;
mod race_end;
