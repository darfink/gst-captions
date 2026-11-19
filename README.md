<div align="center">

# `gst-captions`

## Live-caption GStreamer elements: transcribe, roll up, mux

[![CI][ci-shield]][ci]
[![Version][version-shield]][releases]
[![GStreamer][gstreamer-shield]][gstreamer]
[![License][license-shield]][license]

</div>

One GStreamer plugin (`captions`) covering the full live-caption pipeline: speech-to-text, roll-up formatting, and FLV caption muxing. All three elements are enabled by default, or you can pick only what you need with Cargo features.

| Element | Pads | Purpose |
| --- | --- | --- |
| `captionstranscriber` | audio in, text out | Transcribe speech via `transcribe.cpp`, emitting committed UTF-8 words or segments |
| `captionsrollup` | text filter | Format word buffers into a fixed roll-up caption window |
| `captionsflvmux` | FLV + text in, FLV out | Inject captions into a muxed FLV stream as `onCaption` or `onTextData` script tags |

The standard live chain wires the transcriber into the roll-up formatter and feeds the muxer's `text` pad in `replacement` mode:

```text
captionstranscriber ! captionsrollup ! captionsflvmux input-mode=replacement
```

## Quick examples

1. **Transcribe a file to the console** (any GGUF model family `transcribe.cpp` supports):

```sh
gst-launch-1.0 -e filesrc location=speech.wav ! decodebin ! audioconvert ! audioresample \
  ! captionstranscriber model-path=model.gguf ! fakesink dump=true
```

2. **Add a roll-up window:**

```sh
gst-launch-1.0 -e filesrc location=speech.wav ! decodebin ! audioconvert ! audioresample \
  ! captionstranscriber model-path=model.gguf ! captionsrollup ! fakesink dump=true
```

3. **Mux captions into FLV alongside audio and video:**

```sh
gst-launch-1.0 -e \
  videotestsrc is-live=true ! x264enc tune=zerolatency ! flvmux name=fm streamable=true \
  ! captionsflvmux name=cm input-mode=replacement ! filesink location=captions.flv \
  audiotestsrc is-live=true ! voaacenc ! fm. \
  filesrc location=speech.wav ! decodebin ! audioconvert ! audioresample \
  ! captionstranscriber model-path=model.gguf ! captionsrollup ! cm.text
```

> [!IMPORTANT]
> When writing FLV to a file, set `streamable=true` on `flvmux` (RTMP and other non-seekable sinks stream automatically). File-mode `flvmux` strips buffer timestamps, leaving `captionsflvmux` with no timeline to attach cues to—it logs a warning once and outputs zero caption tags.

Replace `filesrc` with any live audio source to caption live; `captionstranscriber` switches to its live path automatically when upstream answers the GStreamer latency query as live.

### Mixing with stock GStreamer elements

Each element also works standalone with stock plugins:

- **`captionstranscriber ! textwrap` or `textaccumulate`:** `captionstranscriber` emits `rstranscribe/final-transcript` at utterance boundaries (matching `gst-plugins-rs` transcribers), so stock cue-grouping elements drain automatically.
- **Any word-level transcriber into `captionsrollup`:** Pass `text/x-raw,format=utf8` word buffers with PTS and non-zero duration (leading spaces are handled cleanly).
- **Pre-timed subtitles into `captionsflvmux`:** Use the default `input-mode=timed` to inject sentence or cue intervals (for example from `subparse`) that appear at PTS and clear at `PTS + duration`.

## Build

Requires Rust 1.92+ and GStreamer 1.20+ development headers. Building `captionstranscriber` also requires a C++ toolchain and **cmake** (`transcribe-cpp-sys` compiles `transcribe.cpp` and `ggml` from source; on macOS, `brew install cmake`).

If you installed GStreamer from the official macOS framework installer rather than Homebrew, expose its `pkg-config` files first:

```sh
export PKG_CONFIG_PATH="/Library/Frameworks/GStreamer.framework/Versions/Current/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
```

Clone and build:

```sh
git clone https://github.com/darfink/gst-captions.git
cd gst-captions
cargo build --release --locked
```

