// SPDX-License-Identifier: MPL-2.0

use gst::glib;
use gst::prelude::*;

mod amf;
mod caption;
pub mod flv;
mod imp;

glib::wrapper! {
  pub struct CaptionsFlvMux(ObjectSubclass<imp::CaptionsFlvMux>)
    @extends gst_base::Aggregator, gst::Element, gst::Object;
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
  gst::Element::register(
    Some(plugin),
    "captionsflvmux",
    gst::Rank::NONE,
    CaptionsFlvMux::static_type(),
  )
}
