use super::icons::*;
use crate::file_encoder::FileEncoder;
use crate::settings::VolatileSettings;
use crate::thumbnails::{Thumbnails, THUMB_CAPTION_HEIGHT, THUMB_SIZE};
use crate::ui::{render_file_icon, EguiExt, BUTTON_HEIGHT_LARGE};

use anyhow::{Context, Result};
use dirs;
use log::debug;
use notan::egui::{self, *};
use std::io::Write;
use std::{
    fs::{self, read_to_string, File},
    path::{Path, PathBuf},
};
use strum::IntoEnumIterator;

fn load_recent_dir() -> Result<PathBuf> {
    Ok(PathBuf::from(read_to_string(
        dirs::cache_dir()
            .context("Can't get cache dir")?
            .join("oculante")
            .join(".last_open_directory"),
    )?))
}

fn save_recent_dir(p: &Path) -> Result<()> {
    let p = if p.is_file() {
        p.parent().context("Can't get parent")?.to_path_buf()
    } else {
        p.to_path_buf()
    };

    let mut f = File::create(
        dirs::cache_dir()
            .context("Can't get cache dir")?
            .join("oculante")
            .join(".last_open_directory"),
    )?;
    write!(f, "{}", p.to_string_lossy())?;
    Ok(())
}

pub fn browse_modal<F: FnMut(&PathBuf)>(
    save: bool,
    filter: &[&str],
    settings: &mut VolatileSettings,
    mut callback: F,
    ctx: &egui::Context,
) {
    let mut path = ctx
        .data(|r| r.get_temp::<PathBuf>(Id::new("FBPATH")))
        .unwrap_or(load_recent_dir().unwrap_or_default());

    let mut open = true;

    egui::Window::new(if save { "Save" } else { "Open" })
        .collapsible(false)
        .open(&mut open)
        .resizable(true)
        .default_width(822.)
        .default_height(600.)
        .show(ctx, |ui| {
            browse(
                &mut path,
                filter,
                settings,
                save,
                |p| {
                    callback(p);
                    ctx.memory_mut(|w| w.close_popup());
                },
                ui,
            );

            if ui.ctx().input(|r| r.key_pressed(Key::Escape)) {
                ui.ctx().memory_mut(|w| w.close_popup());
            }
            ctx.data_mut(|w| w.insert_temp(Id::new("FBPATH"), path));
        });
    if !open {
        ctx.memory_mut(|w| w.close_popup());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Disk {
    name: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct BrowserState {
    filename: String,
    thumbnails: Thumbnails,
    search_term: String,
    search_active: bool,
    listview_active: bool,
    path_active: bool,
    entries: Option<Vec<PathBuf>>,
    drives: Option<Vec<Disk>>,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            filename: "unnamed.png".into(),
            thumbnails: Default::default(),
            search_term: Default::default(),
            search_active: Default::default(),
            listview_active: Default::default(),
            path_active: Default::default(),
            entries: Default::default(),
            drives: Default::default(),
        }
    }
}

pub fn browse<F: FnMut(&PathBuf)>(
    path: &mut PathBuf,
    filter: &[&str],
    settings: &mut VolatileSettings,
    save: bool,
    mut callback: F,
    ui: &mut Ui,
) {
    let mut prev_path = path.clone();

    let mut state = ui
        .ctx()
        .data(|r| r.get_temp::<BrowserState>(Id::new("FBSTATE")))
        .unwrap_or_default();

    if state.entries.is_none() {
        // mark prev_path as dirty. This is to cause a reload at first start
        prev_path = Default::default();
    }

    if state.drives.is_none() {
        let mut disks: Vec<Disk> = sysinfo::Disks::new_with_refreshed_list()
            .iter()
            .inspect(|d| debug!("d {:?}", d))
            .filter(|d| {
                if cfg!(target_os = "windows") {
                    true
                } else {
                    d.is_removable()
                }
            })
            .map(|d| Disk {
                name: if cfg!(target_os = "windows") {
                    d.mount_point().to_string_lossy().to_string()
                } else {
                    d.name().to_string_lossy().to_string()
                },
                path: d.mount_point().to_path_buf(),
            })
            .collect();
        disks.sort();
        disks.dedup_by(|a, b| a.name == b.name);
        state.drives = Some(disks);
    }

    let b_entries: Vec<PathBuf> = vec![];
    let entries = state
        .entries
        .as_ref()
        .unwrap_or(&b_entries)
        .iter()
        .filter(|e| {
            e.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
                .to_lowercase()
                .contains(&state.search_term.to_lowercase())
        })
        .collect::<Vec<_>>();

    let num_entries = entries.len();

    let item_spacing = 6.;
    ui.add_space(item_spacing);

    // The navigation bar
    ui.horizontal_wrapped(|ui| {
        ui.add_space(item_spacing);

        let search_icon = if state.search_active { BOLDX } else { SEARCH };
        let mut lock_search_focus = false;

        if ui
            .add(
                egui::Button::new(
                    RichText::new(search_icon).color(ui.style().visuals.selection.bg_fill),
                )
                .corner_radius(ui.get_rounding(BUTTON_HEIGHT_LARGE))
                .min_size(vec2(BUTTON_HEIGHT_LARGE, BUTTON_HEIGHT_LARGE)), // .shortcut_text("sds")
            )
            .clicked()
        {
            lock_search_focus = true;
            state.search_active = !state.search_active;
            if !state.search_active {
                state.search_term.clear();
            }
        }
        let textinput_width = if state.search_term.len() < 10 {
            (ui.ctx().animate_bool("id".into(), state.search_active) * 88.) as usize
        } else {
            ui.available_width() as usize
        };

        if state.search_active {
            ui.scope(|ui| {
                ui.visuals_mut().selection.stroke = Stroke::NONE;
                ui.visuals_mut().widgets.active.corner_radius =
                    CornerRadius::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
                ui.visuals_mut().widgets.inactive.corner_radius =
                    CornerRadius::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
                ui.visuals_mut().widgets.hovered.corner_radius =
                    CornerRadius::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
                let resp = ui.add(
                    TextEdit::singleline(&mut state.search_term)
                        .min_size(vec2(0., BUTTON_HEIGHT_LARGE))
                        .desired_width(textinput_width as f32)
                        .vertical_align(Align::Center),
                );

                if lock_search_focus {
                    ui.memory_mut(|r| r.request_focus(resp.id));
                }
            });
        }
        if state.search_term.len() >= 10 {
            ui.end_row();
            ui.add_space(item_spacing);
        }
        if ui
            .add(
                egui::Button::new(
                    RichText::new(CHEVRON_UP).color(ui.style().visuals.selection.bg_fill),
                )
                .corner_radius(ui.get_rounding(BUTTON_HEIGHT_LARGE))
                .min_size(vec2(BUTTON_HEIGHT_LARGE, BUTTON_HEIGHT_LARGE)), // .shortcut_text("sds")
            )
            .clicked()
        {
            if let Some(d) = path.parent() {
                let p = d.to_path_buf();
                *path = p;
            }
        }

        let path_icon = if state.path_active { FOLDER } else { TERMINAL };

        if ui
            .add(
                egui::Button::new(
                    RichText::new(path_icon).color(ui.style().visuals.selection.bg_fill),
                )
                .corner_radius(ui.get_rounding(BUTTON_HEIGHT_LARGE))
                .min_size(vec2(BUTTON_HEIGHT_LARGE, BUTTON_HEIGHT_LARGE)), // .shortcut_text("sds")
            )
            .clicked()
        {
            state.path_active = !state.path_active;
        }

        let current_dir = if path.is_dir() {
            path.clone()
        } else {
            path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
        };

        let cp = path.clone();
        // Too many folders make the dialog too large, cap them at this amount
        // the width, minus the left buttons roughly
        let mut available_width = ui.available_size_before_wrap().x;
        let mut max_nav_items = 0;
        // go through ancestors from the back
        for ancestor in cp.ancestors() {
            let ancestor_stem = ancestor
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or("Computer".to_string());
            let ancestor_len = ancestor_stem.len() as f32 * 11.5
                + ui.spacing().button_padding.x * 2.
                + ui.spacing().item_spacing.x * 2.;
            if available_width - ancestor_len > 0. {
                max_nav_items += 1;
                available_width -= ancestor_len;
            } else {
                break;
            }
        }

        let mut ancestors = cp
            .ancestors()
            .take(max_nav_items.max(1))
            .collect::<Vec<_>>();
        ancestors.reverse();

        if state.path_active {
            ui.scope(|ui| {
                let textinput_width = (ui.ctx().animate_bool("path".into(), state.path_active)
                    * ui.available_width()) as usize;
                let mut path_string = path.to_string_lossy().to_string();
                ui.visuals_mut().selection.stroke = Stroke::NONE;
                ui.visuals_mut().widgets.active.corner_radius =
                    CornerRadius::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
                ui.visuals_mut().widgets.inactive.corner_radius =
                    CornerRadius::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
                ui.visuals_mut().widgets.hovered.corner_radius =
                    CornerRadius::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
                let resp = ui.add(
                    TextEdit::singleline(&mut path_string)
                        .min_size(vec2(0., BUTTON_HEIGHT_LARGE))
                        .desired_width(textinput_width as f32)
                        .vertical_align(Align::Center),
                );

                if resp.changed() {
                    *path = PathBuf::from(path_string);
                }
            });

            // let r = ui.add(TextEdit::singleline(&mut path_string));
        } else {
            for c in ancestors {
                let label = c
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or("Computer".into());
                if ui
                    .styled_selectable_label(current_dir == c, format!("{label}  {CARET_RIGHT}"))
                    .clicked()
                {
                    *path = PathBuf::from(c);
                }
            }
        }
    });

    ui.horizontal(|ui| {
        ui.add_space(item_spacing);
        ui.allocate_ui_with_layout(
            Vec2::new(120., ui.available_height()),
            Layout::top_down_justified(Align::LEFT),
            |ui| {
                if let Some(d) = dirs::home_dir() {
                    if ui.styled_button(format!("{FOLDER} Home")).clicked() {
                        *path = d;
                    }
                }
                if let Some(drives) = state.drives.as_ref() {
                    for drive in drives {
                        if ui
                            .styled_button(format!("{DRIVE} {}", drive.name))
                            .clicked()
                        {
                            *path = drive.path.clone();
                        }
                    }
                }
                if let Some(d) = dirs::desktop_dir() {
                    if ui
                        .styled_button(format!("{FOLDERDESKTOP} Desktop"))
                        .clicked()
                    {
                        *path = d;
                    }
                }
                if let Some(d) = dirs::document_dir() {
                    if ui
                        .styled_button(format!("{FOLDERDOCUMENT} Documents"))
                        .clicked()
                    {
                        *path = d;
                    }
                }
                if let Some(d) = dirs::download_dir() {
                    if ui
                        .styled_button(format!("{FOLDERDOWNLOAD} Downloads"))
                        .clicked()
                    {
                        *path = d;
                    }
                }
                if let Some(d) = dirs::picture_dir() {
                    if ui
                        .styled_button(format!("{FOLDERIMAGE} Pictures"))
                        .clicked()
                    {
                        *path = d;
                    }
                }

                for folder in &settings.folder_bookmarks.clone() {
                    let res = ui.styled_button(format!(
                        "{FOLDERBOOKMARK} {}",
                        folder
                            .file_name()
                            .map(|x| x.to_string_lossy().to_string())
                            .unwrap_or_default()
                    ));

                    if res.clicked() {
                        *path = folder.clone();
                    }

                    if res.hovered() {
                        if ui.input(|r| r.key_released(Key::D)) && !ui.ctx().wants_keyboard_input()
                        {
                            settings.folder_bookmarks.remove(folder);
                        }
                        if ui.input(|r| r.pointer.secondary_released()) {
                            settings.folder_bookmarks.remove(folder);
                        }
                    }
                    res.on_hover_text("Right click or 'd' to delete!");
                }

                ui.vertical_centered_justified(|ui| {
                    let col = ui.style().visuals.widgets.inactive.weak_bg_fill;
                    if ui
                        .add(
                            egui::Button::new(RichText::new(PLUS).color(col))
                                .corner_radius(ui.get_rounding(BUTTON_HEIGHT_LARGE))
                                .fill(Color32::TRANSPARENT)
                                .frame(true)
                                .stroke(Stroke::new(2., col))
                                .min_size(vec2(140., BUTTON_HEIGHT_LARGE)),
                        )
                        .clicked()
                    {
                        settings.folder_bookmarks.insert(path.clone());
                    }
                });
            },
        );

        ui.vertical(|ui| {
            let panel_bg_color = match ui.style().visuals.dark_mode {
                true => Color32::from_gray(13),
                false => Color32::from_gray(217),
            };

            let r = ui.available_rect_before_wrap();

            let spacing = ui.style().spacing.item_spacing.x;
            let w = r.width() - spacing * 3. + 2.;

            let thumbs_per_row = (w / (THUMB_SIZE[0] as f32 + spacing))
                .floor()
                .max(1.)
                .min(num_entries as f32);
            let num_rows = (num_entries as f32 / (thumbs_per_row).max(1.)).ceil() as usize;

            egui::Frame::new()
                .fill(panel_bg_color)
                .corner_radius(ui.style().visuals.widgets.active.corner_radius * 2.0)
                .inner_margin(Margin::same(10))
                .show(ui, |ui| {
                    egui::ScrollArea::new([false, true])
                        .min_scrolled_height(400.)
                        .auto_shrink([false, false])
                        .show_rows(
                            ui,
                            (THUMB_SIZE[1] + THUMB_CAPTION_HEIGHT) as f32,
                            num_rows,
                            |ui, row_range| {
                                let range = (row_range.start * thumbs_per_row as usize)
                                    ..(row_range.end * thumbs_per_row as usize).min(num_entries);
                                let mut visible_entries: Vec<&PathBuf> = vec![];
                                for i in range {
                                    if let Some(e) = entries.get(i) {
                                        visible_entries.push(e);
                                    }
                                }
                                if state.listview_active {
                                } else {
                                    ui.horizontal_wrapped(|ui| {
                                        if visible_entries.is_empty() {
                                            let r = ui.label("Empty directory");
                                            let r = r.interact(Sense::click());
                                            if r.clicked() {
                                                if let Some(parent) = path.parent() {
                                                    *path = parent.to_path_buf();
                                                }
                                            }
                                        } else {
                                            for de in visible_entries.iter().filter(|e| e.is_dir())
                                            {
                                                if render_file_icon(de, ui, &mut state.thumbnails)
                                                    .clicked()
                                                {
                                                    *path = de.to_path_buf();
                                                }
                                            }

                                            for de in visible_entries {
                                                if de.is_file()
                                                    && render_file_icon(
                                                        de,
                                                        ui,
                                                        &mut state.thumbnails,
                                                    )
                                                    .clicked()
                                                {
                                                    _ = save_recent_dir(de);
                                                    if !save {
                                                        state.search_active = false;
                                                        state.search_term.clear();
                                                        callback(de);
                                                    } else {
                                                        state.filename = de
                                                            .file_name()
                                                            .map(|f| {
                                                                f.to_string_lossy().to_string()
                                                            })
                                                            .unwrap_or_default();
                                                    }
                                                }
                                            }
                                        }
                                    });
                                }
                            },
                        );
                });

            // ui.add_space(10.);

            if save {
                let mut ext = Path::new(&state.filename).ext();

                // Safeguard: if saving as lut, ase etc, make sure the extension matches. If not, choose the first filter item.
                if !filter.contains(&ext.as_str()) {
                    ext = filter.first().map(|e| e.to_string()).unwrap_or(ext.clone());
                    state.filename = Path::new(&state.filename)
                        .with_extension(&ext)
                        .to_string_lossy()
                        .to_string();
                }

                ui.label("Filename");
                ui.horizontal(|ui| {
                    ui.spacing_mut().button_padding = Vec2::new(2., 5.);
                    let textinput = ui.add(
                        egui::TextEdit::singleline(&mut state.filename)
                            .min_size(Vec2::new(10., 28.)),
                    );

                    if prev_path == PathBuf::default() {
                        textinput.request_focus();
                    }

                    for f in FileEncoder::iter() {
                        if !filter.contains(&f.ext().as_str()) {
                            continue;
                        }
                        let e = f.ext();
                        if ui.selectable_label(ext == e, &e).clicked() {
                            state.filename = Path::new(&state.filename)
                                .with_extension(&e)
                                .to_string_lossy()
                                .to_string();
                        }
                    }

                    if ui.button("   Save file   ".to_string()).clicked() {
                        state.search_active = false;
                        state.search_term.clear();
                        prev_path = Default::default();
                        callback(&path.join(state.filename.clone()));
                    }
                });

                for fe in settings.encoding_options.iter_mut() {
                    if ext.to_lowercase() == fe.ext() {
                        fe.ui(ui);
                    }
                }
            }
        });
    });

    if prev_path != *path {
        match fs::read_dir(&path) {
            Ok(contents) => {
                debug!("Successfully read {}", path.display());
                let mut contents = contents
                    .into_iter()
                    .flatten()
                    .filter(|de| !de.file_name().to_string_lossy().starts_with("."))
                    .filter(|de| {
                        de.path().is_dir()
                            || filter.contains(&de.path().ext().to_lowercase().as_str())
                    })
                    .map(|d| d.path())
                    .collect::<Vec<_>>();

                contents.sort_by(|a, b| {
                    a.file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default()
                        .to_lowercase()
                        .cmp(
                            &b.file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default()
                                .to_lowercase(),
                        )
                });
                contents.sort_by_key(|b| std::cmp::Reverse(b.is_dir()));
                state.entries = Some(contents);
            }
            Err(_e) => {
                state.entries = None;
            }
        }
    }

    ui.ctx()
        .data_mut(|r| r.insert_temp::<BrowserState>(Id::new("FBSTATE"), state));
}

trait PathExt {
    fn ext(&self) -> String {
        todo!()
    }
}

impl PathExt for PathBuf {
    fn ext(&self) -> String {
        self.extension()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

impl PathExt for Path {
    fn ext(&self) -> String {
        self.extension()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default()
    }
}

// the native file dialog

#[cfg(feature = "file_open")]
use crate::appstate::OculanteState;

// Show file browser to select image to load
#[cfg(feature = "file_open")]
pub fn browse_for_image_path(state: &mut OculanteState) {
    let start_directory = state.volatile_settings.last_open_directory.clone();
    let load_sender = state.load_channel.0.clone();
    state.redraw = true;
    std::thread::spawn(move || {
        let uppercase_lowercase_ext = [
            crate::utils::SUPPORTED_EXTENSIONS
                .iter()
                .map(|e| e.to_ascii_lowercase())
                .collect::<Vec<_>>(),
            crate::utils::SUPPORTED_EXTENSIONS
                .iter()
                .map(|e| e.to_ascii_uppercase())
                .collect::<Vec<_>>(),
        ]
        .concat();
        let file_dialog_result = rfd::FileDialog::new()
            .add_filter("All Supported Image Types", &uppercase_lowercase_ext)
            .add_filter("All File Types", &["*"])
            .set_directory(start_directory)
            .pick_file();
        if let Some(file_path) = file_dialog_result {
            let _ = load_sender.send(file_path);
        }
    });
}
