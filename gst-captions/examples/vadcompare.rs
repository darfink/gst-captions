// SPDX-License-Identifier: MPL-2.0

//! Score the same raw f32 mono 16 kHz file with earshot (the gate's detector)
//! and transcribe.cpp's Silero VAD, fed in live-sized buffers.
//!
//! ```shell
//! cargo run --release --example vadcompare -- audio.f32 silero.gguf out-prefix
//! ```
//!
//! Writes `<out-prefix>.earshot.txt` (16 ms frames) and
//! `<out-prefix>.silero.txt` (32 ms frames) as `start_ms score` lines, and
//! prints each detector's total compute time.

use std::io::Write;
use std::time::{Duration, Instant};

/// Live buffer size: 20 ms, a typical audio buffer duration in the pipeline.
const BUFFER: usize = 320;

fn main() {
  let mut args = std::env::args().skip(1);
  let usage = "usage: vadcompare <raw-f32-mono-16k> <silero.gguf> <out-prefix>";
  let path = args.next().expect(usage);
  let silero_path = args.next().expect(usage);
  let prefix = args.next().expect(usage);

  let bytes = std::fs::read(&path).expect("failed to read input");
  let samples: Vec<f32> = bytes
    .as_chunks::<4>()
    .0
    .iter()
    .map(|b| f32::from_le_bytes(*b))
    .collect();

  // earshot: frame-locked at 256 samples, so carry a remainder across buffers
  // exactly as the gate does.
  let mut detector = earshot::Detector::default_boxed();
  let mut pending: Vec<f32> = Vec::with_capacity(256);
  let mut earshot_scores = Vec::new();
  let mut earshot_time = Duration::ZERO;
  for buffer in samples.chunks(BUFFER) {
    let start = Instant::now();
    for &sample in buffer {
      pending.push(sample);
      if pending.len() == 256 {
        earshot_scores.push(detector.predict_f32(&pending));
        pending.clear();
      }
    }
    earshot_time += start.elapsed();
  }

  let model = transcribe_cpp::Model::load(&silero_path).expect("failed to load silero model");
  let info = model.vad_info().expect("not a VAD model");
  let mut session = model
    .vad_session_with(&transcribe_cpp::VadSessionOptions { n_threads: 1 })
    .expect("failed to open VAD session");
  let mut silero_scores = Vec::new();
  let mut silero_time = Duration::ZERO;
  for buffer in samples.chunks(BUFFER) {
    let start = Instant::now();
    let result = session.stream_feed(buffer).expect("vad feed failed");
    silero_time += start.elapsed();
    silero_scores.extend(result.probs);
  }

  let audio_secs = samples.len() as f64 / 16_000.0;
  write_scores(&format!("{prefix}.earshot.txt"), &earshot_scores, 16.0);
  let silero_frame_ms = info.frame_samples as f64 * 1000.0 / info.sample_rate as f64;
  write_scores(
    &format!("{prefix}.silero.txt"),
    &silero_scores,
    silero_frame_ms,
  );

  println!(
    "audio {audio_secs:.1}s earshot {:.1}ms ({:.0}x rt) silero {:.1}ms ({:.0}x rt)",
    earshot_time.as_secs_f64() * 1000.0,
    audio_secs / earshot_time.as_secs_f64(),
    silero_time.as_secs_f64() * 1000.0,
    audio_secs / silero_time.as_secs_f64(),
  );
}

fn write_scores(path: &str, scores: &[f32], frame_ms: f64) {
  let mut file = std::io::BufWriter::new(std::fs::File::create(path).expect("create output"));
  for (index, score) in scores.iter().enumerate() {
    writeln!(file, "{:.0} {score:.4}", index as f64 * frame_ms).unwrap();
  }
}