Prefer prebuilt binaries? Each [GitHub release](https://github.com/darfink/gst-captions/releases) includes libraries for Linux (CPU-only `.so`) and macOS (Metal `.dylib`).

### Cargo features and GPU backends

The default build enables all three elements on CPU so it compiles everywhere without GPU SDKs. GPU backends are opt-in features forwarded to `transcribe-cpp`; because `captionstranscriber`'s `backend` property defaults to `auto`, rebuilding with a GPU feature enables it automatically:

```sh
cargo build --release --locked --features metal  # macOS
cargo build --release --locked --features cuda   # NVIDIA Linux
```

To build only formatting and FLV muxing—with no C++ toolchain, `cmake`, or ML backend:

```sh
cargo build --release --locked --no-default-features --features flvmux,rollup
```

| Feature | Default | Effect |
| --- | --- | --- |
| `flvmux` | on | Build `captionsflvmux` |
| `rollup` | on | Build `captionsrollup` |
| `transcribe` | on | Build `captionstranscriber` (CPU; needs C++ toolchain + `cmake`) |
| `metal` | off | macOS Metal GPU backend (forwarded to `transcribe-cpp`) |
| `cuda` | off | NVIDIA CUDA backend (forwarded to `transcribe-cpp`) |
| `vulkan` | off | Cross-vendor Vulkan backend (forwarded to `transcribe-cpp`) |
| `openmp` | off | OpenMP CPU threading backend (forwarded to `transcribe-cpp`) |
| `dynamic-backends` | off | Runtime backend loading (forwarded to `transcribe-cpp`) |

### Loading and installing the plugin

Try it straight from the build directory:

```sh
export GST_PLUGIN_PATH="$PWD/target/release"
gst-inspect-1.0 captionstranscriber captionsrollup captionsflvmux
```

Install for your user (`libgstcaptions.dylib` on macOS):

```sh
mkdir -p ~/.local/share/gstreamer-1.0/plugins
install -m 0755 target/release/libgstcaptions.so ~/.local/share/gstreamer-1.0/plugins/
```

Or install system-wide:

```sh
sudo install -m 0755 target/release/libgstcaptions.so "$(pkg-config --variable=pluginsdir gstreamer-1.0)/"
```

Or install with [`cargo-c`](https://github.com/lu-zero/cargo-c):

```sh
cargo install cargo-c
GSTREAMER_LIBDIR="$(pkg-config --variable=libdir gstreamer-1.0)"
cargo cinstall --release --library-type cdylib \
  --prefix="$(pkg-config --variable=prefix gstreamer-1.0)" \
  --libdir="$GSTREAMER_LIBDIR"
```

You can also build and inspect the plugin in Docker without installing GStreamer locally:

```sh
docker build -t gst-captions .
docker run --rm gst-captions
```

## Element reference

### `captionstranscriber`

Speech-to-text across every model family supported by `transcribe.cpp` (Parakeet, Canary, Whisper, Moonshine, Voxtral, Qwen3-ASR, Nemotron, and more).
Its sink pad accepts mono `F32LE` audio at the loaded model's native sample rate and its src pad outputs `text/x-raw,format=utf8`. Place `audioconvert ! audioresample` upstream so GStreamer negotiates the sample rate automatically.

`model-path` is the only required property; every other setting defaults sensibly for the loaded model family.

#### Core properties

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `model-path` | string | (required) | Path to a GGUF (or legacy ggml) model supported by `transcribe.cpp` |
| `mode` | enum | `auto` | `auto` uses native streaming when the model supports it, otherwise `chunked`; or force `stream` / `chunked` |
| `backend` | enum | `auto` | Compute backend: `auto`, `cpu`, `cpu-accel`, `metal`, `vulkan`, `cuda` (`cpu` is deterministic) |
| `n-threads` | int | `0` | CPU thread count for CPU ops (`0` uses the library default) |
| `gpu-device` | int | `0` | GPU device index (`0` auto-selects, preferring discrete GPUs) |
| `model-info` | structure (read-only) | — | Model metadata reported after loading (resolved `mode`, `vad`, `max-timestamp-kind`, etc.) |

#### Timing and voice activity detection

The first three properties sum into the latency reported downstream:

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `latency` | uint (ms) | `1000` | Declared processing budget; live pipelines warn when inference runs slower than real time |
| `chunk-duration` | uint (ms) | `4000` | `mode=chunked`: audio accumulated between inference runs |
| `live-edge-offset` | uint (ms) | `1000` | `mode=chunked`: trailing audio withheld as unstable (must be less than `chunk-duration`) |
| `discont-threshold` | uint (ms) | `500` | Timeline jump above this finalizes the current stream and starts fresh |
| `vad` | boolean | `true` | `mode=stream`: discard audio until speech starts so the model never opens on silence |
| `vad-threshold` | float | `0.6` | Speech score threshold from `0.0` to `1.0` (higher is stricter) |
| `vad-model-path` | string | (none) | Optional Silero VAD GGUF ([`silero-vad-v6.2`](https://huggingface.co/handy-computer/silero-vad-v6.2-gguf)); unset uses built-in `earshot` |
| `warmup-pad` | uint (ms) | `80` | `mode=stream`: priming silence fed before first speech (`0` disables) |

#### Text, language, and decoding

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `language` | string | (none) | Source language ISO code (unset auto-detects) |
| `task` | enum | `transcribe` | `transcribe` in the source language, or `translate` into `target-language` |
| `target-language` | string | (none) | Target language ISO code when `task=translate` |
| `timestamps` | enum | `auto` | Requested timestamp granularity: `none`, `auto`, `segment`, `word`, `token` |
| `pnc` | enum | `default` | Punctuation and capitalization toggle (where supported) |
| `itn` | enum | `default` | Inverse text normalization toggle (where supported) |
| `keep-special-tags` | boolean | `false` | Retain special vocabulary tags in output text |
| `commit-policy` | enum | `auto` | `mode=stream`: when committed text advances (`auto`, `on-finalize`, `stable-prefix`) |
| `stable-prefix-agreement-n` | uint | `0` | `mode=stream`: agreeing hypotheses before prefix commit (`0` uses library default) |
| `n-ctx` | int | `0` | Decoder context limit in tokens (`0` uses model maximum) |
| `kv-type` | enum | `auto` | K/V cache precision: `auto`, `f32`, `f16` |
| `spec-k-drafts` | int | `-1` | Speculative-decode draft length (`-1` family default, `0` off) |
| `family-options` | structure | (none) | Family-specific options structure (e.g. `parakeet-buffered,left-ms=1920,chunk-ms=640,right-ms=320`) |
| `queue-size` | uint | `32` | Maximum audio buffers queued for inference on live sources |
| `overrun` | enum | `block` | Queue behavior when full: `block` upstream or `drop` excess audio |

Recognized `family-options` structure names are `whisper`, `parakeet-stream`, `parakeet-buffered`, `moonshine-streaming`, and `voxtral-realtime`, using kebab-case field names matching `transcribe_cpp`.

#### How streaming, chunked mode, and VAD work

- **Modes:** `mode=stream` uses the model's native streaming API (latency equals the family's commit lag); `mode=chunked` runs sliding-window inference over any model family, starting each window exactly where the last committed text ended and withholding trailing words past `live-edge-offset` until the next pass.
- **Output guarantees:** Every pushed buffer is valid UTF-8 with PTS and non-zero duration, strictly monotonic and non-overlapping. Volatile text never reaches the pad—only committed text is pushed, so subtitle muxers stay append-only. Timeline holes become downstream `GAP` events, and new segments, discontinuities, flushes, or EOS finalize the stream cleanly.
- **Granularity:** `timestamps` is a request, not a guarantee (check `model-info`'s `max-timestamp-kind`). In practice, Whisper produces sentence segments, Nemotron emits one buffer per word, and Moonshine emits one span per commit. When burning word-level captions directly into video without `captionsrollup`, add `textaccumulate extend-duration=true` so cues do not strobe between words.
- **Partials:** Volatile hypotheses are emitted on the `partial-transcript` GObject signal (only for families with a volatile suffix; callbacks run on the streaming thread and must not block). To watch commits and live partials from the terminal, run the bundled example:

  ```sh
  cargo run --release --example transcribe -- model.gguf speech.wav
  cargo run --release --example transcribe -- model.gguf udp://5004 mode=chunked
  ```

- **VAD gate:** Several streaming checkpoints degrade if opened on silence, so `mode=stream` drops leading audio until speech begins and feeds `warmup-pad` milliseconds of silence first (set `vad=false` to disable). The built-in `earshot` detector needs no model file, while `vad-model-path` loads Silero VAD on one thread (<1% of a core). Both handle silence and room noise well, but Silero rejects intro music that triggers `earshot`—improving opening word recall on streams with long musical intros. A bad VAD file or sample-rate mismatch fails startup; a mid-stream Silero error opens the gate rather than muting captions.
- **Events:** Emits `rstranscribe/final-transcript` downstream at utterance boundaries and finalization. Does not emit `rstranscribe/speaker-change` (no diarization) or bus messages. Note that only a genuinely live upstream source answers the latency query as live (a bare `clocksync` does not).

---

### `captionsrollup`

A text-to-text filter (`text/x-raw,format=utf8`) that formats timestamped words into a fixed roll-up window. Completed lines freeze permanently while only the bottom line grows—replacing `textaccumulate ! textwrap` with zero extra formatter latency and no jittering reflow on already-displayed lines.

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `columns` | uint | `42` | Maximum display width per line (Unicode display width; wide CJK characters count as 2) |
| `lines` | uint | `2` | Visible lines in the roll-up window |
| `clear-after` | uint (ms) | `3000` | Media-time silence after the last word before emitting an empty clear buffer (`0` disables) |
| `break-on-sentence` | boolean | `true` | End the current line at sentence-final punctuation even before reaching `columns` |

Properties can be changed while playing and take effect on future lines without rewriting frozen lines.

**Notes:**
- Input buffers must have a PTS and non-zero duration; an empty UTF-8 buffer explicitly clears the window at its PTS.
- Clearing is driven by media-time `GAP` events (not a wall clock): when a `GAP` crosses `last_input_end + clear-after`, the element emits a zero-duration empty buffer at that exact timestamp. (If upstream omits `GAP` events, the clear fires when the next word arrives.)
- A single word wider than `columns` occupies its own line rather than splitting mid-word.
- New segments reset the window so captions never leak across timelines. Upstream latency is forwarded unchanged.

---

### `captionsflvmux`

A strict `GstAggregator` that injects timed text into an already-muxed FLV stream as AMF0 script tags. Place it directly after `flvmux` or `eflvmux`—original audio/video FLV tags pass through unmodified with caption tags interleaved at the exact media timestamps.

Pads: `sink` (`video/x-flv`), `text` (`text/x-raw,format=utf8`), and `src` (`video/x-flv`).

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `message-name` | enum | `oncaption` | AMF0 script message name: `oncaption` (avoids an FFmpeg warning on `onTextData`) or `ontextdata` |
| `input-mode` | enum | `timed` | `timed` shows text from PTS to `PTS + duration`; `replacement` holds text until replaced or cleared by an empty buffer |
| `prime` | boolean | `true` | Emit one empty cue at the first timestamped FLV frame so downstream origins discover the subtitle track immediately |

Standard `GstAggregator` properties (such as `latency`) also apply.

**Notes:**
- Both sink pads require `TIME` segments at rate `1.0`, and every text buffer must carry valid UTF-8 with PTS and duration (`GAP` events supply coverage during silence; `GAP` buffers are never decoded as text).
- Coverage is strict: the text branch must continuously cover the media timeline. Missing, overlapping, or non-monotonic text coverage errors out, and a slow text branch backpressures the FLV branch rather than dropping or clamping timestamps.
- Each caption AMF0 object contains one `text` property; cues exceeding the AMF0 short-string limit are truncated with a warning.

## Repository layout

- `src/flvmux/`: FLV caption aggregation (`amf.rs`, `caption.rs`, `flv.rs`, element shell)
- `src/rollup/`: Roll-up window geometry and element shell
- `src/transcriber/`: Streaming/chunked inference driver, commit tracker, VAD gate, worker
- `src/logging.rs`: `transcribe.cpp` log bridge
- `tests/`: Element harnesses (`flvmux`, `rollup`), roll-up geometry oracle (`oracle`), and ecosystem interop (`interop`)
- `examples/`: `transcribe` (live commits/partials CLI), `vadscore` (VAD calibration), `vadcompare` (earshot vs. Silero)
- `fixtures/`: Golden roll-up window fixture

## Testing and debugging

Run the model-free test suite (unit tests, roll-up oracle, and `gst-check` harnesses for all three elements):

```sh
cargo test --locked
cargo test --locked --no-default-features --features flvmux,rollup
```

Run the optional model-backed interoperability tests in `tests/interop.rs`:

```sh
GST_CAPTIONS_MODEL=model.gguf GST_CAPTIONS_WAV=speech.wav cargo test --release --test interop
```

Enable GStreamer debug logs to inspect timeline coverage, roll-up state, or inference timing (`captionslib` surfaces `transcribe.cpp`'s internal logs):

```sh
GST_DEBUG=captionsflvmux:6 gst-launch-1.0 ...
GST_DEBUG=captionsrollup:6 gst-launch-1.0 ...
GST_DEBUG=captionstranscriber:6,captionslib:5 gst-launch-1.0 ...
```

## License

MPL-2.0. See [LICENSE](LICENSE).

[ci-shield]: https://img.shields.io/github/actions/workflow/status/darfink/gst-captions/ci.yml?branch=main&label=CI&logo=github&style=for-the-badge
[ci]: https://github.com/darfink/gst-captions/actions/workflows/ci.yml?query=branch%3Amain
[version-shield]: https://img.shields.io/github/v/tag/darfink/gst-captions?style=for-the-badge&label=version
[releases]: https://github.com/darfink/gst-captions/releases
[gstreamer-shield]: https://img.shields.io/badge/GStreamer-1.20+-orange?style=for-the-badge
[gstreamer]: https://gstreamer.freedesktop.org/
[license-shield]: https://img.shields.io/badge/license-MPL--2.0-green?style=for-the-badge
[license]: LICENSE
