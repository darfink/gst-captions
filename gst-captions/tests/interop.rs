// SPDX-License-Identifier: MPL-2.0

//! Cross-element interop: our elements against each other and the ecosystem.
//!
//! Always-on tests use fixed buffers shaped like other elements output, so
//! they need no model. Model-backed tests run only when GST_CAPTIONS_MODEL
//! and GST_CAPTIONS_WAV point at a GGUF model and a WAV file.

use gst::prelude::*;
use gstcaptions::flvmux::flv::{TAG_TYPE_SCRIPT_DATA, parse_tag_header};

fn init() {
  use std::sync::Once;
  static INIT: Once = Once::new();
  INIT.call_once(|| {
    gst::init().unwrap();
    gstcaptions::plugin_register_static().unwrap();
  });
}

fn have(factory: &str) -> bool {
  init();
  gst::ElementFactory::find(factory).is_some()
}

fn ms(value: u64) -> gst::ClockTime {
  gst::ClockTime::from_mseconds(value)
}

fn word_buffer(pts_ms: u64, duration_ms: u64, text: &str) -> gst::Buffer {
  let mut buffer = gst::Buffer::from_mut_slice(text.as_bytes().to_vec());
  {
    let buffer = buffer.get_mut().unwrap();
    buffer.set_pts(ms(pts_ms));
    buffer.set_duration(ms(duration_ms));
  }
  buffer
}

fn make_flv(timestamp_ms: u64) -> gst::Buffer {
  let body = [0x17u8, 0x01, 0, 0, 0];
  let mut tag = vec![9u8];
  tag.extend_from_slice(&(body.len() as u32).to_be_bytes()[1..]);
  let timestamp = timestamp_ms as u32;
  tag.extend_from_slice(&timestamp.to_be_bytes()[1..]);
  tag.push((timestamp >> 24) as u8);
  tag.extend_from_slice(&[0, 0, 0]);
  tag.extend_from_slice(&body);
  tag.extend_from_slice(&((11 + body.len()) as u32).to_be_bytes());
  let mut buffer = gst::Buffer::from_mut_slice(tag);
  {
    let buffer = buffer.get_mut().unwrap();
    buffer.set_pts(ms(timestamp_ms));
    buffer.set_duration(ms(100));
  }
  buffer
}

// Word buffers shaped like whisper-family output: one word per buffer with
// a leading space after the first, each with PTS and duration.
#[test]
fn rollup_accepts_whisper_shaped_word_buffers() {
  if !have("captionsrollup") {
    eprintln!("skip: captionsrollup not built");
    return;
  }
  init();
  let mut harness = gst_check::Harness::new("captionsrollup");
  harness
    .element()
    .unwrap()
    .set_property("break-on-sentence", false);
  harness.element().unwrap().set_property("clear-after", 0u32);
  harness.set_src_caps_str("text/x-raw, format=utf8");

  for (i, word) in ["Hello", " world", " here"].iter().enumerate() {
    assert_eq!(
      harness.push(word_buffer(1000 + i as u64 * 80, 80, word)),
      Ok(gst::FlowSuccess::Ok)
    );
  }
  let mut last = String::new();
  while let Some(buffer) = harness.try_pull() {
    let map = buffer.map_readable().unwrap();
    last = std::str::from_utf8(map.as_slice()).unwrap().to_owned();
  }
  assert_eq!(last, "Hello world here");
}

