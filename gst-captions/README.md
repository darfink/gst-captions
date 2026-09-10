## gst-captions — live captions: transcribe, roll up, mux

One plugin (`captions`) for the whole live-caption chain. All three elements
are enabled by default; trim them with cargo features (see [Features](#features)).

| Element | Pads | Purpose |
| --- | --- | --- |
| `captionstranscriber` | audio in, text out | Speech-to-text backed by transcribe.cpp |
| `captionsrollup` | text filter | Word buffers into a fixed roll-up caption window |
| `captionsflvmux` | FLV plus text in, FLV out | Captions into a muxed FLV stream as script data |

The intended wiring is transcriber into roll-up into muxer, with the roll-up
feeding the muxer's `text` pad in `input-mode=replacement`:

```text
captionstranscriber ! captionsrollup ! captionsflvmux input-mode=replacement
```

### captionstranscriber

Speech to text over every model family transcribe.cpp supports (parakeet,
canary, whisper, moonshine, voxtral, qwen3-asr, nemotron, and more) rather
than a single one. The sink pad takes mono `F32LE` audio at whatever rate the
loaded model wants; the src pad emits `text/x-raw,format=utf8`.
Put `audioconvert ! audioresample` in front of it and negotiation lands on
the right rate by itself — there is no rate property.

`model-path` is the only property you must set. Everything else has a default
that suits the loaded family.

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `model-path` | string | (none, required) | Path to a GGUF model understood by transcribe.cpp |
| `mode` | enum | `auto` | `auto` streams when the model supports it (see [Modes](#modes)), else `stream` or `chunked` |
| `backend` | enum | `auto` | Which compute backend to request: `auto`, `cpu`, `cpu-accel`, `metal`, `vulkan`, `cuda`. `cpu` is the deterministic choice |
| `n-threads` | int | `0` | CPU threads for ops that run on CPU; `0` uses the library default |
| `gpu-device` | int | `0` | GPU device registry index; `0` auto-selects, preferring discrete GPUs |
| `model-info` | structure (read-only) | — | What the loaded model reported about itself; unset until loaded |

Timing. The first three add up to the latency reported downstream:

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `latency` | uint (ms) | `1000` | Declared processing budget. A live pipeline warns when inference runs slower than real time |
| `chunk-duration` | uint (ms) | `4000` | `mode=chunked`: new audio accumulated before each inference run |
| `live-edge-offset` | uint (ms) | `1000` | `mode=chunked`: trailing audio whose words are withheld as unstable. Must be less than `chunk-duration` |
| `discont-threshold` | uint (ms) | `500` | A timeline jump larger than this finalizes the current stream and starts a new one |
| `vad` | boolean | `true` | `mode=stream`: discard audio until speech is detected, so the model never opens its stream on silence |
| `vad-threshold` | float | `0.6` | Score from 0 to 1 a frame must reach to count as speech. Higher is stricter |
| `warmup-pad` | uint (ms) | `80` | `mode=stream`: milliseconds of digital silence fed as a priming chunk before the first speech. `0` disables it |

Text and language:

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `language` | string | (none) | Source language hint (ISO code); unset auto-detects |
| `task` | enum | `transcribe` | `transcribe` speech in its source language, or `translate` it into the target language |
| `target-language` | string | (none) | Target language (ISO code) when `task=translate` |
| `timestamps` | enum | `auto` | Timestamp granularity to request: `none`, `auto`, `segment`, `word`, `token`. A request, not a guarantee (see [Output](#output)) |
| `pnc` | enum | `default` | Punctuation and capitalization toggle, on supporting families |
| `itn` | enum | `default` | Inverse text normalization toggle, on supporting families |
| `keep-special-tags` | boolean | `false` | Keep special vocabulary tags in the returned text |

Streaming and decoding:

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `commit-policy` | enum | `auto` | `mode=stream`: when committed text is allowed to grow: `auto`, `on-finalize`, `stable-prefix` |
| `stable-prefix-agreement-n` | uint | `0` | `mode=stream`: consecutive agreeing hypotheses before a prefix commits; `0` uses the library default |
| `n-ctx` | int | `0` | Decoder context cap in tokens; `0` uses the model maximum |
| `kv-type` | enum | `auto` | K/V activation precision: `auto`, `f32`, `f16` |
| `spec-k-drafts` | int | `-1` | Speculative-decode draft length; `-1` family default, `0` disabled |
| `family-options` | structure | (none) | Family-specific knobs, as a structure named after the family (see [Examples](#transcriber-examples)) |

Flow control on a live source:

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `queue-size` | uint | `32` | How many audio buffers may be queued for inference |
| `overrun` | enum | `block` | `block` upstream until the worker catches up, or `drop` audio that does not fit the queue |

Signals and events: one signal, `partial-transcript` (see [Partials](#partials)).
Emits the `rstranscribe/final-transcript` custom downstream event that the
gst-plugins-rs transcribers use, so `textaccumulate` can group words into
cues. `rstranscribe/speaker-change` is not emitted: this element does not
diarize. No bus messages.

Quirks: takes its live path automatically when upstream answers the latency
query as live (a bare `clocksync` is not enough — only a genuinely live
source answers as live). In `chunked` mode each window begins exactly where
the last emitted text ended, with no fixed overlap: a row running past the
live edge is withheld and retried on the next run until it completes. In
`stream` mode audio is discarded until voice activity is detected and a
short priming silence is fed first, because several streaming checkpoints
poison their state when opened on silence; set `vad=false` rather than
working around a detector that misreads a source.

#### Modes

| | `stream` | `chunked` |
|---|---|---|
| Uses | native streaming API | sliding-window offline inference |
| Works with | families advertising streaming support | every family |
| Latency | the family's own commit lag | `chunk-duration` + `live-edge-offset` |
| Tuning | `commit-policy`, `stable-prefix-agreement-n` | `chunk-duration`, `live-edge-offset` |

`mode=auto` (the default) picks `stream` when the loaded model advertises
streaming support and `chunked` otherwise. The resolved mode is logged at
INFO and readable from `model-info`.

#### Output

Every committed word buffer is valid UTF-8 with PTS and non-zero duration;
buffers are monotonic and non-overlapping. Only committed text is pushed —
text the model may still rewrite never reaches the pad, so the output is
append-only and safe for subtitle muxers. Initial, internal, and trailing
timeline holes become GAP events. New segments, discontinuities, flushes, and
EOS finalize or reset the append-only timeline before continuing.

Granularity follows what the family actually produces, best first: word rows
where the model aligns to words, segment rows otherwise, token rows joined
back into words as a last resort. `timestamps` is a request, not a
guarantee — leave it at `auto` and check `model-info`'s
`max-timestamp-kind` before assuming a family aligns anything. In practice:
whisper gives sentence-length captions, nemotron one buffer per word, and
moonshine one span per commit. Set `timestamps=none` to get one buffer per
commit with no alignment data at all.

#### Grouping words into cues

A word-aligned family emits a row per word — tens of milliseconds of text with
a gap after it. That is the right granularity for anything doing its own
layout (like `captionsrollup`), but burned into video at 24 fps most frames
land in a gap, so the caption strobes. Grouping is not this element's job: it
emits `rstranscribe/final-transcript` after the buffer that closes an
utterance and whenever the stream finalizes, and `textaccumulate` drains on
it by default:

```sh
captionstranscriber model-path=model.gguf ! textaccumulate extend-duration=true
```

#### Partials

The volatile hypothesis never reaches the pad, so it is delivered as the
`partial-transcript` signal instead, for live UI. It only fires when the
family actually maintains a volatile suffix, and handlers run on the streaming
thread and must not block. `gst-launch-1.0` cannot connect to signals, so
the bundled example exists to watch them (see
[Examples](#transcriber-examples)).

#### Transcriber examples

```sh
# Streamed inference over a file
gst-launch-1.0 filesrc location=speech.wav ! decodebin ! audioconvert ! audioresample ! captionstranscriber model-path=model.gguf ! fakesink dump=true

# Chunked inference with a whisper model (also reads legacy ggml files)
gst-launch-1.0 filesrc location=speech.wav ! wavparse ! audioconvert ! audioresample ! captionstranscriber model-path=ggml-small.en.bin chunk-duration=4000 live-edge-offset=1000 latency=1000 ! fakesink dump=true

# Family-specific knobs go through one structure-valued property, named after the family
captionstranscriber model-path=parakeet.gguf family-options="parakeet-buffered,left-ms=1920,chunk-ms=640,right-ms=320"
```

Recognized family names: `whisper`, `parakeet-stream`,
`parakeet-buffered`, `moonshine-streaming`, `voxtral-realtime`. Fields are
the kebab-case spelling of the corresponding `transcribe_cpp` option field.

The bundled example prints committed buffers with their timestamps and
overwrites the volatile hypothesis in place, the way a live caption view
would. It accepts `udp://PORT` in place of the file to drive the live path,
and `property=value` arguments to set any element property:

```sh
cargo run --release --example transcribe -- model.gguf speech.wav
cargo run --release --example transcribe -- model.gguf udp://5004 mode=chunked
```

### captionsrollup

A text filter that turns timestamped word-level text into a fixed roll-up
caption window. Completed lines freeze; only the bottom line grows. In the
position it is meant for it replaces `textaccumulate` + `textwrap`,
because owning the line breaking is what keeps already captioned lines
byte-identical while the window is still being written.

It is built for live captioning, where the viewer reads the text while it is
still being written. Two properties follow from that: nothing already on
screen moves (a line's text is decided once, when it fills, and never
recomputed), and a state is emitted the moment input commits rather than
batched toward a sentence boundary — the element adds no formatter latency.

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `columns` | uint | `42` | Maximum display width per line (Unicode display width, so wide CJK characters count as two) |
| `lines` | uint | `2` | Number of lines in the roll-up window |
| `clear-after` | uint (ms) | `3000` | Media-time silence after the previous input end before emitting an explicit clear; `0` disables |
| `break-on-sentence` | boolean | `true` | Finish the current line at sentence-final punctuation even if it is not full |

All properties stay writable while playing, but affect future wrapping only:
lines that are already frozen keep the geometry they were built with.

Signals and events: none. Forwards stream-start, segment, flush, and relevant
custom downstream events. Timeline holes are represented by downstream GAP
events.

Quirks: sink and src pads both carry `text/x-raw,format=utf8`. Every input
buffer must have a PTS and non-zero duration; an empty UTF-8 buffer is an
explicit clear at its PTS. Clearing runs on media-time GAP events, not a
wall-clock timer: when a GAP crosses `last_input_end + clear-after`, the
element emits one zero-duration empty buffer at that exact media timestamp. A
later word performs the same missed-clear check if upstream omitted GAP
events, so upstream should announce silence with GAP events to publish the
clear while silence is in progress. A single word longer than `columns` gets
a line to itself rather than being broken mid-word. A new segment clears the
window so text cannot leak across timelines. The element reports upstream
latency unchanged.

### captionsflvmux

A strict aggregator that inserts timed text into a muxed FLV stream as AMF0
script data. Run it after `flvmux` or `eflvmux`, which provide no text pad
of their own: their output is self-delimiting FLV tags, so this element can
insert a correctly framed caption tag between media tags and forward the
original media buffers unchanged.

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `message-name` | enum | `oncaption` | AMF0 script-data message name carrying each cue: `oncaption` or `ontextdata` |
| `input-mode` | enum | `timed` | Interpret text buffers as finite `timed` intervals, or as persistent `replacement` states |
| `prime` | boolean | `true` | Write one empty cue at the first timestamped FLV position to declare the subtitle stream |

(Base-class aggregator properties such as `latency` behave as documented
for `GstAggregator`.)

Signals and events: none.

Quirks: three always-present pads — `sink` (`video/x-flv` from the muxer),
`text` (`text/x-raw,format=utf8`), `src` (`video/x-flv` with captions).
Every text buffer must have a PTS and duration and be valid UTF-8. GAP events
become coverage intervals; GAP buffers are never decoded as text. The text
timeline must continuously cover the media timeline — missing, overlapping,
non-monotonic, or contradictory coverage is an error, and when the text
branch falls behind the aggregator waits and backpressures the FLV branch
rather than clamping, dropping, or timing out. `input-mode=timed` shows text
at its PTS and clears at PTS plus duration; `input-mode=replacement` keeps
text until another state replaces it, with an empty buffer as the explicit
clear. Both sink pads require TIME segments at rate 1.0. The caption payload
is AMF0 with one `text` property; `onCaption` is the default because it
avoids a diagnostic FFmpeg emits for `onTextData`. Cue text past the AMF0
short-string limit is truncated with a warning. After coverage is proven a
transition keeps its original timestamp — never clamped to the media
position. Upstream note: file-mode `flvmux` strips buffer timestamps, so set
`streamable=true` on it (or stream to a non-seekable sink); otherwise every
media buffer arrives without PTS, no cue can be placed, and the element warns
once. The muxer also accepts sentence-length or accumulated buffers in
`input-mode=timed`, and the roll-up accepts word buffers from any transcriber
(see “Mixing with the ecosystem” in the root README).

### Features

| Feature | Default | Effect |
| --- | --- | --- |
| `flvmux` | on | Build `captionsflvmux` |
| `rollup` | on | Build `captionsrollup` |
| `transcribe` | on | Build `captionstranscriber` (CPU-only; needs a C++ toolchain and cmake) |
| `metal` | off | GPU backend, forwarded to `transcribe-cpp` (macOS) |
| `cuda` | off | GPU backend, forwarded to `transcribe-cpp` (NVIDIA) |
| `vulkan` | off | GPU backend, forwarded to `transcribe-cpp` (cross-vendor) |
| `openmp` | off | CPU threading backend, forwarded to `transcribe-cpp` |
| `dynamic-backends` | off | Runtime backend loading, forwarded to `transcribe-cpp` |

### Layout

- `src/flvmux/`: strict FLV subtitle aggregation (AMF0 framing, caption timeline, FLV tags).
- `src/rollup/`: roll-up window geometry plus the element shell.
- `src/transcriber/`: streaming/chunked inference driver (commit tracker, VAD gate, worker).
- `src/logging.rs`: transcribe.cpp log bridge.
- `tests/`: element harnesses (`flvmux`, `rollup`) and a fixture oracle for the window geometry.
- `examples/`: `transcribe` (watch commits and partials) and `vadscore` (calibrate the VAD gate).
- `fixtures/`: oracle window data shared with the Python reference in `scratchpad/`.

### Tests

```sh
cargo test
cargo test --no-default-features --features flvmux,rollup
```

### Debugging

```sh
GST_DEBUG=captionsflvmux:6 gst-launch-1.0 ...
GST_DEBUG=captionsrollup:6 gst-launch-1.0 ...
GST_DEBUG=captionstranscriber:6,captionslib:5 gst-launch-1.0 ...
```

Logs include text coverage errors and queued transitions (muxer); GAP
splitting, clear positions, and rendered state (roll-up); model load,
committed words, and compute time against audio consumed (transcriber), with a
live-pipeline warning when inference falls behind real time. `captionslib`
carries transcribe.cpp's own log output, where decoder-level detail lives.

### Build / inspect

From the repository root:

```sh
cargo build --release
export GST_PLUGIN_PATH="$PWD/target/release"
gst-inspect-1.0 captionsflvmux captionsrollup captionstranscriber
```
