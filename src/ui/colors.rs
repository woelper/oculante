//! Every colour of the interface is in this file, and nowhere else (clippy.toml and the test
//! below keep it this way). The rest of the code asks for them by name. egui draws its own widgets
//! in the colours of its slots, and `egui_follows_us` points those slots at the colours here.
//!
//! A colour that is the same every time is a constant, written in HSL as designs have them. A
//! function is only used where the colour depends on something at run time: the theme that is
//! on, the accent, a colour it is given.
//!
//! What the person picked or what comes from the image is not a colour of the interface: it is
//! kept as bytes, and made a `Color32` to be shown with `ToColor32`.

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "colours are made here"
)]

use egui::{Color32, Context, Response, Stroke, Style, Ui};

// Making colours

/// A colour from its hue in degrees (0 to 360), and its saturation and lightness in percent (0 to
/// 100), whole numbers as designs have them. It is worked out with integers, rounding half up, so
/// it is the same everywhere and can be a constant.
pub const fn hsl(hue: u16, saturation: u8, lightness: u8) -> Color32 {
    assert!(hue <= 360 && saturation <= 100 && lightness <= 100);
    let (h, s, l) = (hue as i64, saturation as i64, lightness as i64);
    Color32::from_rgb(
        hsl_channel(0, h, s, l),
        hsl_channel(8, h, s, l),
        hsl_channel(4, h, s, l),
    )
}

/// One channel of `hsl`, `offset` 0 for red, 8 for green and 4 for blue. A channel is the
/// lightness, less the saturation times the smaller of the lightness and what is left to white,
/// times a triangle of the hue from -1 to 1. Here the triangle is in thirtieths, from -30 to 30,
/// and the channel in 255ths over 20,000.
const fn hsl_channel(offset: i64, hue: i64, saturation: i64, lightness: i64) -> u8 {
    let k = (offset * 30 + hue) % 360;
    let mut t = if k - 90 < 270 - k { k - 90 } else { 270 - k };
    if t > 30 {
        t = 30;
    }
    if t < -30 {
        t = -30;
    }
    let m = if lightness < 100 - lightness {
        lightness
    } else {
        100 - lightness
    };
    ((17 * (lightness * 3000 - saturation * m * t) + 10_000) / 20_000) as u8
}

/// How much of a colour is over what is behind it, in percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opacity {
    /// To mark an area and leave what is in it as it is
    Subtle = 5,
    /// A hover, a fill that is barely there
    Faint = 10,
    Light = 25,
    Half = 50,
    Strong = 75,
}

