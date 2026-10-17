// No console window for the GUI build on Windows (CLI subcommands still print when started from a terminal).
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod app;
mod cli;
mod ui;

use anyhow::Result;
use clap::Parser;
use draxbyte::config::{default_config_path, Config};

impl eframe::App for app::App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_jobs();
        ui::draw(self, ui);
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let args = cli::Cli::parse();
    let cfg_path = args.config.clone().unwrap_or_else(default_config_path);
    let cfg = Config::load(&cfg_path)?;
    if args.cmd.is_some() {
        return cli::run(args, cfg);
    }

    let icon = egui::IconData {
        rgba: ui::ICON_RGBA.to_vec(),
        width: 256,
        height: 256,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("Draxbyte")
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "Draxbyte",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, cfg, cfg_path)))),
    )
    .map_err(|e| anyhow::anyhow!("GUI başlatılamadı: {e}"))
}
