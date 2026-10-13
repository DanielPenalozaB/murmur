//! Asks a question about (or writes notes for) a recorded session, like the app does.
//!
//!   cargo run -p murmur-llm --example ask -- <transcript.jsonl> <ollama model> "question"
//!   cargo run -p murmur-llm --example ask -- <transcript.jsonl> <ollama model> --notes
//!
//! MURMUR_WARM=1 preloads the model first, as the app does when recording starts.
//! For Claude or Gemini, set MURMUR_PROVIDER=anthropic|gemini and MURMUR_API_KEY.

use std::io::Write;
use std::time::Instant;

use murmur_core::Segment;
use murmur_llm::{Provider, ask_prompt, notes_prompt, stream, warm_up};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [path, model, question] = args.as_slice() else {
        return Err("usage: ask <transcript.jsonl> <model> <question|--notes>".into());
    };
    let segments: Vec<Segment> = std::fs::read_to_string(path)?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    let provider = match std::env::var("MURMUR_PROVIDER").as_deref() {
        Ok("anthropic") => Provider::Anthropic { model: model.clone() },
        Ok("gemini") => Provider::Gemini { model: model.clone() },
        _ => match Provider::default() {
            Provider::Ollama {
                base_url, num_ctx, ..
            } => Provider::Ollama {
                base_url,
                model: model.clone(),
                num_ctx,
            },
            other => other,
        },
    };
    let key = std::env::var("MURMUR_API_KEY").ok();
    let prompt = if question == "--notes" {
        notes_prompt(&segments, "")
    } else {
        ask_prompt(&segments, question)
    };

    if std::env::var_os("MURMUR_WARM").is_some() {
        let t = Instant::now();
        warm_up(&provider).await?;
        println!("(model loaded in {:?})", t.elapsed());
    }
    let started = Instant::now();
    let mut first = None;
    stream(&provider, key.as_deref(), &prompt, |text| {
        first.get_or_insert_with(|| started.elapsed());
        print!("{text}");
        std::io::stdout().flush().ok();
    })
    .await?;
    println!(
        "\n\n(first words after {:?}, done in {:?})",
        first.unwrap_or_default(),
        started.elapsed()
    );
    Ok(())
}
