mod app;
mod collectors;
mod state;
mod ui;

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "nexnwatch=info".to_string()),
        )
        .init();

    app::run()
}
