pub mod crossfade;
pub mod decode;
pub mod encode;
pub mod error;
pub mod normalize;
pub mod pipeline;
pub mod probe;
pub mod resample;
pub mod types;

#[cfg(any(test, feature = "test-helpers"))]
pub mod testutil;

// Re-export the main entry point
pub use pipeline::run;
