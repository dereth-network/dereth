# Auberean's icon

The same picture in the forms each platform reads.

| file | who reads it | wired |
|---|---|---|
| `auberean.ico` | Windows: 16 to 256 pixels, the frames up to 64 sharpened for their size | linked into `auberean.exe` by `build.rs`; the window takes it by number (`src/icon.rs`) |
| `auberean-client.rc` | the resource compiler | by `build.rs` |
| `auberean-256.png` | a window that takes its icon as pixels, or a `.desktop` entry's `Icon=` | the window, outside Windows and macOS (`src/icon.rs`) |
| `Auberean.icon/` | macOS, compiled into an application bundle | not yet: there is no bundle |
