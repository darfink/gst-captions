<div align="center">

# `gst-captions`

## Live-caption GStreamer elements: transcribe, roll up, mux

[![CI][ci-shield]][ci]
[![Version][version-shield]][releases]
[![GStreamer][gstreamer-shield]][gstreamer]
[![License][license-shield]][license]

</div>

One plugin for the whole live-caption chain: speech-to-text, roll-up
formatting, and FLV caption muxing. All three elements ship in the single
`captions` plugin and are all enabled by default.

| Element | Direction | Purpose |
| --- | --- | --- |
| `captionstranscriber` | filter (audio to text) | Transcribe speech with transcribe.cpp, one committed word per buffer |
| `captionsrollup` | filter (text to text) | Format word buffers into a fixed roll-up caption window |
| `captionsflvmux` | muxer (FLV plus text to FLV) | Insert the captions into a muxed FLV stream as `onCaption` / `onTextData` script data |

The intended wiring is transcriber into roll-up into muxer:

```text
captionstranscriber ! captionsrollup ! captionsflvmux
```

with the roll-up feeding the muxer's `text` pad in `input-mode=replacement`.
See [gst-captions/README.md](gst-captions/README.md) for the full property and
behaviour reference.

## Examples

1. Transcribe a file to the console (any GGUF family transcribe.cpp supports):

```sh
gst-launch-1.0 -e filesrc location=speech.wav ! decodebin ! audioconvert ! audioresample ! captionstranscriber model-path=model.gguf ! fakesink dump=true
```

2. Add the roll-up window:

```sh
gst-launch-1.0 -e filesrc location=speech.wav ! decodebin ! audioconvert ! audioresample ! captionstranscriber model-path=model.gguf ! captionsrollup ! fakesink dump=true
```

3. Mux the captions into an FLV file alongside audio and video:

```sh
gst-launch-1.0 -e videotestsrc is-live=true ! x264enc tune=zerolatency ! flvmux name=fm streamable=true ! captionsflvmux name=cm ! filesink location=captions.flv audiotestsrc is-live=true ! voaacenc ! fm. filesrc location=speech.wav ! decodebin ! audioconvert ! audioresample ! captionstranscriber model-path=model.gguf ! captionsrollup ! cm.text
```

`streamable=true` matters: file-mode `flvmux` strips buffer timestamps, leaving
no media position to attach cues to.

Swap `filesrc` for any live source to caption live; the transcriber takes
its live path automatically when upstream answers the latency query as live.

## Mixing with the ecosystem

The trio above is the production wiring, but each element also stands alone
against stock elements:

- `captionstranscriber ! textwrap` — format words into cues with the stock
  text formatter (verified with a real model).
- Any word-buffer transcriber (whisper-family, stock
  `transcribecpptranscriber`) into `captionsrollup` — the roll-up only needs
  `text/x-raw` word buffers with PTS and duration; leading spaces are fine
  (verified with the stock element and a real model).
- Sentence or accumulated buffers into `captionsflvmux` in `input-mode=timed`
  — longer cues arrive as intervals the muxer shows and clears.
- Cue grouping: the transcriber emits the `rstranscribe/final-transcript`
  event at utterance boundaries, the convention stock cue-grouping elements
  drain on.

The same `flvmux` gotcha applies whoever feeds the muxer: toward a file you
need `streamable=true`, otherwise you get a valid FLV with zero caption tags
(toward RTMP or another non-seekable sink it already streams). `captionsflvmux`
warns once when it sees untimestamped media.

Model-backed interop tests live in `gst-captions/tests/interop.rs`; run them
with a model and a WAV file:

```sh
GST_CAPTIONS_MODEL=model.gguf GST_CAPTIONS_WAV=speech.wav cargo test --release --test interop
```

## Build

Requires Rust 1.87+ and GStreamer 1.20 development headers. The transcriber
also needs a C++ toolchain and **cmake** (`transcribe-cpp-sys` vendors
transcribe.cpp and builds it from source; on macOS, `brew install cmake`).
The first build compiles ggml and takes a while.

```sh
git clone https://github.com/darfink/gst-captions.git
cd gst-captions
cargo build --release
```

Prefer not to build? Each [release](https://github.com/darfink/gst-captions/releases)
ships prebuilt libraries for Linux (CPU-only) and macOS (Metal); use one
wherever steps 2–3 reference the file you built.

1. Try it straight from the build tree:

```sh
export GST_PLUGIN_PATH="$PWD/target/release"
gst-inspect-1.0 captionstranscriber captionsrollup captionsflvmux
```

2. Install for your user:

```sh
mkdir -p ~/.local/share/gstreamer-1.0/plugins
install -m 0755 target/release/libgstcaptions.so ~/.local/share/gstreamer-1.0/plugins/
```

(On macOS the file is `libgstcaptions.dylib`.)

3. Or system-wide (needs root):

```sh
sudo install -m 0755 target/release/libgstcaptions.so "$(pkg-config --variable=pluginsdir gstreamer-1.0)/"
```

If you installed GStreamer from its official macOS framework installer (not Homebrew), export this first so `pkg-config` finds it:

```sh
export PKG_CONFIG_PATH=/Library/Frameworks/GStreamer.framework/Versions/Current/lib/pkgconfig:$PKG_CONFIG_PATH
```

GPU backends are opt-in cargo features forwarded to `transcribe-cpp`; the
default build is CPU-only so it compiles everywhere without GPU toolchains.
The element's `backend` property defaults to `auto`, which picks the best
compiled-in backend at runtime and falls back to CPU, so rebuilding with a
GPU feature is enough to use it — no property change needed:

```sh
cargo build --release --features metal # macOS
cargo build --release --features cuda # NVIDIA Linux
```

```sh
cargo build --release --no-default-features --features flvmux,rollup
```

This builds just caption formatting and muxing, with no C++ toolchain,
cmake, or model backend involved.

## Testing

```sh
cargo test --workspace
```

Unit tests cover the commit tracker's prefix handling, token-to-word joining,
the sliding-window logic, the pure roll-up window, and a fixture oracle for
the roll-up geometry. They need no model. Element tests drive all three
elements through `gst-check` harnesses.

## Install

With [`cargo-c`](https://github.com/lu-zero/cargo-c):

```sh
cargo install cargo-c
GSTREAMER_LIBDIR="$(pkg-config --variable=libdir gstreamer-1.0)"
cargo cinstall --release --library-type cdylib -p gst-captions --prefix="$(pkg-config --variable=prefix gstreamer-1.0)" --libdir="$GSTREAMER_LIBDIR"
```

Or run the Docker image, which builds the plugin and inspects all three
elements:

```sh
docker build -t gst-captions .
docker run --rm gst-captions
```

## License

MPL-2.0. See `gst-captions/LICENSE`.

[ci-shield]: https://img.shields.io/github/actions/workflow/status/darfink/gst-captions/ci.yml?branch=main&label=CI&logo=github&style=for-the-badge
[ci]: https://github.com/darfink/gst-captions/actions/workflows/ci.yml?query=branch%3Amain
[version-shield]: https://img.shields.io/github/v/tag/darfink/gst-captions?style=for-the-badge&label=version
[releases]: https://github.com/darfink/gst-captions/releases
[gstreamer-shield]: https://img.shields.io/badge/GStreamer-1.20+-orange?style=for-the-badge
[gstreamer]: https://gstreamer.freedesktop.org/
[license-shield]: https://img.shields.io/badge/license-MPL--2.0-green?style=for-the-badge
[license]: https://github.com/darfink/gst-captions
