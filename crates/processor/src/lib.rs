// Licensed under the GNU Affero General Public License v3.0 — see LICENSE.

pub mod controller;
pub mod stub;

#[cfg(feature = "gstreamer")]
pub mod gst_pipeline;

pub use controller::PipelineController;
pub use stub::StubPipelineController;

#[cfg(feature = "gstreamer")]
pub use gst_pipeline::GstPipelineController;
