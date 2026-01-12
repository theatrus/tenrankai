//! OpenAI Vision API integration for image analysis
//!
//! This module provides AI-powered image analysis using OpenAI's Vision API
//! to generate keywords and accessibility alt-text for gallery images.

mod background;
mod client;
mod config;
mod error;
mod types;

pub use background::start_background_analysis;
pub use client::OpenAIClient;
pub use config::OpenAIConfig;
pub use error::OpenAIError;
pub use types::{ImageAnalysisResult, ImageContext, LocationContext};
