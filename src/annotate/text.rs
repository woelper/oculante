use super::*;

/// Fonts for text annotations: Inter now, and system fonts for other languages once found.
static FONTS: LazyLock<Mutex<Arc<usvg::fontdb::Database>>> = LazyLock::new(|| {
    let mut fonts = usvg::fontdb::Database::new();
    fonts.load_font_data(crate::FONT.to_vec());
    fonts.load_font_data(crate::BOLD_FONT.to_vec());
    // the bold one calls itself "Inter 18pt", here it is the bold of Inter
    let bold: Vec<_> = fonts
        .faces()
        .filter(|face| face.families.iter().any(|(name, _)| name == "Inter 18pt"))
        .cloned()
        .collect();
    for mut face in bold {
        fonts.remove_face(face.id);
        face.families = vec![("Inter".into(), usvg::fontdb::Language::English_UnitedStates)];
        fonts.push_face_info(face);
    }
    fonts.set_sans_serif_family("Inter");
    // tests draw with Inter only, the same on every machine
    if !cfg!(test) {
        let mut with_system = fonts.clone();
        std::thread::spawn(move || {
            with_system.load_system_fonts();
            if let Ok(mut fonts) = FONTS.lock() {
                *fonts = Arc::new(with_system);
            }
            FONTS_VERSION.fetch_add(1, Ordering::Relaxed);
            crate::utils::request_repaint();
        });
    }
    Mutex::new(Arc::new(fonts))
});

/// Incremented whenever the fonts change, so anything showing text redraws on a repaint.
pub(super) static FONTS_VERSION: AtomicU32 = AtomicU32::new(0);

fn fonts() -> Arc<usvg::fontdb::Database> {
    FONTS
        .lock()
        .map(|fonts| fonts.clone())
        .unwrap_or_else(|_| Arc::new(usvg::fontdb::Database::new()))
}

/// Text ready to draw, with the box its lines take
pub(super) struct TextLayout {
    pub(super) tree: usvg::Tree,
    pub(super) bounds: [f32; 4],
}

#[derive(PartialEq)]
struct TextKey {
    text: String,
    size: u32,
    color: [u8; 4],
    font: Font,
    outline: u32,
    outline_color: [u8; 4],
    fonts: u32,
}

/// Recently laid-out text, newest first.
static TEXTS: Mutex<Vec<(TextKey, Arc<TextLayout>)>> = Mutex::new(Vec::new());

/// Distance of the lines of a text, in sizes of the text
pub(crate) const LINE_HEIGHT: f32 = 1.25;

/// The text of an annotation, in its size, colors and font
pub(super) fn text_layout(text: &str, a: &Annotation) -> Option<Arc<TextLayout>> {
    // first, so the text is laid out with the fonts this key says
    let fonts = fonts();
    let key = TextKey {
        text: text.into(),
        size: a.width,
        color: a.color,
        font: a.font.clone(),
        outline: a.outline,
        outline_color: a.outline_color,
        fonts: FONTS_VERSION.load(Ordering::Relaxed),
    };
    let mut texts = TEXTS.lock().ok()?;
    if let Some(i) = texts.iter().position(|(k, _)| *k == key) {
        let found = texts.remove(i);
        let layout = found.1.clone();
        texts.insert(0, found);
        return Some(layout);
    }
    let layout = Arc::new(lay_out(&key, fonts)?);
    texts.insert(0, (key, layout.clone()));
    texts.truncate(32);
    Some(layout)
}

/// Recent results of `line_positions`, newest first
static POSITIONS: Mutex<Vec<(TextKey, Arc<Vec<f32>>)>> = Mutex::new(Vec::new());

/// Where each character of a line starts, plus where the line ends, in the text's own
/// coordinates. Used to place the caret and the selection while typing.
pub(crate) fn line_positions(line: &str, a: &Annotation) -> Arc<Vec<f32>> {
    let fonts = fonts();
    let key = TextKey {
        text: line.into(),
        size: a.width,
        color: [0, 0, 0, 255],
        font: a.font.clone(),
        outline: 0,
        outline_color: [0; 4],
        fonts: FONTS_VERSION.load(Ordering::Relaxed),
    };
    if let Ok(mut known) = POSITIONS.lock()
        && let Some(i) = known.iter().position(|(k, _)| *k == key)
    {
        let found = known.remove(i);
        let positions = found.1.clone();
        known.insert(0, found);
        return positions;
    }
    let positions = Arc::new(lay_out_positions(&key, fonts));
    if let Ok(mut known) = POSITIONS.lock() {
        known.insert(0, (key, positions.clone()));
        known.truncate(16);
    }
    positions
}

