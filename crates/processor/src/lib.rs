// Licensed under the Business Source License 1.1 — see LICENSE.

pub mod controller;
pub mod stub;

#[cfg(feature = "gstreamer")]
pub mod gst_pipeline;

pub use controller::PipelineController;
pub use stub::StubPipelineController;

#[cfg(feature = "gstreamer")]
pub use gst_pipeline::GstPipelineController;
