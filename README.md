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
gst-launch-1.0 -e videotestsrc is-live=true ! x264enc tune=zerolatency ! flvmux name=fm ! captionsflvmux name=cm ! filesink location=captions.flv audiotestsrc is-live=true ! voaacenc ! fm. filesrc location=speech.wav ! decodebin ! audioconvert ! audioresample ! captionstranscriber model-path=model.gguf ! captionsrollup ! cm.text
```

Swap `filesrc` for any live source to caption live; the transcriber takes
its live path automatically when upstream answers the latency query as live.

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

This uses GST_PLUGIN_PATH, so no install or root access is needed.

```sh
export GST_PLUGIN_PATH="$PWD/target/release"
gst-inspect-1.0 captionstranscriber captionsrollup captionsflvmux
```

For a permanent install, prefer the user-local plugin directory (no root
needed) over a system-wide install:

```sh
# Just for your user (no root needed):
install -m 0755 target/release/libgstcaptions.so ~/.local/share/gstreamer-1.0/plugins/
# Or system-wide (needs root):
sudo install -m 0755 target/release/libgstcaptions.so "$(pkg-config --variable=pluginsdir gstreamer-1.0)/"
```

(On macOS the file is `libgstcaptions.dylib`.)

On macOS with the GStreamer framework build, point pkg-config at it first:

```sh
export PKG_CONFIG_PATH=/Library/Frameworks/GStreamer.framework/Versions/Current/lib/pkgconfig:$PKG_CONFIG_PATH
export DYLD_FALLBACK_LIBRARY_PATH=/Library/Frameworks/GStreamer.framework/Versions/Current/lib:$DYLD_FALLBACK_LIBRARY_PATH
export PATH=/Library/Frameworks/GStreamer.framework/Versions/Current/bin:$PATH
```

GPU backends are opt-in cargo features forwarded to `transcribe-cpp`
(`metal`, `cuda`, `vulkan`, `openmp`, `dynamic-backends`); the default
build is CPU-only so it compiles everywhere without GPU toolchains:

```sh
cargo build --release --features metal
cargo build --release --no-default-features --features flvmux,rollup
```

The second line builds just the caption formatting and muxing, with no C++
toolchain, cmake, or model backend involved.

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

## Migrating from the old plugins

Version 0.1.0 merges the former `gst-flvsubmux`, `gst-textrollup`, and
`gst-transcribe-cpp` crates into this one plugin. The old element names are
gone; rename them:

| Before | After |
| --- | --- |
| `flvsubmux` | `captionsflvmux` |
| `textrollup` | `captionsrollup` |
| `transcribecpptranscriber` | `captionstranscriber` |

Debug categories moved with them (`captionsflvmux`, `captionsrollup`,
`captionstranscriber`, plus `captionslib` for transcribe.cpp's own log
output). Properties are otherwise unchanged.

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
