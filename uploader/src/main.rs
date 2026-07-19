use anyhow::{Context, Result, bail};
use clap::Parser;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Body;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio_util::io::ReaderStream;

/// Upload a file to a URL via HTTP POST, with a live progress bar.
#[derive(Parser, Debug)]
#[command(name = "upload-cli", version, about)]
struct Args {
    /// URL to POST the file to
    url: String,

    /// Path to the file to upload
    file: PathBuf,

    /// HTTP header(s) to add, format "Key: Value" (repeatable)
    #[arg(short = 'H', long = "header", value_name = "KEY:VALUE")]
    headers: Vec<String>,

    /// Field name to send Content-Type as (auto-detected from extension if omitted)
    #[arg(long)]
    content_type: Option<String>,

    /// Request timeout in seconds (0 = no timeout)
    #[arg(long, default_value_t = 0)]
    timeout: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if !args.file.exists() {
        bail!("File not found: {}", args.file.display());
    }

    let metadata = tokio::fs::metadata(&args.file)
        .await
        .with_context(|| format!("Failed to read metadata for {}", args.file.display()))?;
    let total_size = metadata.len();

    if total_size == 0 {
        bail!("File is empty: {}", args.file.display());
    }

    let file = tokio::fs::File::open(&args.file)
        .await
        .with_context(|| format!("Failed to open {}", args.file.display()))?;

    // Progress bar setup
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA {eta})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );
    pb.enable_steady_tick(Duration::from_millis(150));

    // Wrap the file in a stream that reports bytes read as it's uploaded.
    let uploaded = Arc::new(AtomicU64::new(0));
    let uploaded_clone = uploaded.clone();
    let pb_clone = pb.clone();

    let stream = ReaderStream::new(file).map(move |chunk| {
        if let Ok(ref bytes) = chunk {
            let prev = uploaded_clone.fetch_add(bytes.len() as u64, Ordering::Relaxed);
            pb_clone.set_position(prev + bytes.len() as u64);
        }
        chunk
    });

    let body = Body::wrap_stream(stream);

    let content_type = args.content_type.clone().unwrap_or_else(|| {
        mime_guess::from_path(&args.file)
            .first_or_octet_stream()
            .to_string()
    });

    let file_name = args
        .file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "upload.bin".to_string());

    let client = {
        let mut builder = reqwest::Client::builder();
        if args.timeout > 0 {
            builder = builder.timeout(Duration::from_secs(args.timeout));
        }
        builder.build().context("Failed to build HTTP client")?
    };

    let mut req = client
        .post(&args.url)
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .header(reqwest::header::CONTENT_LENGTH, total_size)
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", file_name),
        )
        .body(body);

    for h in &args.headers {
        let Some((k, v)) = h.split_once(':') else {
            pb.finish_and_clear();
            bail!("Invalid header '{}', expected 'Key: Value'", h);
        };
        req = req.header(k.trim(), v.trim());
    }

    let result = req.send().await;

    // Make sure the bar reflects 100% even if the last chunk callback raced the response.
    pb.set_position(total_size);

    let resp = match result {
        Ok(r) => r,
        Err(e) => {
            pb.finish_and_clear();
            return Err(e).context("Upload request failed");
        }
    };

    let status = resp.status();
    pb.finish_with_message(if status.is_success() {
        "upload complete"
    } else {
        "upload finished with error status"
    });

    let body_text = resp.text().await.unwrap_or_default();

    println!("Status: {}", status);
    if !body_text.trim().is_empty() {
        println!("Response body:\n{}", body_text);
    }

    if !status.is_success() {
        bail!("Server returned non-success status: {}", status);
    }

    Ok(())
}
