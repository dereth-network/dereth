//! Scheduled device events for an offscreen desktop client.

use std::collections::VecDeque;

use dereth_input::host::HostEvent;
use dereth_input::keys::{Key, MouseButton};

pub struct Replay {
    events: VecDeque<(u64, HostEvent)>,
}

impl Replay {
    pub fn load(argv: &mut Vec<String>) -> Result<Option<Self>, String> {
        let Some(path) = take_path(argv)? else {
            return Ok(None);
        };
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("--input-replay {path}: {e}"))?;
        Self::parse(&text).map(Some)
    }

    fn parse(text: &str) -> Result<Self, String> {
        let mut events = VecDeque::new();
        let mut previous = 1;
        for (line, text) in text.lines().enumerate() {
            let text = text.trim();
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            let parsed = (|| {
                let fields: Vec<_> = text.split_whitespace().collect();
                let frame = fields[0]
                    .parse::<u64>()
                    .map_err(|_| "expected a positive frame number")?;
                if frame == 0 || frame < previous {
                    return Err("frames must be positive and nondecreasing");
                }
                let event = parse_event(&fields[1..])?;
                Ok((frame, event))
            })();
            let (frame, event) =
                parsed.map_err(|e| format!("--input-replay line {}: {e}", line + 1))?;
            previous = frame;
            events.push_back((frame, event));
        }
        if events.is_empty() {
            return Err("--input-replay needs at least one event".into());
        }
        Ok(Self { events })
    }

    pub fn drain_frame(&mut self, frame: u64, mut emit: impl FnMut(HostEvent)) {
        while self.events.front().is_some_and(|(at, _)| *at == frame) {
            if let Some((_, event)) = self.events.pop_front() {
                emit(event);
            }
        }
    }
}

fn take_path(argv: &mut Vec<String>) -> Result<Option<String>, String> {
    let mut path = None;
    let mut retained = Vec::with_capacity(argv.len());
    let mut args = argv.iter();
    while let Some(arg) = args.next() {
        if arg != "--input-replay" {
            retained.push(arg.clone());
            continue;
        }
        if path.is_some() {
            return Err("--input-replay may be given only once".into());
        }
        let value = args
            .next()
            .filter(|s| !s.is_empty() && !s.starts_with("--"))
            .ok_or("--input-replay needs a file path")?;
        path = Some(value.clone());
    }
    if path.is_some() && !retained.iter().any(|a| a == "--headless") {
        return Err("--input-replay requires --headless".into());
    }
    *argv = retained;
    Ok(path)
}

fn pressed(value: &str) -> Result<bool, &'static str> {
    match value {
        "down" => Ok(true),
        "up" => Ok(false),
        _ => Err("expected down or up"),
    }
}

fn integer(value: &str) -> Result<u16, &'static str> {
    value
        .strip_prefix("0x")
        .map_or_else(|| value.parse(), |hex| u16::from_str_radix(hex, 16))
        .map_err(|_| "expected a decimal or 0x hexadecimal integer in 0..65535")
}

fn coordinate(value: &str) -> Result<f64, &'static str> {
    value
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or("expected a finite pointer coordinate")
}

