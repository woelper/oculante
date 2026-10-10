use super::*;
use crate::appstate::OculanteState;
use egui::{Context, FontData, FontDefinitions};
use epaint::FontFamily;
use font_kit::{
    family_name::FamilyName, handle::Handle, properties::Properties, source::SystemSource,
};
use log::warn;
use std::{
    fs::read,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub fn apply_theme(state: &mut OculanteState, ctx: &Context) {
    let mut button_color = Color32::from_hex("#262626").unwrap_or_default();
    let mut panel_color = Color32::from_gray(25);

    match state.persistent_settings.theme {
        ColorTheme::Light => ctx.set_visuals(Visuals::light()),
        ColorTheme::Dark => ctx.set_visuals(Visuals::dark()),
        ColorTheme::System => set_system_theme(ctx),
    }

    // Switching theme resets accent color, set it again
    let mut style: egui::Style = (*ctx.global_style()).clone();
    style.spacing.scroll = egui::style::ScrollStyle::solid();

    if style.visuals.dark_mode {
        // Text color for label
        style.visuals.widgets.noninteractive.fg_stroke.color =
            Color32::from_hex("#CCCCCC").unwrap_or_default();
        // Text color for buttons
        style.visuals.widgets.inactive.fg_stroke.color =
            Color32::from_hex("#CCCCCC").unwrap_or_default();
        style.visuals.extreme_bg_color = Color32::from_hex("#0D0D0D").unwrap_or_default();
        if !state.persistent_settings.background_color_is_custom
            && state.persistent_settings.background_color == [200, 200, 200]
        {
            state.persistent_settings.background_color =
                PersistentSettings::default().background_color;
        }
        if !state.persistent_settings.accent_color_is_custom
            && state.persistent_settings.accent_color == [0, 170, 255]
        {
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
        if !state.persistent_settings.background_color_is_custom
            && state.persistent_settings.background_color
                == PersistentSettings::default().background_color
        {
            state.persistent_settings.background_color = [200, 200, 200];
        }
        if !state.persistent_settings.accent_color_is_custom
            && state.persistent_settings.accent_color == PersistentSettings::default().accent_color
        {
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
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(4);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(4);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(4);

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
    ctx.set_global_style(style);
}

/// Attempt to load a system font by any of the given `family_names`, returning the first match.
/// Set once text shows up that the built-in fonts have no glyphs for
static SYSTEM_FONTS_WANTED: AtomicBool = AtomicBool::new(false);

/// Whether text contains characters that only the system fonts can show:
/// Chinese, Japanese, Korean, Arabic and other scripts beyond Latin, Greek and Cyrillic.
pub fn needs_system_fonts(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c as u32,
            0x0590..=0x1CFF | 0x2E80..=0xD7FF | 0xF900..=0xFFEF | 0x20000..
        )
    })
}

/// Ask for the system fonts if this text needs them. They take time and memory to
/// load, so that only happens once such text is about to be shown.
pub fn want_system_fonts_for(text: &str) {
    if !SYSTEM_FONTS_WANTED.load(Ordering::Relaxed) && needs_system_fonts(text) {
        debug!("System fonts are needed for {text:?}");
        SYSTEM_FONTS_WANTED.store(true, Ordering::Relaxed);
        // they are picked up when the next frame is drawn
        crate::utils::request_repaint();
    }
}

/// Text is being composed with an input method, which is what these scripts are typed with
pub fn want_system_fonts_for_ime() {
    SYSTEM_FONTS_WANTED.store(true, Ordering::Relaxed);
}

pub fn system_fonts_wanted() -> bool {
    SYSTEM_FONTS_WANTED.load(Ordering::Relaxed)
}

/// People who use a language written in these scripts get the fonts right away
pub fn want_system_fonts_for_locale() {
    let language = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|var| std::env::var(var).ok().filter(|value| !value.is_empty()))
        .unwrap_or_default();
    if ["zh", "ja", "ko", "ar", "fa", "ur"]
        .iter()
        .any(|prefix| language.starts_with(prefix))
    {
        debug!("System fonts are needed for the language {language}");
        SYSTEM_FONTS_WANTED.store(true, Ordering::Relaxed);
    }
}

