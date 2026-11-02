//! The plain data and arithmetic held to the September 2013 client's own answers: colors and the
//! float comparisons, new frames, whole numbers of any size, weighted selection tables, the string
//! library's rules for narrow text and the clock. Fixture: tables of cases, each the inputs given
//! and the words the client's own code answered; the strings and the clock by cases read from its
//! machine code.

mod clock;
mod color;
mod color_cases;
mod frames;
mod oracle;
mod selection;
mod selection_cases;
mod strings;
mod vlong;
mod vlong_cases;
