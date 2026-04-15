// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

pub mod controller;
pub mod stub;

#[cfg(feature = "gstreamer")]
pub mod gst_pipeline;

pub use controller::PipelineController;
pub use stub::StubPipelineController;

#[cfg(feature = "gstreamer")]
pub use gst_pipeline::GstPipelineController;