enum FontLookup {
    Loaded(Vec<u8>),
    /// The font is in a file that was loaded for another region
    AlreadyLoaded,
    Missing,
}

fn load_font_family(family_names: &[&str], loaded_files: &mut Vec<PathBuf>) -> FontLookup {
    let system_source = SystemSource::new();
    for &name in family_names {
        let font_handle = system_source
            .select_best_match(&[FamilyName::Title(name.to_string())], &Properties::new());
        match font_handle {
            Ok(h) => match &h {
                Handle::Memory { bytes, .. } => {
                    info!("Loaded {name} from memory.");
                    return FontLookup::Loaded(bytes.to_vec());
                }
                Handle::Path { path, .. } => {
                    if loaded_files.contains(path) {
                        debug!("{name} is in {path:?}, which is loaded already");
                        return FontLookup::AlreadyLoaded;
                    }
                    info!("Loaded {name} from path: {:?}", path);
                    if let Ok(data) = read(path) {
                        loaded_files.push(path.clone());
                        return FontLookup::Loaded(data);
                    }
                }
            },
            Err(e) => debug!("Could not load {}: {:?}", name, e),
        }
    }
    FontLookup::Missing
}

pub fn load_system_fonts(mut fonts: FontDefinitions) -> FontDefinitions {
    debug!("Attempting to load sys fonts");
    // In this order, the first font that has a glyph is used
    let fontdb = [
        (
            "simplified_chinese",
            vec![
                "Heiti SC",
                "Songti SC",
                "Noto Sans CJK SC", // Good coverage for Simplified Chinese
                "Noto Sans SC",
                "WenQuanYi Zen Hei", // INcludes both Simplified and Traditional Chinese.
                "SimSun",
                "Noto Sans SC",
                "PingFang SC",
                "Source Han Sans CN",
            ],
        ),
        ("traditional_chinese", vec!["Source Han Sans HK"]),
        (
            "japanese",
            vec![
                "Noto Sans JP",
                "Noto Sans CJK JP",
                "Source Han Sans JP",
                "MS Gothic",
            ],
        ),
        ("korean", vec!["Source Han Sans KR"]),
        ("taiwanese", vec!["Source Han Sans TW"]),
        (
            "arabic_fonts",
            vec![
                "Noto Sans Arabic",
                "Amiri",
                "Lateef",
                "Al Tarikh",
                "Segoe UI",
            ],
        ),
    ];

    // Several regions can end up with the same font file, the fonts for Chinese and
    // Japanese often are one. It is only loaded once.
    let mut loaded_files = Vec::new();
    for (region, font_names) in fontdb {
        match load_font_family(&font_names, &mut loaded_files) {
            FontLookup::Loaded(font_data) => {
                info!("Inserting font {region}");
                fonts
                    .font_data
                    .insert(region.to_owned(), Arc::new(FontData::from_owned(font_data)));

                fonts
                    .families
                    .get_mut(&FontFamily::Proportional)
                    .unwrap()
                    .push(region.to_owned());
            }
            FontLookup::AlreadyLoaded => {}
            FontLookup::Missing => warn!(
                "Could not load a font for region {region}. If you experience incorrect file names, try installing one of these fonts: [{}]",
                font_names.join(", ")
            ),
        }
    }
    fonts
}

#[cfg(test)]
mod tests {
    use super::needs_system_fonts;

    #[test]
    fn built_in_fonts_cover_western_text() {
        for text in [
            "/home/me/Pictures/holiday 2024 (1).jpg",
            "C:\\Users\\Zoë\\Übersicht – größer.png",
            "Ελληνικά and Кириллица",
            "arrows → and symbols ©",
        ] {
            assert!(!needs_system_fonts(text), "{text}");
        }
    }

    #[test]
    fn other_scripts_need_system_fonts() {
        for text in [
            "写真.png",
            "/home/me/こんにちは/a.jpg",
            "한국어.jpg",
            "صورة.png",
            "ＦＵＬＬ.png",
        ] {
            assert!(needs_system_fonts(text), "{text}");
        }
    }
}