// Sentence-length buffers shaped like textaccumulate with extend-duration
// output arrive as timed intervals the muxer shows and clears.
#[test]
fn mux_accepts_sentence_buffers_as_timed_intervals() {
  if !have("captionsflvmux") {
    eprintln!("skip: captionsflvmux not built");
    return;
  }
  init();
  let mut flv = gst_check::Harness::with_padnames("captionsflvmux", Some("sink"), Some("src"));
  let element = flv.element().unwrap();
  element.set_property("prime", false);
  let mut text = gst_check::Harness::with_element(&element, Some("text"), None);
  flv.set_src_caps_str("video/x-flv");
  text.set_src_caps_str("text/x-raw, format=utf8");
  flv.play();

  let sentence = "Hello world, this is a full sentence.";
  text.push(word_buffer(0, 2000, sentence)).unwrap();
  text.push_event(gst::event::Eos::new());
  flv.push(make_flv(0)).unwrap();
  flv.push(make_flv(2000)).unwrap();

  let mut saw_sentence = false;
  let mut saw_clear = false;
  for _ in 0..4 {
    let buffer = flv.pull().expect("muxed output");
    let map = buffer.map_readable().unwrap();
    let header = parse_tag_header(map.as_slice()).expect("tag header");
    if header.tag_type == TAG_TYPE_SCRIPT_DATA {
      if map
        .as_slice()
        .windows(sentence.len())
        .any(|w| w == sentence.as_bytes())
      {
        saw_sentence = true;
      } else {
        saw_clear = true;
      }
    }
  }
  assert!(saw_sentence, "sentence cue must be muxed as a script tag");
  assert!(saw_clear, "interval end must mux a clear cue");
}

// No-model end to end: word buffers through roll-up into the muxer in
// replacement mode, with flvmux streamable=true so media buffers keep
// their timestamps (file-mode flvmux strips them and no cue can be placed).
#[test]
fn rollup_replacement_states_reach_flv_script_tags() {
  for factory in ["captionsrollup", "captionsflvmux", "flvmux", "x264enc"] {
    if !have(factory) {
      eprintln!("skip: {factory} unavailable");
      return;
    }
  }
  init();
  let pipeline = gst::parse::launch(
    "videotestsrc num-buffers=90 ! video/x-raw,framerate=30/1 ! x264enc tune=zerolatency ! flvmux name=fm streamable=true ! captionsflvmux name=cm input-mode=replacement ! appsink name=sink appsrc name=textsrc is-live=false format=time ! captionsrollup ! cm.text",
  )
  .unwrap();
  let pipeline = pipeline.downcast::<gst::Pipeline>().unwrap();
  let appsink = pipeline
    .by_name("sink")
    .unwrap()
    .downcast::<gst_app::AppSink>()
    .unwrap();
  let appsrc = pipeline
    .by_name("textsrc")
    .unwrap()
    .downcast::<gst_app::AppSrc>()
    .unwrap();
  appsrc.set_caps(Some(
    &gst::Caps::builder("text/x-raw")
      .field("format", "utf8")
      .build(),
  ));
  appsrc.set_format(gst::Format::Time);
  appsrc
    .push_buffer(word_buffer(0, 2000, "hello world"))
    .unwrap();
  appsrc
    .push_buffer(word_buffer(2000, 2000, "second state"))
    .unwrap();
  appsrc.end_of_stream().unwrap();
  pipeline.set_state(gst::State::Playing).unwrap();
  let mut script_tags = 0;
  let mut saw_words = false;
  while let Some(sample) = appsink.try_pull_sample(gst::ClockTime::from_seconds(20)) {
    let buffer = sample.buffer().unwrap();
    let map = buffer.map_readable().unwrap();
    if let Some(header) = parse_tag_header(map.as_slice()) {
      if header.tag_type == TAG_TYPE_SCRIPT_DATA {
        script_tags += 1;
        let bytes = map.as_slice();
        if bytes.windows(5).any(|w| w == b"hello") || bytes.windows(6).any(|w| w == b"second") {
          saw_words = true;
        }
      }
    }
  }
  pipeline.set_state(gst::State::Null).unwrap();
  assert!(script_tags >= 1, "expected script tags, got {script_tags}");
  assert!(saw_words, "expected caption words inside a script tag");
}

