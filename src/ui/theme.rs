use super::*;
use crate::appstate::OculanteState;

pub fn apply_theme(state: &mut OculanteState, ctx: &Context) {
    let mut button_color = Color32::from_hex("#262626").unwrap_or_default();
    let mut panel_color = Color32::from_gray(25);

    match state.persistent_settings.theme {
        ColorTheme::Light => ctx.set_visuals(Visuals::light()),
        ColorTheme::Dark => ctx.set_visuals(Visuals::dark()),
        ColorTheme::System => set_system_theme(ctx),
    }

    // Switching theme resets accent color, set it again
    let mut style: egui::Style = (*ctx.style()).clone();
    style.spacing.scroll = egui::style::ScrollStyle::solid();

    if style.visuals.dark_mode {
        // Text color for label
        style.visuals.widgets.noninteractive.fg_stroke.color =
            Color32::from_hex("#CCCCCC").unwrap_or_default();
        // Text color for buttons
        style.visuals.widgets.inactive.fg_stroke.color =
            Color32::from_hex("#CCCCCC").unwrap_or_default();
        style.visuals.extreme_bg_color = Color32::from_hex("#0D0D0D").unwrap_or_default();
        if state.persistent_settings.background_color == [200, 200, 200] {
            state.persistent_settings.background_color =
                PersistentSettings::default().background_color;
        }
        if state.persistent_settings.accent_color == [0, 170, 255] {
            state.persistent_settings.accent_color = PersistentSettings::default().accent_color;
        }
    } else {
        style.visuals.extreme_bg_color = Color32::from_hex("#D9D9D9").unwrap_or_default();
        // Text color for label
        style.visuals.widgets.noninteractive.fg_stroke.color =
            Color32::from_hex("#333333").unwrap_or_default();
        // Text color for buttons
        style.visuals.widgets.inactive.fg_stroke.color =
            Color32::from_hex("#333333").unwrap_or_default();

        button_color = Color32::from_gray(255);
        panel_color = Color32::from_gray(230);
        if state.persistent_settings.background_color
            == PersistentSettings::default().background_color
        {
            state.persistent_settings.background_color = [200, 200, 200];
        }
        if state.persistent_settings.accent_color == PersistentSettings::default().accent_color {
            state.persistent_settings.accent_color = [0, 170, 255];
        }
        style.visuals.widgets.inactive.bg_fill = Color32::WHITE;
        style.visuals.widgets.hovered.bg_fill = Color32::WHITE.gamma_multiply(0.8);
    }
    style.interaction.tooltip_delay = 0.0;
    style.spacing.icon_width = 20.;
    style.spacing.window_margin = 5.0.into();
    style.spacing.item_spacing = vec2(8., 6.);
    style.spacing.icon_width_inner = style.spacing.icon_width / 1.5;
    style.spacing.interact_size.y = BUTTON_HEIGHT_SMALL;
    style.visuals.window_fill = panel_color;

    // button color
    style.visuals.widgets.inactive.weak_bg_fill = button_color;
    // style.visuals.widgets.inactive.bg_fill = button_color;
    // style.visuals.widgets.inactive.bg_fill = button_color;

    // button rounding
    style.visuals.widgets.inactive.rounding = Rounding::same(4.);
    style.visuals.widgets.active.rounding = Rounding::same(4.);
    style.visuals.widgets.hovered.rounding = Rounding::same(4.);

    // No stroke on buttons
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;

    style.visuals.warn_fg_color = Color32::from_rgb(255, 204, 0);

    style.visuals.panel_fill = panel_color;

    style.text_styles.get_mut(&TextStyle::Body).unwrap().size = 15.;
    style.text_styles.get_mut(&TextStyle::Button).unwrap().size = 15.;
    style.text_styles.get_mut(&TextStyle::Small).unwrap().size = 12.;
    style.text_styles.get_mut(&TextStyle::Heading).unwrap().size = 18.;
    // accent color
    style.visuals.selection.bg_fill = Color32::from_rgb(
        state.persistent_settings.accent_color[0],
        state.persistent_settings.accent_color[1],
        state.persistent_settings.accent_color[2],
    );

    let accent_color = style.visuals.selection.bg_fill.to_array();

    let accent_color_luma = (accent_color[0] as f32 * 0.299
        + accent_color[1] as f32 * 0.587
        + accent_color[2] as f32 * 0.114)
        .clamp(0., 255.) as u8;
    let accent_color_luma = if accent_color_luma < 80 { 220 } else { 80 };
    // Set text on highlighted elements
    style.visuals.selection.stroke = Stroke::new(2.0, Color32::from_gray(accent_color_luma));
    ctx.set_style(style);
}

