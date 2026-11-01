# Athenaeum's icon

The same picture in the forms each platform reads. The editor links them; `build.rs` and
`src/icon.rs` say how.

| file | who reads it |
|---|---|
| `athenaeum.ico` | Windows, out of the executable: 16 to 256 pixels, the frames up to 64 sharpened for their size |
| `athenaeum-editor.rc` | the resource compiler, which links `athenaeum.ico` as the executable's icon (`build.rs`) |
| `athenaeum-256.png` | a window that takes its icon as pixels (`src/icon.rs`), or a `.desktop` entry's `Icon=` |
| `Athenaeum.icon/` | macOS, compiled into an application bundle |