fn lay_out_positions(key: &TextKey, fonts: Arc<usvg::fontdb::Database>) -> Vec<f32> {
    let count = key.text.chars().count();
    // a marker after the line shows where it ends, trailing spaces included
    let marked = TextKey {
        text: format!("{}|", key.text),
        size: key.size,
        color: key.color,
        font: key.font.clone(),
        outline: key.outline,
        outline_color: key.outline_color,
        fonts: key.fonts,
    };
    let mut glyphs = vec![];
    if let Some(layout) = lay_out(&marked, fonts) {
        glyphs_of(layout.tree.root(), &mut glyphs);
    }
    // characters in a ligature share its position
    let mut positions = Vec::with_capacity(count + 1);
    for (text, x) in &glyphs {
        for _ in text.chars() {
            positions.push(*x);
        }
    }
    if positions.len() == count + 1 {
        return positions;
    }
    // fallback if the glyphs don't line up with the characters
    let end = glyphs.last().map_or(0., |(_, x)| *x);
    (0..=count)
        .map(|i| end * i as f32 / count.max(1) as f32)
        .collect()
}

/// Collects the text and x position of each glyph
fn glyphs_of(group: &usvg::Group, glyphs: &mut Vec<(String, f32)>) {
    for node in group.children() {
        match node {
            usvg::Node::Text(text) => {
                for span in text.layouted() {
                    for glyph in &span.positioned_glyphs {
                        glyphs.push((glyph.text.clone(), glyph.transform().tx));
                    }
                }
            }
            usvg::Node::Group(group) => glyphs_of(group, glyphs),
            _ => {}
        }
    }
}

fn lay_out(key: &TextKey, fonts: Arc<usvg::fontdb::Database>) -> Option<TextLayout> {
    let size = key.size.max(1) as f32;
    let escape = |line: &str| {
        line.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let mut lines = String::new();
    for (i, line) in key.text.split('\n').enumerate() {
        let dy = if i == 0 { 0. } else { size * LINE_HEIGHT };
        // an empty line still takes its place
        let line = if line.is_empty() {
            "\u{a0}".into()
        } else {
            escape(line)
        };
        lines += &format!(r#"<tspan x="0" dy="{dy}">{line}</tspan>"#);
    }
    let [r, g, b, a] = key.color;
    // Inter where the font lacks a letter, or is not there
    let family = key.font.family.replace(['\'', '"'], "");
    let family = if family.is_empty() {
        "Inter".into()
    } else {
        format!("'{}', Inter", escape(&family))
    };
    // half of it is under the letters
    let outline = match key.outline {
        0 => String::new(),
        width => {
            let [r, g, b, a] = key.outline_color;
            format!(
                r#" stroke="rgb({r},{g},{b})" stroke-opacity="{}" stroke-width="{}" stroke-linejoin="round" paint-order="stroke""#,
                a as f32 / 255.,
                width * 2
            )
        }
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><text y="{size}" font-family="{family}" font-weight="{}" font-size="{size}" fill="rgb({r},{g},{b})" fill-opacity="{}"{outline} xml:space="preserve">{lines}</text></svg>"#,
        key.font.weight,
        a as f32 / 255.
    );
    let options = usvg::Options {
        fontdb: fonts,
        font_family: "Inter".into(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&svg, &options).ok()?;
    let bounds = text_bounds(tree.root())?;
    Some(TextLayout { tree, bounds })
}

fn text_bounds(group: &usvg::Group) -> Option<[f32; 4]> {
    group.children().iter().find_map(|node| match node {
        usvg::Node::Text(text) => {
            let b = text.bounding_box();
            Some([b.left(), b.top(), b.right(), b.bottom()])
        }
        usvg::Node::Group(group) => text_bounds(group),
        _ => None,
    })
}

/// The families of the fonts with the weights each has, Inter first
pub(crate) type Families = Arc<Vec<(String, Vec<u16>)>>;

static FAMILIES: Mutex<Option<(u32, Families)>> = Mutex::new(None);

pub(crate) fn font_families() -> Families {
    let version = FONTS_VERSION.load(Ordering::Relaxed);
    let fonts = fonts();
    let Ok(mut known) = FAMILIES.lock() else {
        return Default::default();
    };
    if let Some((v, families)) = &*known
        && *v == version
    {
        return families.clone();
    }
    let mut weights = std::collections::BTreeMap::<String, Vec<u16>>::new();
    for face in fonts.faces() {
        if face.style != usvg::fontdb::Style::Normal {
            continue;
        }
        if let Some((family, _)) = face.families.first() {
            let list = weights.entry(family.clone()).or_default();
            if !list.contains(&face.weight.0) {
                list.push(face.weight.0);
            }
        }
    }
    let mut families: Vec<(String, Vec<u16>)> = weights
        .into_iter()
        .map(|(family, mut list)| {
            list.sort_unstable();
            (family, list)
        })
        .collect();
    if let Some(i) = families.iter().position(|(family, _)| family == "Inter") {
        let inter = families.remove(i);
        families.insert(0, inter);
    }
    let families = Arc::new(families);
    *known = Some((version, families.clone()));
    families
}