// use std::fs::read;

// use egui::{Context, FontData, FontDefinitions};

// use font_kit::{
//     family_name::FamilyName, handle::Handle, properties::Properties, source::SystemSource,
// };

// pub fn load_system_font(ctx: &Context) {
//     let mut fonts = FontDefinitions::default();

//     font_kit::source::Source::

//     let handle = SystemSource::new()
//         .select_best_match(&[FamilyName::], &Properties::new())
//         .unwrap();

//     let buf: Vec<u8> = match handle {
//         Handle::Memory { bytes, .. } => bytes.to_vec(),
//         Handle::Path { path, .. } => read(path).unwrap(),
//     };

//     const FONT_SYSTEM_SANS_SERIF: &'static str = "System Sans Serif";

//     fonts
//         .font_data
//         .insert(FONT_SYSTEM_SANS_SERIF.to_owned(), FontData::from_owned(buf));

//     if let Some(vec) = fonts.families.get_mut(&FontFamily::Proportional) {
//         vec.push(FONT_SYSTEM_SANS_SERIF.to_owned());
//     }

//     if let Some(vec) = fonts.families.get_mut(&FontFamily::Monospace) {
//         vec.push(FONT_SYSTEM_SANS_SERIF.to_owned());
//     }

//     ctx.set_fonts(fonts);
// }


use std::fs::read;

use egui::{Context, FontData, FontDefinitions};
use epaint::FontFamily;
use font_kit::{
    family_name::{FamilyName},
    handle::Handle,
    properties::Properties,
    source::SystemSource,
};

/// Attempt to load a system font by any of the given `family_names`, returning the first match.
fn load_font_family(family_names: &[&str]) -> Option<Vec<u8>> {
    let system_source = SystemSource::new();

    for &name in family_names {
        // FamilyName::Title will match an explicit font-family name like "Noto Sans Arabic"
        let family = FamilyName::Title(name.to_string());
        if let Ok(font_handle) = system_source.select_best_match(&[family], &Properties::new()) {
            match font_handle {
                Handle::Memory { bytes, .. } => {
                    return Some(bytes.to_vec());
                }
                Handle::Path { path, .. } => {
                    if let Ok(data) = read(path) {
                        return Some(data);
                    }
                }
            }
        }
    }

    None
}

pub fn load_system_fonts(ctx: &Context) {
    info!("Attempting to load sys fonts");
    // Start from the default fonts
    let mut fonts = FontDefinitions::default();

 
    if let Some(cjk_data) = load_font_family(&[
        "Noto Sans CJK SC",   // Good coverage for Simplified Chinese
        "Noto Sans SC",
        "Noto Sans CJK JP",   // Japanese
        "Noto Sans JP",
        "WenQuanYi Zen Hei",  // Another common Linux CJK font
        "SimSun",             // Common on Windows for Simplified Chinese
        "MS Gothic",          // Another Windows fallback for Japanese
        "Songti SC"
    ]) {

        info!("Inserting font !");
        // Insert it into font_data under a name we control:
        fonts.font_data.insert(
            "my_cjk_font".to_owned(),
            FontData::from_owned(cjk_data),
        );

        // Push to fallback for both proportional and monospace
        if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
            list.push("my_cjk_font".to_owned());
        }
    }

    //
    // 2) Load a font that supports Arabic
    //
    // Try from a set of known Arabic-capable fonts
    //
    if let Some(arabic_data) = load_font_family(&[
        "Noto Sans Arabic",
        "Amiri",
        "Lateef",
        "Scheherazade",
        // Add more if needed
    ]) {
        fonts.font_data.insert(
            "my_arabic_font".to_owned(),
            FontData::from_owned(arabic_data),
        );

        if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
            list.push("my_arabic_font".to_owned());
        }
    }

    //
    // 3) (Optional) Load additional fonts for more languages/scripts as needed.
    //

    // Finally, tell egui to use our new font definitions:
    ctx.set_fonts(fonts);
}
