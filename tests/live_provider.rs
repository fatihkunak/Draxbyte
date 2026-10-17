//! Opt-in check against a real OpenAI-compatible server (e.g. a local llama-server / Ollama):
//! `DRAXBYTE_LIVE_URL=http://localhost:8080/v1 cargo test --test live_provider -- --ignored --nocapture`

use draxbyte::ai::providers::{check, list_models, resolve};
use draxbyte::config::AiConfig;

#[tokio::test]
#[ignore = "needs a running server"]
async fn live_models_and_chat() {
    let url = std::env::var("DRAXBYTE_LIVE_URL").expect("set DRAXBYTE_LIVE_URL");
    let mut cfg = AiConfig {
        enabled: true,
        provider: "custom".into(),
        base_url: url,
        ..AiConfig::default()
    };
    cfg.api_keys.insert("custom".into(), "not-needed".into());

    let creds = resolve(&cfg).unwrap();
    let models = list_models(&creds).await.unwrap();
    println!("models: {models:?}");
    assert!(!models.is_empty());

    let creds = resolve(&AiConfig {
        model: models[0].clone(),
        ..cfg
    })
    .unwrap();
    let (ms, tokens) = check(&creds).await.unwrap();
    println!("chat ok: {ms} ms, {tokens} tokens");
}
