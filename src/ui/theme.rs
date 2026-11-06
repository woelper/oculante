use super::*;
use crate::appstate::OculanteState;
use egui::{Context, FontData, FontDefinitions};
use epaint::FontFamily;
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

    // egui follows the system theme on its own and keeps a style for each, the one not set
    // here stays egui's: Dark picked on a light system turned teal when the system went dark
    match state.persistent_settings.theme {
        ColorTheme::Light => {
            ctx.set_theme(egui::Theme::Light);
            ctx.set_visuals(Visuals::light())
        }
        ColorTheme::Dark => {
            ctx.set_theme(egui::Theme::Dark);
            ctx.set_visuals(Visuals::dark())
        }
        ColorTheme::System => {
            ctx.set_theme(egui::ThemePreference::System);
            set_system_theme(ctx)
        }
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
    let [r, g, b] = state.persistent_settings.accent_color;
    let accent = Color32::from_rgb(r, g, b);
    ctx.data_mut(|data| data.insert_temp(accent_id(), accent));

    // The cursor where text is typed, egui draws its own light blue
    style.visuals.text_cursor.stroke.color = accent;

    // What is selected in menus, lists and text, the outline of a text field being typed in
    let (background, text) = selected_colors(accent, style.visuals.dark_mode);
    style.visuals.selection.bg_fill = background;
    style.visuals.selection.stroke = Stroke::new(2.0, text);
    ctx.set_global_style(style);
}

fn accent_id() -> egui::Id {
    egui::Id::new("accent color")
}

/// The accent color picked in the settings. egui's `selection` holds the colors of what is
/// selected, which are made from it. It reads the context, so it waits forever when called
/// in a closure that holds it, such as one given to `fonts_mut`.
pub fn accent_color(ctx: &Context) -> Color32 {
    ctx.data(|data| data.get_temp(accent_id()))
        .unwrap_or_else(|| ctx.global_style().visuals.selection.stroke.color)
}

/// What is selected, and a button that is on: a dark shade of the accent behind and the
/// accent as text, after Stoppedpuma's design (#3D0A19 behind our red), a light shade in the
/// light theme. Gray text on the accent was hard to read. Where the accent would not read
/// well on its shade, the text gets lighter or darker.
pub fn selected_colors(accent: Color32, dark_mode: bool) -> (Color32, Color32) {
    let (hue, saturation, lightness) = to_hsl(accent);
    let background = from_hsl(hue, saturation * 0.72, if dark_mode { 0.14 } else { 0.86 });
    let step = if dark_mode { 0.02 } else { -0.02 };
    let (mut text, mut text_lightness) = (accent, lightness);
    // a little below the 4.5 of WCAG: the red of the design has 4.3
    while contrast(text, background) < 4.0 && (0.0..=1.0).contains(&(text_lightness + step)) {
        text_lightness += step;
        text = from_hsl(hue, saturation, text_lightness);
    }
    (background, text)
}

/// Hue in degrees, saturation and lightness from 0 to 1
fn to_hsl(color: Color32) -> (f32, f32, f32) {
    let [r, g, b] = [color.r(), color.g(), color.b()].map(|c| c as f32 / 255.);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (max + min) / 2.;
    let chroma = max - min;
    if chroma == 0. {
        return (0., 0., lightness);
    }
    let saturation = chroma / (1. - (2. * lightness - 1.).abs());
    let hue = if max == r {
        ((g - b) / chroma).rem_euclid(6.)
    } else if max == g {
        (b - r) / chroma + 2.
    } else {
        (r - g) / chroma + 4.
    };
    (hue * 60., saturation, lightness)
}

fn from_hsl(hue: f32, saturation: f32, lightness: f32) -> Color32 {
    let chroma = (1. - (2. * lightness - 1.).abs()) * saturation;
    let sector = hue / 60.;
    let x = chroma * (1. - (sector.rem_euclid(2.) - 1.).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.),
        1 => (x, chroma, 0.),
        2 => (0., chroma, x),
        3 => (0., x, chroma),
        4 => (x, 0., chroma),
        _ => (chroma, 0., x),
    };
    let m = lightness - chroma / 2.;
    let to_u8 = |c: f32| ((c + m) * 255.).round().clamp(0., 255.) as u8;
    Color32::from_rgb(to_u8(r), to_u8(g), to_u8(b))
}