/// A colour at one of the steps of `Opacity`: at `Light` a quarter of it is over what is behind
pub const fn opacity(color: Color32, opacity: Opacity) -> Color32 {
    const fn scaled(channel: u8, percent: u32) -> u8 {
        ((channel as u32 * percent + 50) / 100) as u8
    }
    let percent = opacity as u32;
    Color32::from_rgba_premultiplied(
        scaled(color.r(), percent),
        scaled(color.g(), percent),
        scaled(color.b(), percent),
        scaled(color.a(), percent),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemedColor {
    dark: Color32,
    light: Color32,
}

impl ThemedColor {
    const fn new(dark: Color32, light: Color32) -> Self {
        Self { dark, light }
    }

    /// The shade for the theme that is on. This gets looked up every call so that theme change takes effect immediately.
    pub fn of(self, ui: &Ui) -> Color32 {
        self.in_theme(ui.visuals().dark_mode)
    }

    /// The shade for the dark or the light theme
    pub fn in_theme(self, dark_mode: bool) -> Color32 {
        if dark_mode { self.dark } else { self.light }
    }
}

/// The picked colour, or the image's colour, as RGBA bytes
pub trait ToColor32 {
    fn to_color32(self) -> Color32;
}

impl ToColor32 for [u8; 3] {
    fn to_color32(self) -> Color32 {
        Color32::from_rgb(self[0], self[1], self[2])
    }
}

impl ToColor32 for [u8; 4] {
    fn to_color32(self) -> Color32 {
        Color32::from_rgba_unmultiplied(self[0], self[1], self[2], self[3])
    }
}

// The colours

pub const WHITE: Color32 = hsl(0, 0, 100);
pub const BLACK: Color32 = hsl(0, 0, 0);
pub const GRAY: Color32 = hsl(0, 0, 50);
/// Nothing is filled
pub const TRANSPARENT: Color32 = Color32::TRANSPARENT;
/// An image drawn as it is, not tinted
pub const UNTINTED: Color32 = Color32::WHITE;

// Red, green and blue as the channels of the image have them: in the histogram, the curves and the levels.
pub const PURE_RED: Color32 = hsl(0, 100, 50);
pub const PURE_GREEN: Color32 = hsl(120, 100, 50);
pub const PURE_BLUE: Color32 = hsl(240, 100, 50);
// The fill under the lines of the histogram
pub const CHANNEL_FILL_RED: Color32 = Color32::from_rgba_premultiplied(70, 0, 0, 0);
pub const CHANNEL_FILL_GREEN: Color32 = Color32::from_rgba_premultiplied(0, 70, 0, 0);
pub const CHANNEL_FILL_BLUE: Color32 = Color32::from_rgba_premultiplied(0, 0, 70, 0);

// What is wrong, what is to be seen, what is to be known
pub const RED: Color32 = hsl(0, 100, 65);
pub const YELLOW: Color32 = hsl(48, 100, 50);
pub const BLUE: Color32 = hsl(202, 100, 61);

/// The dark edge on both sides of a line over the image, see `over_image`
pub const OVER_IMAGE_EDGE: Color32 = opacity(BLACK, Opacity::Light);

/// A white line over the image, with a dark edge on both sides so it shows on light images.
/// The edge is drawn first, then the line, both are centred on the same path.
pub fn over_image(width: f32) -> (Stroke, Stroke) {
    (
        Stroke::new(width + 2., OVER_IMAGE_EDGE),
        Stroke::new(width, WHITE),
    )
}

/// Windows and the panels at the sides
pub const PANEL: ThemedColor = ThemedColor::new(hsl(0, 0, 10), hsl(0, 0, 90));
/// What is sunk into a panel: its sections, the list of files, text fields and scroll bars
pub const FIELD: ThemedColor = ThemedColor::new(hsl(0, 0, 5), hsl(0, 0, 85));
/// A card on a panel, one for each edit and annotation
pub const CARD: ThemedColor = ThemedColor::new(hsl(0, 0, 13), hsl(0, 0, 95));
pub const BUTTON: ThemedColor = ThemedColor::new(hsl(0, 0, 15), WHITE);
pub const TEXT: ThemedColor = ThemedColor::new(hsl(0, 0, 80), hsl(0, 0, 20));

// The colours the person can pick start as these. They are kept as bytes in the settings and
// compared with what was saved. About hsl(342, 100, 50) and hsl(200, 100, 50), and about
// hsl(0, 0, 12) and hsl(0, 0, 78): the light theme swaps them for the second until one is picked.
pub const DEFAULT_ACCENT: [u8; 3] = [255, 0, 75];
pub const LIGHT_THEME_ACCENT: [u8; 3] = [0, 170, 255];
pub const DEFAULT_BACKGROUND: [u8; 3] = [30, 30, 30];
pub const LIGHT_THEME_BACKGROUND: [u8; 3] = [200, 200, 200];

// Colours of egui that are not ours yet, read from its slots

/// Text that is less important
pub fn weak_text(ui: &Ui) -> Color32 {
    ui.visuals().weak_text_color()
}

/// Text that stands out
pub fn strong_text(ui: &Ui) -> Color32 {
    ui.visuals().strong_text_color()
}

/// What is drawn on a widget, in the colour of the state it is in: idle, hovered or pressed
pub fn state_color(ui: &Ui, response: &Response) -> Color32 {
    ui.style().interact(response).fg_stroke.color
}

fn accent_id() -> egui::Id {
    egui::Id::new("accent color")
}

/// The accent colour picked in the settings, as the theme applied it last. It reads the
/// context, so it waits forever when called in a closure that holds it, such as one given to
/// `fonts_mut`.
pub fn accent_color(ctx: &Context) -> Color32 {
    ctx.data(|data| data.get_temp(accent_id()))
        .unwrap_or_else(|| DEFAULT_ACCENT.to_color32())
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

/// What is selected and a button that is on, as `selected_colors` makes them for the accent
/// and the theme that are on: the shade behind and the colour of the text
pub fn selection_colors(ui: &Ui) -> (Color32, Color32) {
    selected_colors(accent_color(ui.ctx()), ui.visuals().dark_mode)
}

/// `t` of the way from `from` to `to`, from 0 to 1: a colour that changes over a moment, as a
/// handle under the mouse turns red
pub fn fade(from: Color32, to: Color32, t: f32) -> Color32 {
    from.lerp_to_gamma(to, t)
}

/// The gray furthest in brightness from `value` so that it stands out on that colour.
pub fn contrast_gray(value: u8) -> Color32 {
    Color32::from_gray(255 - value)
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

// What egui draws by itself, in our colours

/// egui draws its widgets in the colours of its slots. This points each slot it uses at a
/// colour above, for the theme that is on, and remembers the accent. Change the named colour,
/// not the line here.
pub(super) fn egui_follows_us(ctx: &Context, style: &mut Style, accent: Color32) {
    ctx.data_mut(|data| data.insert_temp(accent_id(), accent));
    let dark = style.visuals.dark_mode;
    let v = &mut style.visuals;
    v.panel_fill = PANEL.in_theme(dark);
    v.window_fill = PANEL.in_theme(dark);
    v.extreme_bg_color = FIELD.in_theme(dark);
    v.widgets.noninteractive.fg_stroke.color = TEXT.in_theme(dark);
    v.widgets.inactive.fg_stroke.color = TEXT.in_theme(dark);
    v.widgets.inactive.weak_bg_fill = BUTTON.in_theme(dark);
    if !dark {
        v.widgets.inactive.bg_fill = BUTTON.in_theme(dark);
        v.widgets.hovered.bg_fill = opacity(WHITE, Opacity::Strong);
    }
    // No stroke on buttons
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.warn_fg_color = YELLOW;
    v.error_fg_color = RED;
    // The cursor where text is typed, egui draws its own light blue
    v.text_cursor.stroke.color = accent;
    // What is selected in menus, lists and text, the outline of a text field being typed in
    let (background, text) = selected_colors(accent, dark);
    v.selection.bg_fill = background;
    v.selection.stroke = Stroke::new(2.0, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The integer HSL matches the float one of the selection shades within a step of rounding
    #[test]
    fn hsl_is_hsl() {
        for (h, s, l) in [
            (0, 100, 50),
            (48, 100, 50),
            (202, 100, 61),
            (342, 100, 50),
            (0, 0, 13),
        ] {
            let made = hsl(h, s, l);
            let float = from_hsl(h as f32, s as f32 / 100., l as f32 / 100.);
            for (a, b) in [
                (made.r(), float.r()),
                (made.g(), float.g()),
                (made.b(), float.b()),
            ] {
                assert!(a.abs_diff(b) <= 1, "hsl({h}, {s}, {l}): {made:?} {float:?}");
            }
        }
        assert_eq!(RED, Color32::from_rgb(255, 77, 77));
        assert_eq!(YELLOW, Color32::from_rgb(255, 204, 0));
        assert_eq!(TEXT.in_theme(true), Color32::from_gray(204));
        assert_eq!(TEXT.in_theme(false), Color32::from_gray(51));
    }

    #[test]
    fn opacity_is_a_part_of_the_colour() {
        assert_eq!(
            opacity(WHITE, Opacity::Half),
            Color32::from_white_alpha(128)
        );
        assert_eq!(
            opacity(BLACK, Opacity::Light),
            Color32::from_black_alpha(64)
        );
        let steps = [
            Opacity::Subtle,
            Opacity::Faint,
            Opacity::Light,
            Opacity::Half,
            Opacity::Strong,
        ];
        assert_eq!(
            steps.map(|step| opacity(RED, step).a()),
            [13, 26, 64, 128, 191]
        );
    }

    /// What was selected in a menu had gray text on the red accent, hard to read.
    /// Stoppedpuma's design has the red on #3D0A19.
    #[test]
    fn what_is_selected_has_the_accent_on_a_shade_of_it() {
        let red = DEFAULT_ACCENT.to_color32();
        assert_eq!(
            selected_colors(red, true),
            (Color32::from_hex("#3D0A19").unwrap(), red)
        );
        // any accent reads on its shade, in both themes
        for accent in [
            red,
            LIGHT_THEME_ACCENT.to_color32(),
            Color32::from_rgb(255, 204, 0),
            Color32::from_rgb(20, 40, 160),
            Color32::from_rgb(10, 200, 30),
            WHITE,
            BLACK,
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
        let (background, text) = selected_colors(LIGHT_THEME_ACCENT.to_color32(), false);
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

    /// Before the theme is applied the accent is the default one, not a colour made from it
    #[test]
    fn the_accent_before_the_theme_is_the_default_one() {
        let ctx = egui::Context::default();
        assert_eq!(accent_color(&ctx), DEFAULT_ACCENT.to_color32());
    }

    /// What clippy does not see, outside this file: egui's own colours (`Color32::WHITE`) and
    /// the colour slots of egui read or set. Clippy finds the functions and methods that make
    /// colours, see clippy.toml.
    #[test]
    fn ci_no_colors_are_made_outside_colors_rs() {
        const SLOTS: [&str; 8] = [
            "extreme_bg_color",
            "panel_fill",
            "window_fill",
            "faint_bg_color",
            "code_bg_color",
            "hyperlink_color",
            "warn_fg_color",
            "error_fg_color",
        ];
        let mut found = vec![];
        let mut folders = vec![std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src"
        ))];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    folders.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs") || path.ends_with("ui/colors.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                for (i, line) in text.lines().enumerate() {
                    let code = without_comment(line);
                    let constant = code.match_indices("Color32::").any(|(at, name)| {
                        code[at + name.len()..].starts_with(|c: char| c.is_ascii_uppercase())
                    });
                    if constant || SLOTS.iter().any(|slot| code.contains(slot)) {
                        found.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
                    }
                }
            }
        }
        assert!(
            found.is_empty(),
            "made outside src/ui/colors.rs, ask it for these by name:\n{}",
            found.join("\n")
        );
    }

    /// The line up to a `//` that is not in a string
    fn without_comment(line: &str) -> &str {
        let bytes = line.as_bytes();
        let (mut in_string, mut escaped) = (false, false);
        for (i, &byte) in bytes.iter().enumerate() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
            } else if byte == b'"' {
                // not the character '"'
                let quoted = i > 0 && bytes[i - 1] == b'\'' && bytes.get(i + 1) == Some(&b'\'');
                in_string = !quoted;
            } else if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
                return &line[..i];
            }
        }
        line
    }

    #[test]
    fn comments_are_not_code() {
        assert_eq!(without_comment("let a = 1; // Color32::RED"), "let a = 1; ");
        assert_eq!(
            without_comment(r#"let url = "https://x"; // b"#),
            r#"let url = "https://x"; "#
        );
        assert_eq!(without_comment(r#"let c = '"'; // b"#), r#"let c = '"'; "#);
        assert_eq!(
            without_comment(r#"let s = "a\"//b";"#),
            r#"let s = "a\"//b";"#
        );
    }
}
