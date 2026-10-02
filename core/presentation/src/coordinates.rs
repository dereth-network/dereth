//! The player's coordinates as text: the magnitude to one decimal and the hemisphere letter,
//! north/south first.

/// The three coordinate strings the radar writes.
///
/// **The field names read as crossed.** The client writes the first returned coordinate into its
/// Y-coordinate text and the second into its X-coordinate text. A live retail run shows
/// `42.2N, 33.8E`, so this reproduces the
/// **output**, not the field names: `combined` is "N first, E second", `y_field` carries the first
/// (N/S) coordinate and `x_field` the second (E/W).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coordinates {
    /// The combined coordinate text — `"42.2N, 33.8E"`.
    pub combined: String,
    /// The Y-coordinate text — receives the first coordinate.
    pub y_field: String,
    /// The X-coordinate text — receives the second.
    pub x_field: String,
}

/// Format one AC coordinate: the magnitude to one decimal, then the hemisphere letter.
///
/// The original formatter receives each absolute magnitude with one fixed decimal place; the
/// formatting string supplies the N/S or E/W letter, so only the magnitude is numeric input.
#[must_use]
pub fn format_coordinate(v: f32, positive: char, negative: char) -> String {
    let letter = if v < 0.0 { negative } else { positive };
    format!("{:.1}{letter}", v.abs())
}

/// The coordinate update. `coords` is the player-coordinates query's `(x, y)`; the
/// first is the north/south value and the second the east/west one, which is what makes the output
/// read `42.2N, 33.8E`.
#[must_use]
pub fn update_coordinates(coords: (f32, f32)) -> Coordinates {
    let ns = format_coordinate(coords.0, 'N', 'S');
    let ew = format_coordinate(coords.1, 'E', 'W');
    Coordinates {
        combined: format!("{ns}, {ew}"),
        y_field: ns,
        x_field: ew,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the live-run evidence — a live retail session shows `42.2N, 33.8E`.
    /// The *output* is what is matched, not the field names.
    #[test]
    fn the_coordinates_format_as_the_live_run_shows_them() {
        let c = update_coordinates((42.2, 33.8));
        assert_eq!(c.combined, "42.2N, 33.8E");
        assert_eq!(
            c.y_field, "42.2N",
            "the first coordinate goes to the Y-coordinate text"
        );
        assert_eq!(c.x_field, "33.8E");
        // Negative magnitudes flip the letter, and only the magnitude is printed.
        assert_eq!(
            update_coordinates((-3.15_f32, -12.0)).combined,
            "3.2S, 12.0W"
        );
    }
}
