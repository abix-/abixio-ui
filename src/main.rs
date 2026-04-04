use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "abixio-ui", about = "native desktop s3 manager")]
struct Args {
    #[arg(long)]
    endpoint: Option<String>,

    #[arg(long)]
    access_key: Option<String>,

    #[arg(long)]
    secret_key: Option<String>,

    #[arg(long)]
    run_tests: bool,

    #[arg(long)]
    test_report_file: Option<PathBuf>,
}

fn main() -> iced::Result {
    let args = Args::parse();

    if !args.run_tests {
        tracing_subscriber::fmt::init();
    }

    let endpoint = args.endpoint;
    let creds = match (args.access_key, args.secret_key) {
        (Some(ak), Some(sk)) => Some((ak, sk)),
        _ => None,
    };
    let auto_run_tests = args.run_tests;
    if auto_run_tests && endpoint.is_none() {
        eprintln!("--run-tests requires --endpoint");
        std::process::exit(2);
    }
    let test_report_path = if auto_run_tests {
        Some(
            args.test_report_file
                .unwrap_or_else(default_test_report_path),
        )
    } else {
        None
    };
    let startup = abixio_ui::app::StartupOptions {
        endpoint,
        creds,
        auto_run_tests,
        test_report_path,
    };

    iced::application(
        move || abixio_ui::app::App::new(startup.clone()),
        abixio_ui::app::App::update,
        abixio_ui::app::App::view,
    )
    .theme(abixio_ui::app::App::theme)
    .title(abixio_ui::app::App::title)
    .subscription(abixio_ui::app::App::subscription)
    .window_size((1024.0, 768.0))
    .run()
}

fn default_test_report_path() -> PathBuf {
    let base = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".abixio-ui")
        .join("test-reports");
    let timestamp = time::OffsetDateTime::now_utc().unix_timestamp();
    base.join(format!("abixio-ui-test-results-{}.json", timestamp))
}
