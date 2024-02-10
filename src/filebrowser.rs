use super::utils::SUPPORTED_EXTENSIONS;
use anyhow::{Context, Result};
use dirs;
use egui_phosphor::variants::regular::*;
use notan::egui::{self, *};
use std::io::Write;

use std::{
    fs::{self, read_to_string, File},
    path::{Path, PathBuf},
};

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

pub fn browse_modal<F: FnMut(Option<&PathBuf>)>(save: bool, mut callback: F, ctx: &egui::Context) {
    let mut path = ctx
        .data(|r| r.get_temp::<PathBuf>(Id::new("FBPATH")))
        .unwrap_or(load_recent_dir().unwrap_or_default());

    let mut filename = ctx
        .data(|r| r.get_temp::<String>(Id::new("FBFILENAME")))
        .unwrap_or(String::from("unnamed.png"));

    let mut open = true;

    egui::Window::new(if save { "Save" } else { "Open" })
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .open(&mut open)
        .resizable(true)
        .default_width(500.)
        .default_height(600.)
        // .auto_sized()
        .show(ctx, |ui| {
            let d = fs::read_dir(&path).ok();
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(120., ui.available_height()),
                    Layout::top_down_justified(Align::LEFT),
                    |ui| {
                        if let Some(d) = dirs::desktop_dir() {
                            if ui.button(format!("{DESKTOP} Desktop")).clicked() {
                                path = d;
                            }
                        }
                        if let Some(d) = dirs::home_dir() {
                            if ui.button(format!("{HOUSE} Home")).clicked() {
                                path = d;
                            }
                        }
                        if let Some(d) = dirs::document_dir() {
                            if ui.button(format!("{FILE} Documents")).clicked() {
                                path = d;
                            }
                        }
                        if let Some(d) = dirs::download_dir() {
                            if ui.button(format!("{DOWNLOAD} Downloads")).clicked() {
                                path = d;
                            }
                        }
                        if let Some(d) = dirs::picture_dir() {
                            if ui.button(format!("{IMAGES} Pictures")).clicked() {
                                path = d;
                            }
                        }
                    },
                );
                ui.separator();

                ui.vertical(|ui| {
                    if ui.button(ARROW_BEND_LEFT_UP).clicked() {
                        if let Some(d) = path.parent() {
                            let p = d.to_path_buf();
                            path = p;
                        }
                    }

                    ui.separator();

                    egui::ScrollArea::new([false, true])
                        // .max_width(500.)
                        .min_scrolled_height(200.)
                        .auto_shrink([true, false])
                        .show(ui, |ui| match d {
                            Some(contents) => {
                                egui::Grid::new("browser")
                                    .striped(true)
                                    .num_columns(0)
                                    .min_col_width(ui.available_width())
                                    .show(ui, |ui| {
                                        for de in contents
                                            .into_iter()
                                            .flat_map(|x| x)
                                            .filter(|de| {
                                                !de.file_name().to_string_lossy().starts_with(".")
                                            })
                                            .filter(|de| {
                                                de.path().is_dir()
                                                    || SUPPORTED_EXTENSIONS.contains(
                                                        &de.path()
                                                            .extension()
                                                            .map(|ext| {
                                                                ext.to_string_lossy().to_string()
                                                            })
                                                            .unwrap_or_default()
                                                            .to_lowercase()
                                                            .as_str(),
                                                    )
                                            })
                                        {
                                            if de.path().is_dir() {
                                                if ui
                                                    .add(
                                                        egui::Button::new(format!(
                                                            "{FOLDER} {}",
                                                            de.file_name()
                                                                .to_string_lossy()
                                                                .chars()
                                                                .take(50)
                                                                .collect::<String>()
                                                        ))
                                                        .frame(false),
                                                    )
                                                    .clicked()
                                                {
                                                    path = de.path();
                                                }
                                            } else {
                                                if ui
                                                    .add(
                                                        egui::Button::new(format!(
                                                            "{IMAGE_SQUARE} {}",
                                                            de.file_name()
                                                                .to_string_lossy()
                                                                .chars()
                                                                .take(50)
                                                                .collect::<String>()
                                                        ))
                                                        .frame(false),
                                                    )
                                                    .clicked()
                                                {
                                                    _ = save_recent_dir(&de.path());
                                                    if !save {
                                                        callback(Some(&de.path()));
                                                    } else {
                                                        filename = de
                                                            .path()
                                                            .to_path_buf()
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
                                                    // self.result = Some(de.path().to_path_buf());
                                                }
                                            }
                                            ui.end_row();
                                        }
                                    });
                            }
                            None => {
                                ui.label("no contents");
                            }
                        });
                    ui.spacing();
                    ui.separator();

                    if !save {
                        if ui.button("Cancel").clicked() {
                            ctx.memory_mut(|w| w.close_popup());
                        }
                    }

                    if save {
                        ui.horizontal(|ui| {
                            ui.label("Filename");
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut filename)
                                        .desired_width(f32::INFINITY),
                                )
                                .changed()
                            {
                                ui.ctx().data_mut(|w| {
                                    w.insert_temp(Id::new("FBFILENAME"), filename.clone())
                                });
                            }
                        });

                        ui.horizontal(|ui| {
                            if ui.button("Save").clicked() {
                                callback(Some(&path.join(filename)));
                            }
                            if ui.button("Cancel").clicked() {
                                ctx.memory_mut(|w| w.close_popup());
                            }
                        });
                    }
                });
            });

            ctx.data_mut(|w| w.insert_temp(Id::new("FBPATH"), path));
        });
    if !open {
        ctx.memory_mut(|w| w.close_popup());
    }
}