/// The contrast ratio of WCAG, from 1 to 21
fn contrast(a: Color32, b: Color32) -> f32 {
    let luminance = |c: Color32| {
        let [r, g, b] = [c.r(), c.g(), c.b()].map(egui::ecolor::linear_f32_from_gamma_u8);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
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

fn load_font_family(
    system: &fontdb::Database,
    family_names: &[&str],
    loaded_files: &mut Vec<PathBuf>,
) -> FontLookup {
    for &name in family_names {
        let query = fontdb::Query {
            families: &[fontdb::Family::Name(name)],
            ..Default::default()
        };
        let Some(id) = system.query(&query) else {
            debug!("Could not find {name}");
            continue;
        };
        match system.face_source(id) {
            Some((fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _), _)) => {
                if loaded_files.contains(&path) {
                    debug!("{name} is in {path:?}, which is loaded already");
                    return FontLookup::AlreadyLoaded;
                }
                info!("Loaded {name} from path: {:?}", path);
                if let Ok(data) = read(&path) {
                    loaded_files.push(path);
                    return FontLookup::Loaded(data);
                }
            }
            Some((fontdb::Source::Binary(data), _)) => {
                info!("Loaded {name} from memory.");
                return FontLookup::Loaded((*data).as_ref().to_vec());
            }
            None => debug!("Could not load {name}"),
        }
    }
    FontLookup::Missing
}

pub fn load_system_fonts(mut fonts: FontDefinitions) -> FontDefinitions {
    debug!("Attempting to load sys fonts");
    // In this order, the first font that has a glyph is used
    // System fonts should be prioritised if they are high quality.
    let regions = [
        (
            "simplified_chinese",
            vec![
                "PingFang SC", // macOS system font
                "Heiti SC",
                "Songti SC",
                "Noto Sans CJK SC", // Good coverage for Simplified Chinese
                "Noto Sans SC",
                "WenQuanYi Zen Hei", // Includes both Simplified and Traditional Chinese.
                "SimSun",
                "Noto Sans SC",
                "Source Han Sans CN",
            ],
        ),
        (
            "traditional_chinese",
            vec![
                "PingFang HK", // macOS system font
                "Source Han Sans HK",
            ],
        ),
        (
            "japanese",
            vec![
                "Hiragino Sans", // macOS system font
                "Noto Sans JP",
                "Noto Sans CJK JP",
                "Source Han Sans JP",
                "MS Gothic",
            ],
        ),
        (
            "korean",
            vec![
                "Apple SD Gothic Neo", // macOS system font
                "Source Han Sans KR",
            ],
        ),
        (
            "taiwanese",
            vec![
                "PingFang TC", // macOS system font
                "Source Han Sans TW",
            ],
        ),
        (
            "arabic_fonts",
            vec![
                "SF Arabic", // macOS system font
                "Noto Sans Arabic",
                "Amiri",
                "Lateef",
                "Al Tarikh",
                "Segoe UI",
            ],
        ),
    ];

    // the fonts the system has, from the folders and settings of fontconfig on Linux
    let mut system = fontdb::Database::new();
    system.load_system_fonts();

    // Several regions can end up with the same font file, the fonts for Chinese and
    // Japanese often are one. It is only loaded once.
    let mut loaded_files = Vec::new();
    for (region, font_names) in regions {
        match load_font_family(&system, &font_names, &mut loaded_files) {
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
    use super::{
        ColorTheme, OculanteState, accent_color, apply_theme, contrast, from_hsl,
        needs_system_fonts, selected_colors, to_hsl,
    };
    use egui::Color32;

    /// What was selected in a menu had gray text on the red accent, hard to read.
    /// Stoppedpuma's design has the red on #3D0A19.
    #[test]
    fn what_is_selected_has_the_accent_on_a_shade_of_it() {
        let red = Color32::from_rgb(255, 0, 75);
        assert_eq!(
            selected_colors(red, true),
            (Color32::from_hex("#3D0A19").unwrap(), red)
        );
        let ctx = egui::Context::default();
        apply_theme(&mut OculanteState::default(), &ctx);
        let visuals = &ctx.global_style().visuals;
        assert_eq!(
            visuals.selection.bg_fill,
            Color32::from_hex("#3D0A19").unwrap()
        );
        assert_eq!(visuals.selection.stroke.color, red);
        // what is drawn in the accent color does not take the shade
        assert_eq!(accent_color(&ctx), red);

        // any accent reads on its shade, in both themes
        for accent in [
            red,
            Color32::from_rgb(0, 170, 255),
            Color32::from_rgb(255, 204, 0),
            Color32::from_rgb(20, 40, 160),
            Color32::from_rgb(10, 200, 30),
            Color32::WHITE,
            Color32::BLACK,
        ] {
            for dark in [true, false] {
                let (background, text) = selected_colors(accent, dark);
                assert!(
                    contrast(text, background) >= 4.0,
                    "{accent:?} dark {dark}: {text:?} on {background:?}"
                );
                assert!(
                    (to_hsl(background).0 - to_hsl(accent).0).abs() < 3.,
                    "the shade keeps the hue of {accent:?}"
                );
            }
        }
        // the blue of the light theme gets darker to read on its light shade
        let (background, text) = selected_colors(Color32::from_rgb(0, 170, 255), false);
        assert!(
            background.r() > 180 && text.b() < 255,
            "{background:?} {text:?}"
        );
    }

    #[test]
    fn hsl_goes_there_and_back() {
        for color in [
            Color32::from_rgb(255, 0, 75),
            Color32::from_rgb(0, 170, 255),
            Color32::from_rgb(12, 200, 99),
            Color32::from_rgb(128, 128, 128),
            Color32::from_rgb(250, 240, 10),
        ] {
            let (h, s, l) = to_hsl(color);
            assert_eq!(from_hsl(h, s, l), color);
        }
        let (h, s, l) = to_hsl(Color32::from_rgb(255, 0, 75));
        assert_eq!((h.round(), s, (l * 100.).round()), (342., 1., 50.));
    }

    /// The text cursor was egui's light blue whatever the accent color was
    #[test]
    fn the_text_cursor_has_the_accent_color() {
        for theme in [ColorTheme::Dark, ColorTheme::Light] {
            let ctx = egui::Context::default();
            let mut state = OculanteState::default();
            state.persistent_settings.theme = theme.clone();
            state.persistent_settings.accent_color = [10, 200, 30];
            state.persistent_settings.accent_color_is_custom = true;
            apply_theme(&mut state, &ctx);
            assert_eq!(
                ctx.global_style().visuals.text_cursor.stroke.color,
                egui::Color32::from_rgb(10, 200, 30),
                "{theme:?}"
            );
        }
    }

    /// Dark picked in the settings while the system was light, then the system turned dark:
    /// egui showed its own style, with a teal accent (Stoppedpuma on macOS in #813)
    #[test]
    fn the_theme_stays_when_the_system_theme_changes() {
        for (picked, dark) in [(ColorTheme::Dark, true), (ColorTheme::Light, false)] {
            for (start, then) in [
                (egui::Theme::Light, egui::Theme::Dark),
                (egui::Theme::Dark, egui::Theme::Light),
            ] {
                let ctx = egui::Context::default();
                let mut state = OculanteState::default();
                state.persistent_settings.theme = picked.clone();
                state.persistent_settings.accent_color = [10, 200, 30];
                state.persistent_settings.accent_color_is_custom = true;
                let system = |theme| egui::RawInput {
                    system_theme: Some(theme),
                    ..Default::default()
                };
                ctx.run_ui(system(start), |ui| apply_theme(&mut state, ui.ctx()))
                    .textures_delta
                    .clear();
                ctx.run_ui(system(then), |_| {}).textures_delta.clear();
                let visuals = &ctx.global_style().visuals;
                assert_eq!(
                    visuals.text_cursor.stroke.color,
                    Color32::from_rgb(10, 200, 30),
                    "{picked:?} on a {start:?} system"
                );
                assert_eq!(visuals.dark_mode, dark, "{picked:?} on a {start:?} system");
            }
        }
    }

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
