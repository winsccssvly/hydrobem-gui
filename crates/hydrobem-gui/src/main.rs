//! HydroBEM.exe: the graphical interface of the HydroBEM engine (egui + wgpu).
//!
//! `HydroBEM.exe [file.hbm | file.hbr]` opens a model or a results file; `HydroBEM.exe --untitled
//! file.hbm` opens a model as a new unsaved project (`hydrobem_gui.show` of a model).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use hydrobem_gui::app::App;

fn fail(message: &str) -> ! {
    rfd::MessageDialog::new().set_level(rfd::MessageLevel::Error).set_title("HydroBEM").set_description(message).show();
    std::process::exit(1);
}

fn main() -> eframe::Result {
    // The engine is compiled for AVX2 and FMA; without them it would fault on the first solve.
    if !std::arch::is_x86_feature_detected!("avx2") || !std::arch::is_x86_feature_detected!("fma") {
        fail("HydroBEM needs a processor with AVX2 and FMA (most x86-64 processors since 2013-2015).");
    }
    // The engine is hydrobem.dll beside HydroBEM.exe, of the C ABI this build was made for.
    if let Err(message) = gui_core::hydrobem_engine::load() {
        fail(&format!("{message}\n\nInstall hydrobem.dll of this HydroBEM release beside HydroBEM.exe."));
    }
    // `--untitled` opens the file as a new unsaved project (a model handed over from Python)
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let untitled = args.first().is_some_and(|a| a == "--untitled");
    if untitled {
        args.remove(0);
    }
    let file = args.into_iter().next().map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("HydroBEM")
            .with_drag_and_drop(true),
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native(
        "HydroBEM",
        options,
        Box::new(move |cc| {
            let mut app = App::new(cc);
            if let Some(file) = file {
                if untitled {
                    app.open_untitled(&file);
                } else if file.extension().is_some_and(|e| e.eq_ignore_ascii_case("hbr")) {
                    app.open_results_file(&file);
                } else {
                    app.open_project(&file);
                }
            }
            Ok(Box::new(app))
        }),
    )
}
