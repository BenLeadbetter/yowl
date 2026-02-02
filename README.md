# kitty-yowl

> **Work in Progress** — This project is under active development and not yet ready for use. Expect rough edges, missing features, and breaking changes. Contributions and feedback are welcome!

---

Voice dictation for [Kitty](https://sw.kovidgoyal.net/kitty/) terminal. Speak into your mic, and your words appear at the shell prompt.

## What is this?

kitty-yowl is a Kitty *kitten* that lets you dictate text directly into your terminal using your voice. It uses a persistent Rust daemon powered by [whisper-rs](https://github.com/tazz4843/whisper-rs) for fast, local speech-to-text — no cloud services, no latency penalties.

Press a keybinding, start talking, and watch your words stream into the prompt in real-time.

## How it works

```
┌─────────────────┐      Unix Socket      ┌──────────────────┐
│  Kitty Kitten   │◄────────────────────►│  Rust Daemon     │
│  (Python)       │                       │  (whisper-rs)    │
│                 │                       │                  │
│  • Keybindings  │                       │  • Audio capture │
│  • Text inject  │                       │  • Transcription │
└─────────────────┘                       └──────────────────┘
```

- **Python kitten**: Handles user interaction, manages the daemon lifecycle, and injects transcribed text into your active shell
- **Rust daemon**: Captures audio, runs Whisper inference, and streams transcript updates back to the kitten

## Features (planned)

- Low-latency streaming transcription
- Local processing (your voice stays on your machine)
- Metal acceleration on Apple Silicon
- Simple keybinding to start/stop dictation
- Text appears at your cursor, ready to edit before hitting Enter

## Requirements

- [Kitty](https://sw.kovidgoyal.net/kitty/) terminal emulator
- Rust toolchain (for building the daemon)
- A Whisper model (e.g., `base.en` or `small.en`)

## Status

Early days! The kitten scaffolding is in place, but the Rust daemon is still being built. Check back soon, or watch the repo for updates.

## License

* MIT
* Apache 2.0

---

*Why "yowl"? Because cats yowl, and this kitten helps you speak.*
