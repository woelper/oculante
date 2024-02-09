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

pub fn browse<F: FnMut(Option<&PathBuf>)>(save: bool, mut callback: F, ctx: &egui::Context) {
    let mut path = ctx
        .data(|r| r.get_temp::<PathBuf>(Id::new("FBPATH")))
        .unwrap_or(load_recent_dir().unwrap_or_default());

    let mut open = true;

    egui::Window::new("Browse")
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

                ui.vertical(|ui| {
                    if ui.button(ARROW_BEND_LEFT_UP).clicked() {
                        if let Some(d) = path.parent() {
                            let p = d.to_path_buf();
                            path = p;
                        }
                    }

                    egui::ScrollArea::new([false, true])
                        // .max_width(500.)
                        .min_scrolled_height(500.)
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
                                                    callback(Some(&de.path()));
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
                });
            });

            ctx.data_mut(|w| w.insert_temp(Id::new("FBPATH"), path));
        });
    if !open {
        ctx.memory_mut(|w| w.close_popup());
    }
}
