use super::icons::*;
use anyhow::{Context, Result};
use dirs;
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

pub fn browse_modal<F: FnMut(&PathBuf)>(
    save: bool,
    filter: &[&str],
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
        .default_width(500.)
        .default_height(600.)
        // .auto_sized()
        .show(ctx, |ui| {
            browse(
                &mut path,
                filter,
                save,
                |p| {
                    callback(p);
                    ctx.memory_mut(|w| w.close_popup());
                },
                ui,
            );

            if ui.button("Cancel").clicked() {
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
    save: bool,
    mut callback: F,
    ui: &mut Ui,
) {
    let mut filename = ui
        .ctx()
        .data(|r| r.get_temp::<String>(Id::new("FBFILENAME")))
        .unwrap_or(String::from("unnamed.png"));

    ui.horizontal(|ui| {
        // let current_dir = if path.
        for c in path.components() {
            // ui.selectable_label(format!("{}", c.as_os_str().to_string_lossy()));

            // if ui.add(egui::SelectableLabel::new(path, "First")).clicked() {

            // }
            ui.label(format!("{}", c.as_os_str().to_string_lossy()));
        }
    });

    let d = fs::read_dir(&path).ok();
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(120., ui.available_height()),
            Layout::top_down_justified(Align::LEFT),
            |ui| {
                if let Some(d) = dirs::desktop_dir() {
                    if ui.button(format!("{DESKTOP} Desktop")).clicked() {
                        *path = d;
                    }
                }
                if let Some(d) = dirs::home_dir() {
                    if ui.button(format!("{HOUSE} Home")).clicked() {
                        *path = d;
                    }
                }
                if let Some(d) = dirs::document_dir() {
                    if ui.button(format!("{FILE} Documents")).clicked() {
                        *path = d;
                    }
                }
                if let Some(d) = dirs::download_dir() {
                    if ui.button(format!("{DOWNLOAD} Downloads")).clicked() {
                        *path = d;
                    }
                }
                if let Some(d) = dirs::picture_dir() {
                    if ui.button(format!("{IMAGES} Pictures")).clicked() {
                        *path = d;
                    }
                }
            },
        );
        ui.separator();

        ui.vertical(|ui| {
            if ui.button(ARROW_BEND_LEFT_UP).clicked() {
                if let Some(d) = path.parent() {
                    let p = d.to_path_buf();
                    *path = p;
                }
            }

            ui.separator();

            egui::ScrollArea::new([false, true])
                .max_width(100.)
                .min_scrolled_height(400.)
                .auto_shrink([true, false])
                .show(ui, |ui| match d {
                    Some(contents) => {
                        egui::Grid::new("browser")
                            .striped(true)
                            .num_columns(0)
                            .min_col_width(ui.available_width())
                            .show(ui, |ui| {
                                let mut entries = contents
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
                                    .collect::<Vec<_>>();

                                entries.sort_by(|a, b| {
                                    a.file_name()
                                        .to_string_lossy()
                                        .to_lowercase()
                                        .cmp(&b.file_name().to_string_lossy().to_lowercase())
                                });

                                for de in entries {
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
                                            *path = de.path();
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
                                                callback(&de.path());
                                            } else {
                                                filename = de
                                                    .path()
                                                    .to_path_buf()
                                                    .file_name()
                                                    .map(|f| f.to_string_lossy().to_string())
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

            if save {
                ui.horizontal(|ui| {
                    ui.label("Filename");
                    ui.add(
                        egui::TextEdit::singleline(&mut filename)
                            .desired_width(ui.available_width() - 10.),
                    );
                });

                ui.horizontal(|ui| {
                    let ext = Path::new(&filename)
                        .extension()
                        .map(|e| e.to_string_lossy().to_string())
                        .unwrap_or_default();
                    for f in filter {
                        if ui.selectable_label(&ext == f, f.to_string()).clicked() {
                            filename = Path::new(&filename)
                                .with_extension(f)
                                .to_string_lossy()
                                .to_string();
                        }
                    }
                });

                ui.ctx()
                    .data_mut(|w| w.insert_temp(Id::new("FBFILENAME"), filename.clone()));
                if ui.button("Save").clicked() {
                    callback(&path.join(filename));
                }
            }
        });
    });
}
