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
            .context("Can't get temp dir")?
            .join(".efd_history"),
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
            .context("Can't get temp dir")?
            .join(".efd_history"),
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
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .open(&mut open)
        .resizable(true)
        //TODO: Change default_width to 815 after folder misalignment fix, discord this comment and use another closest to reference design value if the slider can be combined into the image area BG
        .default_width(818.)
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
#[derive(Debug, Clone)]
struct BrowserState {
    filename: String,
    thumbnails: Thumbnails,
    search_term: String,
    search_active: bool,
    listview_active: bool,
    path_active: bool,
    entries: Option<Vec<PathBuf>>,
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
        // mark prev_path as dirty. This is to cause a reload at first start,
        prev_path = Default::default();
    }
    let mut entries = state
        .entries
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|e| {
            e.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
                .to_lowercase()
                .contains(&state.search_term.to_lowercase())
        })
        .collect::<Vec<_>>();

    entries.sort_by(|a, b| b.is_dir().cmp(&a.is_dir()));

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
                    RichText::new(format!("{search_icon}"))
                        .color(ui.style().visuals.selection.bg_fill),
                )
                .rounding(ui.get_rounding(BUTTON_HEIGHT_LARGE))
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
                ui.visuals_mut().widgets.active.rounding =
                    Rounding::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
                ui.visuals_mut().widgets.inactive.rounding =
                    Rounding::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
                ui.visuals_mut().widgets.hovered.rounding =
                    Rounding::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
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
                    RichText::new(format!("{CHEVRON_UP}"))
                        .color(ui.style().visuals.selection.bg_fill),
                )
                .rounding(ui.get_rounding(BUTTON_HEIGHT_LARGE))
                .min_size(vec2(BUTTON_HEIGHT_LARGE, BUTTON_HEIGHT_LARGE)), // .shortcut_text("sds")
            )
            .clicked()
        {
            if let Some(d) = path.parent() {
                let p = d.to_path_buf();
                *path = p;
            }
        }

        let path_icon = if state.path_active { FOLDER } else { "$" };

        if ui
            .add(
                egui::Button::new(
                    RichText::new(path_icon).color(ui.style().visuals.selection.bg_fill),
                )
                .rounding(ui.get_rounding(BUTTON_HEIGHT_LARGE))
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
        let max_nav_items = 6;
        let mut ancestors = cp.ancestors().take(max_nav_items).collect::<Vec<_>>();
        ancestors.reverse();

        if state.path_active {
            ui.scope(|ui| {
                let textinput_width = (ui.ctx().animate_bool("path".into(), state.path_active)
                    * ui.available_width()) as usize;
                let mut path_string = path.to_string_lossy().to_string();
                ui.visuals_mut().selection.stroke = Stroke::NONE;
                ui.visuals_mut().widgets.active.rounding =
                    Rounding::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
                ui.visuals_mut().widgets.inactive.rounding =
                    Rounding::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
                ui.visuals_mut().widgets.hovered.rounding =
                    Rounding::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));
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
                    .styled_selectable_label(&current_dir == &c, &format!("{label}  {CARET_RIGHT}"))
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
                    if ui.styled_button(&format!("{FOLDER} Home")).clicked() {
                        *path = d;
                    }
                }
                if let Some(d) = dirs::desktop_dir() {
                    if ui
                        .styled_button(&format!("{FOLDERDESKTOP} Desktop"))
                        .clicked()
                    {
                        *path = d;
                    }
                }
                if let Some(d) = dirs::document_dir() {
                    if ui
                        .styled_button(&format!("{FOLDERDOCUMENT} Documents"))
                        .clicked()
                    {
                        *path = d;
                    }
                }
                if let Some(d) = dirs::download_dir() {
                    if ui
                        .styled_button(&format!("{FOLDERDOWNLOAD} Downloads"))
                        .clicked()
                    {
                        *path = d;
                    }
                }
                if let Some(d) = dirs::picture_dir() {
                    if ui
                        .styled_button(&format!("{FOLDERIMAGE} Pictures"))
                        .clicked()
                    {
                        *path = d;
                    }
                }

                for folder in &settings.folder_bookmarks.clone() {
                    let res = ui.styled_button(&format!(
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
                        if ui.input(|r| r.key_released(Key::D)) {
                            if !ui.ctx().wants_keyboard_input() {
                                settings.folder_bookmarks.remove(folder);
                            }
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
                                .rounding(ui.get_rounding(BUTTON_HEIGHT_LARGE))
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
            let w = r.width() - spacing * 3.;

            let thumbs_per_row = (w / (THUMB_SIZE[0] as f32 + spacing)).floor().max(1.);
            let num_rows = entries.len() / (thumbs_per_row as usize).max(1);

            // info!("tpr {thumbs_per_row} {w}, rows: {num_rows}");

            egui::Frame::none()
                .fill(panel_bg_color)
                .rounding(ui.style().visuals.widgets.active.rounding * 2.0)
                .inner_margin(Margin::same(10.))
                .show(ui, |ui| {
                    egui::ScrollArea::new([false, true])
                        .min_scrolled_height(400.)
                        .auto_shrink([false, false])
                        .show_rows(
                            ui,
                            (THUMB_SIZE[1] + THUMB_CAPTION_HEIGHT) as f32,
                            num_rows,
                            |ui, row_range| {
                                let entries = entries
                                    .clone()
                                    .drain(
                                        (row_range.start * thumbs_per_row as usize)
                                            ..(row_range.end * thumbs_per_row as usize),
                                    )
                                    .collect::<Vec<_>>();

                                if state.listview_active {
                                } else {
                                    ui.horizontal_wrapped(|ui| {
                                        if entries.is_empty() {
                                            let r = ui.label("Empty directory");
                                            let r = r.interact(Sense::click());
                                            if r.clicked() {
                                                if let Some(parent) = path.parent() {
                                                    *path = parent.to_path_buf();
                                                }
                                            }
                                        } else {
                                            for de in entries.iter().filter(|e| e.is_dir()) {
                                                if render_file_icon(&de, ui, &mut state.thumbnails)
                                                    .clicked()
                                                {
                                                    *path = de.to_path_buf();
                                                }
                                            }

                                            for de in entries {
                                                if de.is_file() {
                                                    if render_file_icon(
                                                        &de,
                                                        ui,
                                                        &mut state.thumbnails,
                                                    )
                                                    .clicked()
                                                    {
                                                        _ = save_recent_dir(&de);
                                                        if !save {
                                                            state.search_active = false;
                                                            state.search_term.clear();
                                                            callback(&de);
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
                                        }
                                    });
                                }
                            },
                        );
                });

            // ui.add_space(10.);

            if save {
                let ext = Path::new(&state.filename).ext();
                ui.label("Filename");
                ui.horizontal(|ui| {
                    ui.spacing_mut().button_padding = Vec2::new(2., 5.);
                    ui.add(
                        egui::TextEdit::singleline(&mut state.filename)
                            .min_size(Vec2::new(10., 28.)),
                    );
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

                    if ui.button(format!("   Save file   ")).clicked() {
                        state.search_active = false;
                        state.search_term.clear();
                        callback(&path.join(state.filename.clone()));
                    }
                });

                for fe in settings.encoding_options.iter_mut() {
                    if ext.to_lowercase() == fe.ext() {
                        fe.ui(ui);
                    }
                }
            }

            if prev_path != *path {
                if let Ok(contents) = fs::read_dir(&path) {
                    debug!("read {}", path.display());
                    let mut contents = contents
                        .into_iter()
                        .flat_map(|x| x)
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

                    // ui.ctx()
                    //     .data_mut(|r| r.insert_temp::<Vec<PathBuf>>(Id::new("FBDIRS"), contents));
                    state.entries = Some(contents);
                }
            }
        });
    });

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
