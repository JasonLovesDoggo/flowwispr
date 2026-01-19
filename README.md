# Flow

Flow is a voice dictation app that captures audio, transcribes speech, and formats it into clean text. It supports shortcuts, learning, and tone that adapts per app. 


## Product

- Dictation with app-aware formatting
- Voice shortcuts and learned corrections
- Usage stats and configurable providers

## Installation

### Download from Releases (Recommended)

1. Download the latest macOS build: https://github.com/JasonLovesDoggo/Flow/releases/latest/download/Flow-macOS-universal.dmg
2. Open the downloaded .dmg file and drag Flow to Applications
3. Flow will be installed to your Applications folder

The universal binary works on both Apple Silicon (M1/M2/M3) and Intel Macs running macOS 14+.

To verify the download integrity, you can check the SHA256 checksum:
```sh
shasum -a 256 Flow-macOS-universal.dmg
# Compare with the .sha256 file from the release
```

### Build from Source

See the [Setup](#setup) section below for instructions on building from source.

## Tech stack

- Rust core engine in `flow-core/`
- FFI bridge for native app integration (C ABI in `flow-core/src/ffi.rs`)
- Provider abstraction for transcription and completion
- SQLite-backed storage for user data and stats

The rest of the app lives here in the repo root.

## Setup

```sh
git clone https://github.com/JasonLovesDoggo/flow.git
cd flow
cd flow-core
cargo build
cd ..
swift run
```
