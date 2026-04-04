#![allow(dead_code)]

mod app;
mod async_op;
pub mod perf;
mod s3;
mod views;

use clap::Parser;
use eframe::egui;

#[derive(Parser)]
#[command(name = "abixio-ui", about = "native desktop s3 manager")]
struct Args {
    #[arg(long, default_value = "http://localhost:9000")]
    endpoint: String,
}

fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("abixio-ui")
            .with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };

    eframe::run_native(
        "abixio-ui",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, &args.endpoint)))),
    )
    .unwrap();
}
