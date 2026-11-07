# mnRekord

A small Windows screen recorder written in Rust with [egui](https://github.com/emilk/egui).

## Features

- Record a screen region, a whole monitor or a window, chosen on a stage or drawn directly on the desktop
- Aspect ratio lock (16:9, 9:16 or free) with size presets (360p, HD, FHD, QHD, 4K UHD)
- System audio (loopback) with a level meter
- H.264 + AAC in MP4 (Media Foundation), standard or high detail encoder, 30 or 60 fps
- Pause, resume, markers, countdown, global shortcuts (F9 record, F10 stop, Ctrl+M marker)
- Dark UI

## Build

Requires Windows 10 or newer and a current stable Rust toolchain.

```
cargo build --release
```

The executable is `target/release/mnrekord.exe`.

## Command line

A running instance accepts commands from a second launch, for example
`mnrekord record`, `mnrekord stop`, `mnrekord setregion X Y W H`,
`mnrekord setoutputpath "D:\Videos"`, `mnrekord setscreenframerate 60`.
See `src/ipc.rs` for the full list.

## Settings

Stored in `%APPDATA%\mnRekord\settings.json`. Recordings go to `Documents\mnRekord` by default.

## License

MIT, see `LICENSE`.