fn gated_model_and_wav() -> Option<(String, String)> {
  let model = std::env::var("GST_CAPTIONS_MODEL").ok()?;
  let wav = std::env::var("GST_CAPTIONS_WAV").ok()?;
  if std::path::Path::new(&model).is_file() && std::path::Path::new(&wav).is_file() {
    Some((model, wav))
  } else {
    None
  }
}

fn pull_text_samples(appsink: &gst_app::AppSink, timeout_secs: u64) -> Vec<String> {
  let mut out = Vec::new();
  while let Some(sample) = appsink.try_pull_sample(gst::ClockTime::from_seconds(timeout_secs)) {
    let buffer = sample.buffer().unwrap();
    let map = buffer.map_readable().unwrap();
    if let Ok(text) = std::str::from_utf8(map.as_slice()) {
      if !text.trim().is_empty() {
        out.push(text.to_owned());
      }
    }
  }
  out
}

fn drain_pipeline_error(pipeline: &gst::Pipeline) -> Option<String> {
  let bus = pipeline.bus().unwrap();
  let mut error = None;
  while let Some(msg) = bus.pop() {
    if let gst::MessageView::Error(err) = msg.view() {
      error = Some(err.error().to_string());
    }
  }
  error
}

// Our transcriber feeds the stock textwrap formatter.
#[cfg(feature = "transcribe")]
#[test]
fn transcriber_feeds_stock_textwrap() {
  let Some((model, wav)) = gated_model_and_wav() else {
    eprintln!("skip: set GST_CAPTIONS_MODEL and GST_CAPTIONS_WAV");
    return;
  };
  if !have("captionstranscriber") || !have("textwrap") {
    eprintln!("skip: captionstranscriber or textwrap unavailable");
    return;
  }
  init();
  let launch = format!(
    "filesrc location={wav} ! wavparse ! audioconvert ! audioresample ! captionstranscriber model-path={model} ! textwrap lines=2 ! appsink name=sink sync=false",
    wav = wav,
    model = model,
  );
  let pipeline = gst::parse::launch(&launch).unwrap();
  let pipeline = pipeline.downcast::<gst::Pipeline>().unwrap();
  let appsink = pipeline
    .by_name("sink")
    .unwrap()
    .downcast::<gst_app::AppSink>()
    .unwrap();
  pipeline.set_state(gst::State::Playing).unwrap();
  let texts = pull_text_samples(&appsink, 120);
  let error = drain_pipeline_error(&pipeline);
  pipeline.set_state(gst::State::Null).unwrap();
  assert!(error.is_none(), "pipeline error: {error:?}");
  assert!(!texts.is_empty(), "expected wrapped transcription output");
}

// The stock transcribe.cpp element feeds our roll-up window.
#[test]
fn stock_transcriber_feeds_our_rollup() {
  let Some((model, wav)) = gated_model_and_wav() else {
    eprintln!("skip: set GST_CAPTIONS_MODEL and GST_CAPTIONS_WAV");
    return;
  };
  if !have("transcribecpptranscriber") || !have("captionsrollup") {
    eprintln!("skip: transcribecpptranscriber or captionsrollup unavailable");
    return;
  }
  init();
  let launch = format!(
    "filesrc location={wav} ! wavparse ! audioconvert ! audioresample ! transcribecpptranscriber model-path={model} ! captionsrollup ! appsink name=sink sync=false",
    wav = wav,
    model = model,
  );
  let pipeline = gst::parse::launch(&launch).unwrap();
  let pipeline = pipeline.downcast::<gst::Pipeline>().unwrap();
  let appsink = pipeline
    .by_name("sink")
    .unwrap()
    .downcast::<gst_app::AppSink>()
    .unwrap();
  pipeline.set_state(gst::State::Playing).unwrap();
  let texts = pull_text_samples(&appsink, 120);
  let error = drain_pipeline_error(&pipeline);
  pipeline.set_state(gst::State::Null).unwrap();
  assert!(error.is_none(), "pipeline error: {error:?}");
  assert!(
    !texts.is_empty(),
    "expected roll-up states from stock transcriber"
  );
}
