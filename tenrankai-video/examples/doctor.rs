//! Small standalone production prerequisite check, without building the server.
use tenrankai_video::{CancellationToken, VideoProcessor, VideoTools};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let processor = VideoProcessor::new(VideoTools::default())?;
    let cancel = CancellationToken::new();
    let report = processor.check_prerequisites(&cancel).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !report.ready() {
        return Err("video prerequisites are incomplete".into());
    }
    if std::env::args().any(|arg| arg == "--smoke-test") {
        processor.smoke_test(&cancel).await?;
        println!("Video processing smoke test passed.");
    }
    Ok(())
}
