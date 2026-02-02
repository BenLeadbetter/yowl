# kitty-yowl — Technical Plan

## Project Summary

**Goal:**  
Implement a Kitty *kitten* extension that enables low-latency voice dictation into the active shell prompt using a persistent Rust background service powered by `whisper-rs` (whisper.cpp bindings).

The system consists of:

- A **Kitty kitten (Python)** responsible for:
  - Lifecycle management of the Rust daemon
  - User interaction (keybindings)
  - IPC communication
  - Injecting text into the active Kitty window

- A **Rust speech daemon** responsible for:
  - Capturing microphone input
  - Running streaming Whisper inference
  - Sending incremental transcription updates over a Unix domain socket

The architecture avoids macOS-specific APIs and async runtimes (Tokio), instead using explicit threads and channels for clarity and determinism.

---

# High-Level Architecture

```
User presses keybinding in Kitty
        │
        ▼
Kitten (Python)
  - Sends START/STOP to daemon
  - Receives transcript deltas
  - Injects text via `kitty @ send-text`
        │
        ▼
Rust Speech Daemon
  Thread 1: Audio capture (cpal callback)
  Thread 2: Inference worker (whisper-rs)
  Thread 3: IPC server (Unix socket)
```


Communication between components:

- Rust ↔ Kitten: Unix domain socket  
- Threads inside Rust: crossbeam channels  

---

# Component Responsibilities

## 1. Kitty Kitten (Python)

### Responsibilities

- Spawn Rust daemon if not running
- Establish Unix domain socket connection
- Bind to a key (e.g. Ctrl+Shift+D)
- Send control messages:
  - `START`
  - `STOP`
  - `SHUTDOWN`
- Receive transcript updates
- Inject incremental text into active shell prompt

### Text Injection Strategy

- Maintain last injected transcript
- On update:
  - Compute delta
  - Replace current shell line (safe approach)
- Never auto-submit (no implicit Enter)

Uses:

```
kitty @ send-text --match id:$KITTY_WINDOW_ID ...
```


---

## 2. Rust Speech Daemon

### Core Design Principles

- Persistent process (loads model once)
- Manual thread model (no Tokio)
- Clear separation of concerns
- Bounded channels between stages
- Deterministic shutdown

---

## Thread Model

### Thread A — Audio Capture

- Uses `cpal`
- Captures mic input
- Resamples to 16kHz mono (if needed)
- Pushes audio frames into bounded channel
- Never blocks

### Thread B — Inference Worker

- Maintains rolling audio buffer (e.g. 5–10 seconds)
- Periodically runs Whisper inference
- Computes transcript delta
- Sends incremental transcript updates to IPC thread
- Runs only when in `RECORDING` state

### Thread C — IPC Server

- Unix domain socket listener
- Handles a single client (kitten)
- Accepts commands:
  - `START`
  - `STOP`
  - `PING`
  - `SHUTDOWN`
- Sends:
  - `PARTIAL <text>`
  - `FINAL <text>`
  - `ERROR <msg>`

Protocol is simple line-delimited UTF-8 text.

---

# State Machine (Rust Side)

States:

* Idle
* Recording
* ShuttingDown


Transitions:

- `START` → Recording  
- `STOP` → Idle  
- Parent process exit → ShuttingDown  

---

# Inference Strategy

- Use `whisper-rs` with whisper.cpp backend
- Prefer `base.en` or `small.en`
- Enable Metal acceleration on macOS
- Chunk inference at ~300–500ms intervals
- Maintain previous transcript for diffing

Delta strategy:

- Compare previous transcript with new one
- Send only appended text
- Avoid rewriting entire buffer unless necessary

---

# Lifecycle Management

## Startup

- Kitten checks if daemon is running (PID file or socket probe)
- If not, spawn daemon
- Wait until socket is ready
- Connect

## Shutdown

- If Kitty exits:
  - Daemon detects parent PID disappearance
  - Graceful shutdown
- Or kitten sends explicit `SHUTDOWN`

---

# Safety Considerations

- No automatic execution of commands
- No newline injection
- Transcript always editable before submission
- Optional: configurable “overlay mode” instead of direct injection

---

# Performance Expectations

On Apple Silicon:

- `base.en` runs near real-time
- ~200–400ms streaming latency
- CPU moderate but acceptable
- Model loaded once at startup

---

# Why Manual Threads (Not Tokio)

- Workload is CPU-bound + audio callback driven
- No high concurrency or many clients
- Simpler mental model
- No async/await glue
- Avoid `spawn_blocking` complexity
- Clean separation via channels

---

# Deliverables

1. `kitty_voice.py` (Kitten)
2. `voice_daemon` (Rust binary)
3. Simple text-based IPC protocol
4. Model configuration options
5. Kitty config keybinding example

---

# Final Project Definition (One Sentence)

A persistent Rust-based speech-to-text daemon using whisper-rs, controlled by a Kitty kitten, enabling low-latency streaming dictation directly into the active shell prompt via Unix socket IPC and explicit thread-based concurrency.
