//! The light engineering theme of HydroBEM GUI and its fonts (those of Windows).

use std::sync::Arc;

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};

pub const WINDOW: Color32 = Color32::from_rgb(0xF3, 0xF5, 0xF7);
pub const TOP_BAR: Color32 = Color32::from_rgb(0xF8, 0xF9, 0xFA);
pub const SPLITTER: Color32 = Color32::from_rgb(0xD7, 0xDC, 0xE2);
pub const VIEWPORT: Color32 = Color32::from_rgb(0xEE, 0xF3, 0xF8);
pub const HEADER: Color32 = Color32::from_rgb(0xC9, 0xDF, 0xF1);
pub const HEADER_LINE: Color32 = Color32::from_rgb(0xAA, 0xB6, 0xC1);
pub const SECTION: Color32 = Color32::from_rgb(0xDC, 0xE3, 0xEA);
pub const GRID_LINE: Color32 = Color32::from_rgb(0xC8, 0xCF, 0xD6);
pub const SELECT: Color32 = Color32::from_rgb(0x16, 0x77, 0xC8);
pub const HOVER: Color32 = Color32::from_rgb(0xE5, 0xF1, 0xFB);
pub const TEXT: Color32 = Color32::from_rgb(0x20, 0x24, 0x28);
pub const READ_ONLY: Color32 = Color32::from_rgb(0x70, 0x76, 0x7C);
pub const ARROW: Color32 = Color32::from_rgb(0x50, 0x58, 0x60);
pub const BUILD_OK: Color32 = Color32::from_rgb(0x1F, 0x8A, 0x4C);
pub const ERROR: Color32 = Color32::from_rgb(0xC0, 0x30, 0x30);
/// A caution, not an error: results of inputs changed since the Run.
pub const CAUTION: Color32 = Color32::from_rgb(0xB0, 0x60, 0x00);
pub const RUN: Color32 = Color32::from_rgb(0x2F, 0xA4, 0x4F);
pub const RUN_HOVER: Color32 = Color32::from_rgb(0x36, 0xB9, 0x5A);
pub const STOP: Color32 = Color32::from_rgb(0xC8, 0x43, 0x3A);
pub const TABLE_HEADER: Color32 = Color32::from_rgb(0xF3, 0xF5, 0xF7);
pub const TABLE_SELECT: Color32 = Color32::from_rgb(0xCC, 0xE4, 0xF7);
/// A point picked on a graph.
pub const PICKED: Color32 = Color32::from_rgb(0xE0, 0x6C, 0x1A);

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
    ctx.set_theme(egui::Theme::Light);
    ctx.style_mut_of(egui::Theme::Light, |style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, FontId::proportional(13.5)),
            (TextStyle::Heading, FontId::proportional(15.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(10.0, 3.0);
        style.spacing.interact_size.y = 22.0;
        style.spacing.menu_margin = egui::Margin::same(4);

        let v = &mut style.visuals;
        v.panel_fill = Color32::WHITE;
        v.window_fill = Color32::WHITE;
        v.extreme_bg_color = Color32::WHITE;
        v.faint_bg_color = Color32::from_rgb(0xF6, 0xF8, 0xFA);
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = SELECT;
        v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        v.window_corner_radius = CornerRadius::same(3);
        v.menu_corner_radius = CornerRadius::same(2);
        let radius = CornerRadius::same(2);
        for (w, fill, line) in [
            (&mut v.widgets.inactive, Color32::from_rgb(0xFD, 0xFD, 0xFD), Color32::from_rgb(0xAD, 0xB2, 0xB8)),
            (&mut v.widgets.hovered, HOVER, Color32::from_rgb(0x00, 0x78, 0xD7)),
            (&mut v.widgets.active, Color32::from_rgb(0xCC, 0xE4, 0xF7), Color32::from_rgb(0x00, 0x55, 0x9B)),
            (&mut v.widgets.open, Color32::from_rgb(0xCC, 0xE4, 0xF7), Color32::from_rgb(0x00, 0x78, 0xD7)),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = Stroke::new(1.0, line);
            w.corner_radius = radius;
            w.expansion = 0.0;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, GRID_LINE);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    });
}

/// The fonts of the window, read from Windows when it opens: the executable carries no font of its
/// own (egui is built without its bundled fonts). Each family takes its first font and then the
/// fallbacks for the characters it lacks: Segoe UI Symbol (arrows, shapes), Segoe UI Emoji and
/// Malgun Gothic (Hangul, so Korean file names show).
const TEXT_FONTS: [&str; 1] = ["segoeui.ttf"];
const MONO_FONTS: [&str; 1] = ["consola.ttf"];
const FALLBACK_FONTS: [&str; 3] = ["seguisym.ttf", "seguiemj.ttf", "malgun.ttf"];

/// The folders searched for a font: the system's, then the user's own fonts.
fn font_folders() -> Vec<std::path::PathBuf> {
    let mut folders = Vec::new();
    if let Some(windows) = std::env::var_os("WINDIR") {
        folders.push(std::path::Path::new(&windows).join("Fonts"));
    }
    folders.push("C:/Windows/Fonts".into());
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        folders.push(std::path::Path::new(&local).join("Microsoft/Windows/Fonts"));
    }
    folders
}

fn read_font(file: &str) -> Option<Vec<u8>> {
    font_folders().iter().find_map(|folder| std::fs::read(folder.join(file)).ok())
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::empty();
    let mut add = |file: &str| -> Option<String> {
        let bytes = read_font(file)?;
        fonts.font_data.insert(file.into(), Arc::new(FontData::from_owned(bytes)));
        Some(file.to_string())
    };
    let text: Vec<String> = TEXT_FONTS.iter().filter_map(|f| add(f)).take(1).collect();
    let mono: Vec<String> = MONO_FONTS.iter().filter_map(|f| add(f)).take(1).collect();
    let fallbacks: Vec<String> = FALLBACK_FONTS.iter().filter_map(|f| add(f)).collect();
    // a family without its own font takes the other's, then the fallbacks
    let proportional: Vec<String> = text.iter().chain(&mono).take(1).chain(&fallbacks).cloned().collect();
    let monospace: Vec<String> = mono.iter().chain(&text).take(1).chain(&fallbacks).cloned().collect();
    if proportional.is_empty() {
        // no font at all: egui cannot draw text; keep its (empty) definitions rather than panic
        eprintln!("HydroBEM: none of the Windows fonts {TEXT_FONTS:?} {MONO_FONTS:?} {FALLBACK_FONTS:?} was found");
        return;
    }
    fonts.families.insert(FontFamily::Proportional, proportional);
    fonts.families.insert(FontFamily::Monospace, monospace);
    ctx.set_fonts(fonts);
}
