# mnRekord

A small Windows screen recorder written in Rust with [egui](https://github.com/emilk/egui).

## Features

- Record a screen region, a whole monitor or a window, chosen on a stage or drawn directly on the desktop
- Follow a window: the frame moves with the window it was picked from while recording
- Aspect ratio lock (16:9, 9:16 or free) with size presets (360p, HD, FHD, QHD, 4K UHD)
- System audio (loopback) with a level meter
- H.264 + AAC in MP4 (Media Foundation) with a quality setting, hardware encoding when available, 15, 24, 30 or 60 fps
- Pointer highlight and click ring
- Long recordings can be split into parts (a new file every N minutes)
- Animated GIF of the last recording (up to 30 s, 640 px wide)
- File name template (`{date}`, `{time}`, `{window}`)
- Pause, resume, markers, countdown, global shortcuts (F9 record, F10 stop, Ctrl+M marker)
- Tray icon with show / start / stop / quit, and the recording state in the window title
- A recording cut off by a crash can be recovered at the next start
- Dark UI

## Build

Requires Windows 10 or newer and a current stable Rust toolchain.

```
cargo build --release
```

The executable is `target/release/mnrekord.exe`.

## Installer

`installeruild.ps1` builds the release exe and an installer, `dist\mnRekord-<version>-setup.exe`
(needs [Inno Setup 6](https://jrsoftware.org/isinfo.php)). It installs per user by default, adds a
Start menu entry (and optionally a desktop icon) and an uninstaller. English and Hungarian.

## Command line

A running instance accepts commands from a second launch, for example
`mnrekord record`, `mnrekord stop`, `mnrekord setregion X Y W H`,
`mnrekord setoutputpath "D:\Videos"`, `mnrekord setscreenframerate 60`, `mnrekord followwindow "Notepad"`.
See `src/ipc.rs` for the full list.

`mnrekord --gif in.mp4 out.gif` converts a recording to a GIF and exits (the result is written to `out.gif.log`).

## Settings

Stored in `%APPDATA%\mnRekord\settings.json`. Recordings go to `Documents\mnRekord` by default.

## License

MIT, see `LICENSE`.
