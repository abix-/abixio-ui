use clap::Parser;

#[derive(Parser)]
#[command(name = "abixio-ui", about = "native desktop s3 manager")]
struct Args {
    #[arg(long, default_value = "http://localhost:9000")]
    endpoint: String,
}

fn main() -> iced::Result {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let endpoint = args.endpoint;

    iced::application(
        move || abixio_ui::app::App::new(endpoint.clone()),
        abixio_ui::app::App::update,
        abixio_ui::app::App::view,
    )
    .theme(abixio_ui::app::App::theme)
    .title(abixio_ui::app::App::title)
    .subscription(abixio_ui::app::App::subscription)
    .window_size((1024.0, 768.0))
    .run()
}
