// SPDX-License-Identifier: MPL-2.0

//! Live captions for GStreamer: transcribe speech, format roll-up windows,
//! and mux caption cues into FLV streams.

#![allow(clippy::non_send_fields_in_send_ty, unused_doc_comments)]

use gst::glib;

#[cfg(feature = "flvmux")]
pub mod flvmux;
#[cfg(feature = "transcribe")]
mod logging;
#[cfg(feature = "rollup")]
pub mod rollup;
#[cfg(feature = "transcribe")]
pub mod transcriber;

fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
  #[cfg(feature = "transcribe")]
  logging::init();
  #[cfg(feature = "flvmux")]
  flvmux::register(plugin)?;
  #[cfg(feature = "rollup")]
  rollup::register(plugin)?;
  #[cfg(feature = "transcribe")]
  transcriber::register(plugin)?;
  Ok(())
}

gst::plugin_define!(
  captions,
  env!("CARGO_PKG_DESCRIPTION"),
  plugin_init,
  concat!(env!("CARGO_PKG_VERSION"), "-", env!("COMMIT_ID")),
  "MPL",
  env!("CARGO_PKG_NAME"),
  env!("CARGO_PKG_NAME"),
  env!("CARGO_PKG_REPOSITORY"),
  env!("BUILD_REL_DATE")
);
