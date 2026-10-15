//! Entry point: read the configuration and start the application.

#[tokio::main]
async fn main() {
    let config = bagerista::Config::from_env();
    if let Err(err) = bagerista::run(config).await {
        eprintln!("Грешка при стартиране: {err}");
        std::process::exit(1);
    }
}
