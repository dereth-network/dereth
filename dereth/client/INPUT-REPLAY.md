# Replaying input headlessly

The desktop client can play back a file of input events: `--headless --input-replay events.txt`.
It checks the whole file (UTF-8) before the client starts, then feeds each event through the same
input queue as the window. A replay needs `--headless` and does not end the session on its own:
stop it with `--frames` or by logging out.

Each nonempty line is a one-based frame number and an event. Frames must not go down; events on
the same frame keep file order. Lines starting with `#` are comments.

```text
# Move, press, drag, release, then type a translated character.
100 move 120 240
100 button left down
105 move 220 240
106 button left up
120 key 0x41 0x1e down U+0061
121 key 0x41 0x1e up
```

| Event | Meaning |
|---|---|
| `move X Y` | Pointer position in client pixels (finite decimals) |
| `button BUTTON down`, `button BUTTON up` | `left`, `right`, `middle`, `back` or `forward` |
| `wheel NOTCHES` | Signed wheel distance; one detent is one notch |
| `alt down`, `alt up` | The host's Alt modifier |
| `key VK SCAN down [U+HEX ...]` | A virtual key and scan code, with any translated Unicode text |
| `key VK SCAN up` | Releases the key, without text |
| `pad button NAME down`, `pad button NAME up` | A pad button: `a` (`south`), `b` (`east`), `x` (`west`), `y` (`north`), `lb`, `rb`, `back`, `start`, `up`, `down`, `left`, `right`, `ls`, `rs` |
| `pad stick left X Y`, `pad stick right X Y` | A stick held at X and Y, each from -1 to 1 |
| `pad trigger left AT`, `pad trigger right AT` | A trigger held at 0 to 1 |
| `pad off` | The pad unplugged |

A pad stays as the last `pad` lines left it until the next one changes it.

`VK` is a virtual-key number from 0 to 255 and `SCAN` a 16-bit set-1 scan code, extended prefix
included. Both take decimal or `0x` hex. Text is Unicode scalar values: `U+0020` is a space,
`U+000D` is Return and `U+0016` is the character Ctrl+V translates to. Replay never derives text
from a keyboard layout, so give modifier key presses too when a gesture needs them.

Events go into the queue before their frame begins. `--capture-at 106:drag.png` saves the picture
frame 106 drew, at the start of frame 107, so run at least 107 frames. Network and loading times
still vary, so use captures to check the right screen was up before the gesture. The file names no
controls or game requests: clicks and keys go wherever the active interface sends them.
