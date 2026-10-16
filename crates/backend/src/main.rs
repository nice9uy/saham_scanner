use backend::config::Config;
use tracing_subscriber::EnvFilter;

const DEFAULT_LOG: &str = "info,sqlx=warn,tower_http=warn";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = std::env::var("RUST_LOG").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| DEFAULT_LOG.into());
    tracing_subscriber::fmt().with_env_filter(EnvFilter::try_new(filter).unwrap_or_else(|_| DEFAULT_LOG.into())).init();
    backend::run(Config::from_env()?).await
}