fn parse_event(fields: &[&str]) -> Result<HostEvent, &'static str> {
    match fields {
        ["move", x, y] => Ok(HostEvent::CursorMoved {
            x: coordinate(x)?,
            y: coordinate(y)?,
        }),
        ["button", button, state] => Ok(HostEvent::MouseInput {
            button: match *button {
                "left" => MouseButton::Left,
                "right" => MouseButton::Right,
                "middle" => MouseButton::Middle,
                "back" => MouseButton::Back,
                "forward" => MouseButton::Forward,
                _ => return Err("unknown mouse button"),
            },
            pressed: pressed(state)?,
        }),
        ["wheel", notches] => Ok(HostEvent::MouseWheel {
            notches: notches
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or("expected finite wheel notches")?,
        }),
        ["alt", state] => Ok(HostEvent::ModifiersChanged {
            alt: pressed(state)?,
        }),
        ["key", vk, scan, state, text @ ..] => {
            let vk = integer(vk)?;
            if vk > 255 {
                return Err("virtual key must be in 0..255");
            }
            let pressed = pressed(state)?;
            if !pressed && !text.is_empty() {
                return Err("only a key down may carry translated text");
            }
            let translated = text
                .iter()
                .map(|point| {
                    point
                        .strip_prefix("U+")
                        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                        .and_then(char::from_u32)
                        .ok_or("expected a Unicode scalar as U+ hexadecimal digits")
                })
                .collect::<Result<String, _>>()?;
            Ok(HostEvent::KeyboardInput {
                key: Key::new(usize::from(vk), integer(scan)?),
                pressed,
                text: (!translated.is_empty()).then_some(translated),
            })
        }
        _ => Err("expected move, button, wheel, alt or key with its documented fields"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: shell.input-replay.validates-and-preserves-device-events
    #[test]
    fn replay_preserves_host_events_and_order_at_each_frame() {
        let mut replay = Replay::parse(
            "# device transitions\n1 move 12.5 -2\n1 button left down\n2 move 30 40\n\
             2 button left up\n3 key 0x41 0x1e down U+0061 U+0020 U+1F642\n\
             4 key 65 30 up\n4 alt down\n4 wheel -1.5\n5 alt up\n",
        )
        .unwrap();
        let mut events = Vec::new();
        replay.drain_frame(1, |event| events.push(event));
        assert_eq!(
            events,
            [
                HostEvent::CursorMoved { x: 12.5, y: -2.0 },
                HostEvent::MouseInput {
                    button: MouseButton::Left,
                    pressed: true
                },
            ]
        );
        replay.drain_frame(1, |_| panic!("events are delivered once"));
        for frame in 2..=5 {
            replay.drain_frame(frame, |event| events.push(event));
        }
        assert_eq!(events[2], HostEvent::CursorMoved { x: 30.0, y: 40.0 });
        assert_eq!(
            events[3],
            HostEvent::MouseInput {
                button: MouseButton::Left,
                pressed: false
            }
        );
        assert_eq!(
            events[4],
            HostEvent::KeyboardInput {
                key: Key::new(0x41, 0x1e),
                pressed: true,
                text: Some("a 🙂".into()),
            }
        );
        assert_eq!(
            events[5],
            HostEvent::KeyboardInput {
                key: Key::new(0x41, 0x1e),
                pressed: false,
                text: None,
            }
        );
        assert_eq!(events[6], HostEvent::ModifiersChanged { alt: true });
        assert_eq!(events[7], HostEvent::MouseWheel { notches: -1.5 });
        assert_eq!(events[8], HostEvent::ModifiersChanged { alt: false });
        assert!(replay.events.is_empty());
    }

    /// Behaviour: shell.input-replay.validates-and-preserves-device-events
    #[test]
    fn invalid_replay_is_rejected_with_a_line_number() {
        for invalid in [
            "0 move 1 2",
            "x move 1 2",
            "1",
            "1 move NaN 2",
            "1 move 1 inf",
            "1 move 1 2 extra",
            "1 button other down",
            "1 button left click",
            "1 wheel NaN",
            "1 wheel 1e100",
            "1 alt true",
            "1 unknown",
            "1 key 256 1 down",
            "1 key 1 65536 down",
            "1 key -1 1 down",
            "1 key 65 30 up U+0061",
            "1 key 65 30 down U+d800",
            "1 key 65 30 down U+110000",
            "1 key 65 30 down a",
        ] {
            let err = Replay::parse(&format!("# header\n{invalid}"))
                .err()
                .unwrap();
            assert!(err.contains("line 2:"), "{invalid}: {err}");
        }
        assert!(Replay::parse("2 move 1 2\n1 move 3 4").is_err());
        assert!(Replay::parse("# empty\n").is_err());
    }

    /// Behaviour: shell.input-replay.validates-and-preserves-device-events
    #[test]
    fn replay_argument_requires_headless_and_preserves_other_arguments() {
        let args = |values: &[&str]| values.iter().map(|v| (*v).to_owned()).collect::<Vec<_>>();
        let mut argv = args(&[
            "--headless",
            "--input-replay",
            "events.txt",
            "--frames",
            "10",
        ]);
        assert_eq!(take_path(&mut argv).unwrap().as_deref(), Some("events.txt"));
        assert_eq!(argv, args(&["--headless", "--frames", "10"]));
        assert_eq!(take_path(&mut argv).unwrap(), None);
        for bad in [
            vec!["--input-replay", "events.txt"],
            vec!["--headless", "--input-replay"],
            vec!["--headless", "--input-replay", "--frames", "10"],
            vec!["--headless", "--input-replay", "a", "--input-replay", "b"],
        ] {
            let mut argv = args(&bad);
            let original = argv.clone();
            assert!(take_path(&mut argv).is_err());
            assert_eq!(argv, original);
        }
    }
}
