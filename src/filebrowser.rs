use super::icons::*;
#[cfg(feature = "file_open")]
use crate::appstate::OculanteState;
use crate::file_encoder::FileEncoder;
use crate::file_sort::{FileInfo, FileSort, SortBy};
use crate::settings::VolatileSettings;
use crate::thumbnails::{THUMB_CAPTION_HEIGHT, THUMB_SIZE, Thumbnails};
use crate::ui::{BUTTON_HEIGHT_LARGE, EguiExt, paint_thumbnail, render_file_icon};

use egui::{self, *};
use egui_extras::{Column, TableBuilder};
use log::debug;
use std::{
    fs,
    path::{Path, PathBuf},
};
use strum::IntoEnumIterator;

pub fn browse_modal<F: FnMut(&PathBuf)>(
    save: bool,
    filter: &[&str],
    settings: &mut VolatileSettings,
    mut callback: F,
    ctx: &egui::Context,
    popup_id: Id,
) {
    let mut path = ctx
        .data(|r| r.get_temp::<PathBuf>(Id::new("FBPATH")))
        .unwrap_or_else(|| settings.last_open_directory.clone());

    let mut open = true;
    // Escape closes a list that is open in the dialog, not the dialog
    let popup_was_open = egui::Popup::is_any_open(ctx);

    egui::Window::new(if save { "Save" } else { "Open" })
        .collapsible(false)
        .open(&mut open)
        .resizable(true)
        .default_width(830.)
        .default_height(600.)
        .show(ctx, |ui| {
            browse(
                &mut path,
                filter,
                settings,
                save,
                |p| {
                    callback(p);
                    crate::ui::close_popup(ctx, popup_id);
                },
                ui,
            );

            if !popup_was_open && ui.ctx().input(|r| r.key_pressed(Key::Escape)) {
                crate::ui::close_popup(ctx, popup_id);
            }
            ctx.data_mut(|w| w.insert_temp(Id::new("FBPATH"), path));
        });
    if !open {
        crate::ui::close_popup(ctx, popup_id);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Disk {
    name: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct BrowserState {
    filename: String,
    thumbnails: Thumbnails,
    search_term: String,
    search_active: bool,
    path_active: bool,
    pub entries: Option<Vec<FileInfo>>,
    /// The order the entries are in
    sorted: FileSort,
    pub last_dir: BrowserDir,
    drives: Option<Vec<Disk>>,
}

impl BrowserState {
    /// Check if the directory needs to be reloaded based on the current [`BrowserDir`].
    pub fn check_refresh_entries(ui: &Ui, last_dir: BrowserDir, this_parent: Option<&Path>) {
        let (saved_last_dir, last_parent_eq) = ui
            .ctx()
            .data(|r| r.get_temp::<BrowserState>(Id::new("FBSTATE")))
            .map(|state| {
                (
                    state.last_dir,
                    state
                        .entries
                        .as_deref()
                        .and_then(|entries| {
                            let entry = entries.first()?;
                            entry.path.parent()
                        })
                        .map(|last_parent| Some(last_parent) == this_parent)
                        .unwrap_or_default(),
                )
            })
            .unzip();
        let last_parent_eq = last_parent_eq.unwrap_or_default();

        // Refresh the file browser state if the CWD changed.
        // This can mean that the current image's dir is different from the last image's dir or
        // that the user switched file browser modes.
        //
        // So, the path should refresh if the user shift clicked previously (current image
        // mode) and clicked this time (last dir mode) or vice versa.
        //
        // OR we should refresh the directory if the user shift clicked previously AND this time
        // BUT the two images are in different directories.
        if saved_last_dir != Some(last_dir)
            || saved_last_dir == Some(BrowserDir::CurrentImageDir)
            || !last_parent_eq
        {
            ui.ctx().data_mut(|w| {
                w.remove::<BrowserState>(Id::new("FBSTATE"));

                let state = BrowserState {
                    last_dir,
                    ..Default::default()
                };
                w.insert_temp(Id::new("FBSTATE"), state)
            });
        }
    }
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            filename: "unnamed.png".into(),
            thumbnails: Default::default(),
            search_term: Default::default(),
            search_active: Default::default(),
            path_active: Default::default(),
            entries: Default::default(),
            sorted: Default::default(),
            last_dir: BrowserDir::default(),
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

    if state.sorted != settings.file_sort
        && let Some(entries) = state.entries.as_mut()
    {
        settings.file_sort.sort(entries);
        state.sorted = settings.file_sort;
    }

    if state.drives.is_none() {
        // Only names and mount points are needed. Asking for the free space as
        // well blocks on network mounts, an rclone mount can take seconds.
        let mut disks: Vec<Disk> =
            sysinfo::Disks::new_with_refreshed_list_specifics(sysinfo::DiskRefreshKind::nothing())
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

    let b_entries: Vec<FileInfo> = vec![];
    let search_term = state.search_term.to_lowercase();
    let entries = state
        .entries
        .as_ref()
        .unwrap_or(&b_entries)
        .iter()
        .filter(|e| e.name().to_lowercase().contains(&search_term))
        .collect::<Vec<_>>();
    // what was clicked, an index into `entries`
    let mut clicked = None;

    let num_entries = entries.len();

    let item_spacing = 6.;
    ui.add_space(item_spacing);

    // The navigation bar
    ui.horizontal_wrapped(|ui| {
        ui.add_space(item_spacing);

        let search_icon = if state.search_active { BOLDX } else { SEARCH };
        let mut lock_search_focus = false;

        if ui.add(nav_button(ui, search_icon)).clicked() {
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
        if ui.add(nav_button(ui, CHEVRON_UP)).clicked()
            && let Some(d) = path.parent()
        {
            let p = d.to_path_buf();
            *path = p;
        }

        let path_icon = if state.path_active { FOLDER } else { TERMINAL };

        if ui.add(nav_button(ui, path_icon)).clicked() {
            state.path_active = !state.path_active;
        }

        // room for the order and the view on the right
        let controls_width = 180.;

        let current_dir = if path.is_dir() {
            path.clone()
        } else {
            path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
        };

        let cp = path.clone();
        // Too many folders make the dialog too large, cap them at this amount
        // the width, minus the left buttons roughly
        let mut available_width = ui.available_size_before_wrap().x - controls_width;
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
                    * (ui.available_width() - controls_width))
                    .max(0.) as usize;
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

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(item_spacing);
            let (view_icon, view_hint) = if settings.browser_list {
                (GRID, "Show thumbnails")
            } else {
                (LIST, "Show a list")
            };
            if ui
                .add(nav_button(ui, view_icon))
                .on_hover_text(view_hint)
                .clicked()
            {
                settings.browser_list = !settings.browser_list;
            }
            sort_menu(ui, &mut settings.file_sort);
        });
    });

    // Take the whole remaining height of the window. A plain horizontal layout is only
    // as high as one row, which pins the file list and with it the window height.
    ui.allocate_ui_with_layout(
        ui.available_size(),
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            ui.add_space(item_spacing);
            ui.allocate_ui_with_layout(
                Vec2::new(120., ui.available_height()),
                Layout::top_down_justified(Align::LEFT),
                |ui| {
                    if let Some(d) = dirs::home_dir()
                        && ui.styled_button(format!("{FOLDER} Home")).clicked()
                    {
                        *path = d;
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
                    if let Some(d) = dirs::desktop_dir()
                        && ui
                            .styled_button(format!("{FOLDERDESKTOP} Desktop"))
                            .clicked()
                    {
                        *path = d;
                    }
                    if let Some(d) = dirs::document_dir()
                        && ui
                            .styled_button(format!("{FOLDERDOCUMENT} Documents"))
                            .clicked()
                    {
                        *path = d;
                    }
                    if let Some(d) = dirs::download_dir()
                        && ui
                            .styled_button(format!("{FOLDERDOWNLOAD} Downloads"))
                            .clicked()
                    {
                        *path = d;
                    }
                    if let Some(d) = dirs::picture_dir()
                        && ui
                            .styled_button(format!("{FOLDERIMAGE} Pictures"))
                            .clicked()
                    {
                        *path = d;
                    }

                    for folder in &settings.folder_bookmarks.clone() {
                        crate::ui::want_system_fonts_for(&folder.to_string_lossy());
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
                            if ui.input(|r| r.key_released(Key::D))
                                && !ui.ctx().egui_wants_keyboard_input()
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

                // The name of the file and the button to save go below the files. Keep room
                // for them, as high as they were in the last frame.
                let save_row_id = Id::new("file_browser_save_row");
                let save_row = if save {
                    ui.ctx()
                        .data(|d| d.get_temp::<f32>(save_row_id))
                        .unwrap_or(BUTTON_HEIGHT_LARGE * 2.)
                        + ui.spacing().item_spacing.y
                } else {
                    0.
                };
                let files_size = vec2(
                    ui.available_width(),
                    (ui.available_height() - save_row).max(0.),
                );
                ui.allocate_ui(files_size, |ui| {
                    egui::Frame::new()
                        .fill(panel_bg_color)
                        .corner_radius(ui.style().visuals.widgets.active.corner_radius * 2.0)
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            if settings.browser_list && !entries.is_empty() {
                                clicked = list(
                                    ui,
                                    &entries,
                                    &mut settings.file_sort,
                                    &mut state.thumbnails,
                                );
                                return;
                            }
                            // never taller than the room left, the row to save goes below
                            egui::ScrollArea::new([false, true])
                                .min_scrolled_height(ui.available_height().min(400.))
                                .auto_shrink([false, false])
                                .scroll_source(egui::containers::scroll_area::ScrollSource::ALL)
                                .show_rows(
                                    ui,
                                    (THUMB_SIZE[1] + THUMB_CAPTION_HEIGHT) as f32,
                                    num_rows,
                                    |ui, row_range| {
                                        let range = (row_range.start * thumbs_per_row as usize)
                                            ..(row_range.end * thumbs_per_row as usize)
                                                .min(num_entries);
                                        ui.horizontal_wrapped(|ui| {
                                            if entries.is_empty() {
                                                let r = ui.label("Empty directory");
                                                let r = r.interact(Sense::click());
                                                if r.clicked()
                                                    && let Some(parent) = path.parent()
                                                {
                                                    *path = parent.to_path_buf();
                                                }
                                            }
                                            for i in range {
                                                if render_file_icon(
                                                    &entries[i].path,
                                                    entries[i].is_dir,
                                                    ui,
                                                    &mut state.thumbnails,
                                                )
                                                .clicked()
                                                {
                                                    clicked = Some(i);
                                                }
                                            }
                                        });
                                    },
                                );
                        });
                });

                // ui.add_space(10.);

                if save {
                    let save_ui = ui
                        .scope(|ui| {
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

                                let formats = FileEncoder::iter()
                                    .map(|f| f.ext())
                                    .filter(|e| filter.contains(&e.as_str()))
                                    .collect::<Vec<_>>();
                                if formats.len() > 1 {
                                    let mut chosen = ext.clone();
                                    egui::ComboBox::from_id_salt("save_format")
                                        .selected_text(&ext)
                                        .width(70.)
                                        .show_ui(ui, |ui| {
                                            for e in formats {
                                                ui.selectable_value(&mut chosen, e.clone(), e);
                                            }
                                        });
                                    if chosen != ext {
                                        state.filename = Path::new(&state.filename)
                                            .with_extension(&chosen)
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
                        })
                        .response;
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(save_row_id, save_ui.rect.height()));
                }
            });
        },
    );

    if let Some(i) = clicked {
        let entry = entries[i].clone();
        if entry.is_dir {
            *path = entry.path;
            // If user has a search term active, we want to
            // clear it when changing dir
            if state.search_active {
                state.search_term.clear();
            }
        } else {
            settings.last_open_directory = entry
                .path
                .parent()
                .and_then(|parent| parent.canonicalize().ok())
                .unwrap_or_default();
            if !save {
                state.search_active = false;
                state.search_term.clear();
                callback(&entry.path);
            } else {
                state.filename = entry.name().to_string();
            }
        }
    }

    if prev_path != *path {
        match fs::read_dir(&path) {
            Ok(contents) => {
                debug!("Successfully read {}", path.display());
                crate::ui::want_system_fonts_for(&path.to_string_lossy());
                let mut contents = contents
                    .into_iter()
                    .flatten()
                    .filter(|de| !de.file_name().to_string_lossy().starts_with("."))
                    .map(|d| FileInfo::read(d.path()))
                    .filter(|info| {
                        info.is_dir || filter.contains(&info.path.ext().to_lowercase().as_str())
                    })
                    .collect::<Vec<_>>();
                settings.file_sort.sort(&mut contents);
                state.sorted = settings.file_sort;
                for entry in &contents {
                    crate::ui::want_system_fonts_for(&entry.name());
                }
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

/// A round button with an icon, as in the bar above the files
fn nav_button(ui: &Ui, icon: &str) -> Button<'static> {
    egui::Button::new(RichText::new(icon).color(ui.style().visuals.selection.bg_fill))
        .corner_radius(ui.get_rounding(BUTTON_HEIGHT_LARGE))
        .min_size(vec2(BUTTON_HEIGHT_LARGE, BUTTON_HEIGHT_LARGE))
}

fn arrow(sort: FileSort) -> &'static str {
    if sort.descending {
        "\u{2193}"
    } else {
        "\u{2191}"
    }
}

/// What the files are sorted by, and which way
fn sort_menu(ui: &mut Ui, sort: &mut FileSort) {
    // as wide as the longest, so the button stays where it is when the order changes
    let button = egui::Button::new(format!("{} {}", sort.by.label(), arrow(*sort)))
        .corner_radius(ui.get_rounding(BUTTON_HEIGHT_LARGE))
        .min_size(vec2(130., BUTTON_HEIGHT_LARGE));
    egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        for by in SortBy::iter() {
            if ui.selectable_label(sort.by == by, by.label()).clicked() {
                sort.by = by;
            }
        }
        ui.separator();
        if ui.selectable_label(!sort.descending, "Ascending").clicked() {
            sort.descending = false;
        }
        if ui.selectable_label(sort.descending, "Descending").clicked() {
            sort.descending = true;
        }
    });
}

/// A column title in the list, a click sorts by it and a second one turns the order around
fn sort_header(ui: &mut Ui, title: &str, by: SortBy, sort: &mut FileSort) {
    let text = if sort.by == by {
        format!("{title} {}", arrow(*sort))
    } else {
        title.to_string()
    };
    if ui
        .add(
            Label::new(RichText::new(text).strong())
                .selectable(false)
                .sense(Sense::click()),
        )
        .clicked()
    {
        if sort.by == by {
            sort.descending = !sort.descending;
        } else {
            *sort = FileSort {
                by,
                descending: false,
            };
        }
    }
}

/// The files as a list with a small picture, type, size and date. Returns the one clicked.
fn list(
    ui: &mut Ui,
    entries: &[&FileInfo],
    sort: &mut FileSort,
    thumbnails: &mut Thumbnails,
) -> Option<usize> {
    let row_height = 34.;
    let picture = vec2(row_height * 4. / 3., row_height) - vec2(4., 4.);
    // the date that is sorted by, the time of the last change otherwise
    let (date_title, date_by) = if sort.by == SortBy::Created {
        ("Created", SortBy::Created)
    } else {
        ("Modified", SortBy::Modified)
    };
    let mut clicked = None;
    let header_height = 24.;
    // the header comes on top of it
    let min_height =
        (ui.available_height() - header_height - ui.spacing().item_spacing.y).clamp(0., 400.);
    let label = |ui: &mut Ui, text: String| {
        ui.add(Label::new(text).selectable(false).truncate());
    };
    TableBuilder::new(ui)
        .id_salt("file_list")
        .striped(true)
        .sense(Sense::click())
        .auto_shrink([false, false])
        // never taller than the room left, the row to save goes below
        .min_scrolled_height(min_height)
        .max_scroll_height(f32::INFINITY)
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(Column::exact(picture.x + 8.))
        .column(Column::remainder().at_least(120.).clip(true))
        .column(Column::exact(60.))
        .column(Column::exact(80.))
        .column(Column::exact(130.))
        .header(header_height, |mut header| {
            header.col(|_| {});
            header.col(|ui| sort_header(ui, "Name", SortBy::Name, sort));
            header.col(|ui| sort_header(ui, "Type", SortBy::Type, sort));
            header.col(|ui| sort_header(ui, "Size", SortBy::Size, sort));
            header.col(|ui| sort_header(ui, date_title, date_by, sort));
        })
        .body(|body| {
            body.rows(row_height, entries.len(), |mut row| {
                let entry = entries[row.index()];
                row.col(|ui| small_file_icon(ui, entry, picture, thumbnails));
                row.col(|ui| label(ui, entry.name().to_string()));
                row.col(|ui| {
                    label(
                        ui,
                        if entry.is_dir {
                            "Folder".into()
                        } else {
                            entry.extension().to_uppercase()
                        },
                    )
                });
                row.col(|ui| {
                    if !entry.is_dir {
                        label(ui, human_size(entry.size));
                    }
                });
                row.col(|ui| {
                    let date = if date_by == SortBy::Created {
                        entry.created
                    } else {
                        entry.modified
                    };
                    label(ui, date.map(date_text).unwrap_or_default());
                });
                if row.response().clicked() {
                    clicked = Some(row.index());
                }
            });
        });
    clicked
}

/// A folder, or the thumbnail of a file, as large as a row of the list
fn small_file_icon(ui: &mut Ui, entry: &FileInfo, size: Vec2, thumbnails: &mut Thumbnails) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if entry.is_dir {
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            FOLDERFILL,
            FontId::proportional(size.y * 0.9),
            ui.style().visuals.text_color(),
        );
        return;
    }
    paint_thumbnail(
        ui,
        &entry.path,
        thumbnails,
        rect,
        CornerRadius::same(3),
        10.,
    );
}

/// A size in bytes, as file managers show it
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1000. && unit < UNITS.len() - 1 {
        size /= 1000.;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else if size < 10. {
        format!("{size:.1} {}", UNITS[unit])
    } else {
        format!("{size:.0} {}", UNITS[unit])
    }
}

/// A date in local time
fn date_text(time: std::time::SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(time)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

trait PathExt {
    fn ext(&self) -> String;
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

/// Default directory for file browser.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrowserDir {
    #[default]
    LastOpenDir,
    CurrentImageDir,
}

// Show file browser to select image to load
#[cfg(feature = "file_open")]
pub fn browse_for_image_path(state: &mut OculanteState) {
    let start_directory = state.filebrowser_path();
    let load_sender = state.load_channel.0.clone();
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
