use super::icons::*;
use crate::file_encoder::FileEncoder;
use crate::settings::VolatileSettings;
use crate::ui::{EguiExt, BUTTON_HEIGHT_LARGE};

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
        .default_width(700.)
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

pub fn browse<F: FnMut(&PathBuf)>(
    path: &mut PathBuf,
    filter: &[&str],
    settings: &mut VolatileSettings,
    save: bool,
    mut callback: F,
    ui: &mut Ui,
) {
    let mut prev_path = path.clone();
    let mut filename = ui
        .ctx()
        .data(|r| r.get_temp::<String>(Id::new("FBFILENAME")))
        .unwrap_or(String::from("unnamed.png"));

    // read cached entries
    let entries = ui
        .ctx()
        .data(|r| r.get_temp::<Vec<PathBuf>>(Id::new("FBDIRS")));

    if entries.is_none() {
        // mark prev_path as dirty. This is to cause a reload at first start,
        prev_path = Default::default();
    }
    let entries = entries.unwrap_or_default();

    let item_spacing = 6.;
    ui.add_space(item_spacing);

    // The navigation bar
    ui.horizontal(|ui| {
        ui.add_space(item_spacing);
        if ui
            .add(
                egui::Button::new(format!("{CHEVRON_UP}"))
                    .rounding(5.)
                    .min_size(vec2(0., 35.)), // .shortcut_text("sds")
            )
            .clicked()
        {
            if let Some(d) = path.parent() {
                let p = d.to_path_buf();
                *path = p;
            }
        }

        let current_dir = if path.is_dir() {
            path.clone()
        } else {
            path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
        };

        let cp = path.clone();
        // Too  many folders make the dialog too large, cap them at this amount
        let max_nav_items = 6;
        let mut ancestors = cp.ancestors().take(max_nav_items).collect::<Vec<_>>();
        ancestors.reverse();

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
                        if ui.input(|r| r.pointer.secondary_released() || r.key_released(Key::D)) {
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
                                .rounding(ui.style().visuals.widgets.inactive.rounding)
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

            egui::ScrollArea::new([false, true])
                .min_scrolled_height(400.)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Frame::none()
                        .fill(panel_bg_color)
                        .rounding(ui.style().visuals.widgets.active.rounding * 1.5)
                        .inner_margin(Margin::same(6.))
                        .show(ui, |ui| {
                            egui::Grid::new("browser")
                                .striped(true)
                                .num_columns(0)
                                .min_col_width(ui.available_width())
                                .show(ui, |ui| {
                                    if entries.is_empty() {
                                        ui.label("Empty directory");
                                    } else {
                                        for de in entries {
                                            if de.is_dir() {
                                                if ui
                                                    .add(
                                                        egui::Button::new(format!(
                                                            "{FOLDER} {}",
                                                            de.file_name()
                                                                .map(|n| n.to_string_lossy())
                                                                .unwrap_or_default()
                                                                .chars()
                                                                .take(50)
                                                                .collect::<String>()
                                                        ))
                                                        .frame(false),
                                                    )
                                                    .clicked()
                                                {
                                                    *path = de;
                                                }
                                            } else {
                                                if ui
                                                    .add(
                                                        egui::Button::new(format!(
                                                            "{IMAGE_SQUARE} {}",
                                                            de.file_name()
                                                                .map(|f| f.to_string_lossy())
                                                                .unwrap_or_default()
                                                                .chars()
                                                                .take(50)
                                                                .collect::<String>()
                                                        ))
                                                        .frame(false),
                                                    )
                                                    .clicked()
                                                {
                                                    _ = save_recent_dir(&de);
                                                    if !save {
                                                        callback(&de);
                                                    } else {
                                                        filename = de
                                                            .file_name()
                                                            .map(|f| {
                                                                f.to_string_lossy().to_string()
                                                            })
                                                            .unwrap_or_default();
                                                        ui.ctx().data_mut(|w| {
                                                            w.insert_temp(
                                                                Id::new("FBFILENAME"),
                                                                filename.clone(),
                                                            )
                                                        });
                                                    }
                                                }
                                            }
                                            ui.end_row();
                                        }
                                    }
                                });
                        });
                });

            ui.add_space(10.);

            if save {
                let ext = Path::new(&filename)
                    .extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();

                ui.label("Filename");
                ui.horizontal(|ui| {
                    ui.spacing_mut().button_padding = Vec2::new(2., 5.);

                    ui.add(egui::TextEdit::singleline(&mut filename).min_size(Vec2::new(10., 28.)));

                    for f in FileEncoder::iter() {
                        let e = f.ext();
                        if ui.selectable_label(ext == e, &e).clicked() {
                            filename = Path::new(&filename)
                                .with_extension(&e)
                                .to_string_lossy()
                                .to_string();
                        }
                    }

                    if ui.button(format!("   Save file   ")).clicked() {
                        callback(&path.join(filename.clone()));
                    }
                });

                for fe in settings.encoding_options.iter_mut() {
                    if ext.to_lowercase() == fe.ext() {
                        fe.ui(ui);
                    }
                }
            }

            ui.ctx()
                .data_mut(|w| w.insert_temp(Id::new("FBFILENAME"), filename.clone()));

            if prev_path != *path {
                if let Ok(contents) = fs::read_dir(&path) {
                    debug!("read {}", path.display());
                    let mut contents = contents
                        .into_iter()
                        .flat_map(|x| x)
                        .filter(|de| !de.file_name().to_string_lossy().starts_with("."))
                        .filter(|de| {
                            de.path().is_dir()
                                || filter.contains(
                                    &de.path()
                                        .extension()
                                        .map(|ext| ext.to_string_lossy().to_string())
                                        .unwrap_or_default()
                                        .to_lowercase()
                                        .as_str(),
                                )
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

                    ui.ctx()
                        .data_mut(|r| r.insert_temp::<Vec<PathBuf>>(Id::new("FBDIRS"), contents));
                }
            }
        });
    });
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
