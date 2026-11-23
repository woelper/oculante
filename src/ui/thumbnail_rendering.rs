use super::*;
use egui::*;

/// Helper function to paint an image - directly from egui as it is private to it
fn paint_texture_load_result(
    ui: &Ui,
    tlr: &load::TextureLoadResult,
    rect: Rect,
    show_loading_spinner: Option<bool>,
    options: &ImageOptions,
) {
    match tlr {
        Ok(load::TexturePoll::Ready { texture }) => {
            paint_texture_at(ui.painter(), rect, options, texture);
        }
        Ok(load::TexturePoll::Pending { .. }) => {
            let show_loading_spinner =
                show_loading_spinner.unwrap_or(ui.visuals().image_loading_spinners);
            if show_loading_spinner {
                Spinner::new().paint_at(ui, rect);
            }
        }
        Err(_) => {
            let font_id = TextStyle::Body.resolve(ui.style());
            ui.painter()
                .text(rect.center(), Align2::CENTER_CENTER, "⚠", font_id, RED);
        }
    }
}

/// The URI egui loads a file from. On Windows egui 0.36 takes `file://C:\x` for a path
/// on the network, 0.31 of 0.9.6 did not: `file:///C:\x` is `C:\x`.
pub fn file_uri(path: &Path) -> String {
    if cfg!(windows) {
        format!("file:///{}", path.display())
    } else {
        format!("file://{}", path.display())
    }
}

/// The thumbnail of a file in `rect`, or its extension while there is none
pub fn paint_thumbnail(
    ui: &Ui,
    path: &Path,
    thumbnails: &mut Thumbnails,
    rect: Rect,
    rounding: CornerRadius,
    font_size: f32,
) {
    match thumbnails.get(path) {
        Ok(tp) => {
            let image = egui::Image::new(file_uri(&tp))
                .corner_radius(rounding)
                .show_loading_spinner(true);

            let load_result = image.load_for_size(ui.ctx(), rect.size());

            paint_texture_load_result(ui, &load_result, rect, None, image.image_options());
            if load_result.is_err() {
                // If an image could not be loaded, reload. This is usually because the image
                // is being written while loading.
                ui.ctx().forget_image(&file_uri(&tp));
            }
        }
        Err(_) => {
            ui.painter().rect_filled(rect, rounding, FIELD.of(ui));
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                path.extension()
                    .map(|e| e.to_string_lossy().to_string().to_uppercase())
                    .unwrap_or_default(),
                FontId::proportional(font_size),
                TEXT.of(ui),
            );
        }
    }
}

pub fn render_file_icon(
    icon_path: &Path,
    is_dir: bool,
    ui: &mut Ui,
    thumbnails: &mut Thumbnails,
) -> Response {
    let zoom = 1.;

    let size = Vec2::new(
        THUMB_SIZE[0] as f32,
        (THUMB_SIZE[1] + THUMB_CAPTION_HEIGHT) as f32,
    ) * zoom;
    let response = ui.allocate_response(size, Sense::click());
    let rounding = CornerRadius::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));

    let mut image_rect = response.rect;
    image_rect.max = image_rect.max.round();
    image_rect.min = image_rect.min.round();
    image_rect.set_bottom(image_rect.max.y - THUMB_CAPTION_HEIGHT as f32);

    let text_color = TEXT.of(ui);
    if is_dir {
        ui.painter().text(
            response.rect.center(),
            Align2::CENTER_CENTER,
            FOLDERFILL,
            FontId::proportional(85.),
            text_color,
        );
    } else {
        paint_thumbnail(ui, icon_path, thumbnails, image_rect, rounding, 25.);
    }

    let name = icon_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    let width = THUMB_SIZE[0] as f32;
    let mut job = LayoutJob::simple(name, FontId::proportional(13.), text_color, width);
    job.halign = Align::Center;

    // the generic hover effect, a rect over everything
    if response.hovered() {
        ui.painter()
            .rect_filled(image_rect, rounding, opacity(WHITE, Opacity::Light));

        // the whole name, over what is under it
        let galley = ui.painter().layout_job(job);
        let painter = ui
            .ctx()
            .layer_painter(LayerId::new(Order::Tooltip, "Folder captions".into()))
            .with_clip_rect(ui.clip_rect());

        let mut right_bottom = image_rect.right_bottom();
        right_bottom.y += galley.rect.height() + 14.;
        let r = Rect::from_two_pos(image_rect.left_bottom(), right_bottom).expand(1.);
        painter.rect_filled(r, rounding, FIELD.of(ui));
        painter.galley(image_rect.expand(6.).center_bottom(), galley, text_color);
    } else {
        job.wrap = TextWrapping::truncate_at_width(width);
        let galley = ui.painter().layout_job(job);
        ui.painter()
            .galley(image_rect.expand(6.).center_bottom(), galley, text_color);
    }
    response
}
