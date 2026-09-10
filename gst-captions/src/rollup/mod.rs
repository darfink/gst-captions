// SPDX-License-Identifier: MPL-2.0

use gst::glib;
use gst::prelude::*;

mod imp;
pub mod window;

glib::wrapper! {
    pub struct CaptionsRollup(ObjectSubclass<imp::CaptionsRollup>) @extends gst::Element, gst::Object;
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
  gst::Element::register(
    Some(plugin),
    "captionsrollup",
    gst::Rank::NONE,
    CaptionsRollup::static_type(),
  )
}
