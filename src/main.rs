// No console window for the GUI build on Windows (CLI subcommands still print when started from a terminal).
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod app;
mod cli;
mod map_dupes;
mod optimize;
mod panels;
mod ui;
mod wizard;

use anyhow::Result;
use clap::Parser;
use draxbyte::config::{default_config_path, Config};

impl eframe::App for app::App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_jobs();
        self.launch_update(ui.ctx());
        ui::draw(self, ui);
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    // Setup / uninstall wizard: this same program, started as `Draxbyte-Setup.exe`, with `--setup`
    // or `--uninstall`, or as Draxbyte.app opened from the disk image. Decided before the CLI parses
    // its arguments (those flags are not CLI subcommands).
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(mode) = wizard::detect_mode(&raw, &exe) {
            let lang = Config::load(&default_config_path()).map(|c| c.ui.language).unwrap_or_else(|_| "auto".into());
            draxbyte::i18n::set(draxbyte::i18n::resolve(&lang));
            return wizard::run(mode, &raw);
        }
    }
    let args = cli::Cli::parse();
    let cfg_path = args.config.clone().unwrap_or_else(default_config_path);
    let cfg = Config::load(&cfg_path)?;
    draxbyte::i18n::set(draxbyte::i18n::resolve(&cfg.ui.language));
    if args.cmd.is_some() {
        return cli::run(args, cfg);
    }

    // Windows: ask for administrator rights at start so nothing is out of reach (the user's
    // choice; Settings can turn it off). Declining the UAC prompt just starts the app as usual.
    // DRAXBYTE_NO_ELEVATE=1 skips it (automated screenshots and tests).
    if cfg!(windows)
        && cfg.ui.always_admin
        && std::env::var_os("DRAXBYTE_NO_ELEVATE").is_none()
        && !draxbyte::core::elevate::is_elevated()
        && draxbyte::core::elevate::relaunch_elevated().is_ok()
    {
        return Ok(());
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
    .map_err(|e| {
        anyhow::anyhow!(draxbyte::tf!(
            "GUI başlatılamadı: {e}",
            "could not start the GUI: {e}"
        ))
    })
}
