use super::*;
use notan::egui::*;

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
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "⚠",
                font_id,
                ui.visuals().error_fg_color,
            );
        }
    }
}

pub fn render_file_icon(icon_path: &Path, ui: &mut Ui, thumbnails: &mut Thumbnails) -> Response {
    let zoom = 1.;

    let size = Vec2::new(
        THUMB_SIZE[0] as f32,
        (THUMB_SIZE[1] + THUMB_CAPTION_HEIGHT) as f32,
    ) * zoom;
    let response = ui.allocate_response(size, Sense::click());
    let rounding = Rounding::same(ui.get_rounding(BUTTON_HEIGHT_LARGE));

    let mut image_rect = response.rect;
    image_rect.max = image_rect.max.round();
    image_rect.min = image_rect.min.round();
    image_rect.set_bottom(image_rect.max.y - THUMB_CAPTION_HEIGHT as f32);

    if icon_path.is_dir() {
        ui.painter().text(
            response.rect.center(),
            Align2::CENTER_CENTER,
            FOLDERFILL,
            FontId::proportional(85.),
            ui.style().visuals.text_color(),
        );
    } else {
        match thumbnails.get(icon_path) {
            Ok(tp) => {
                let image = egui::Image::new(format!("file://{}", tp.display()))
                    .rounding(rounding)
                    .show_loading_spinner(true);

                let load_result = image.load_for_size(ui.ctx(), image_rect.size());

                paint_texture_load_result(
                    ui,
                    &load_result,
                    image_rect,
                    None,
                    image.image_options(),
                );
                if load_result.is_err() {
                    // If an image could not be loaded, reload. This is usually because the image
                    // is being written while loading.
                    ui.ctx().forget_image(&format!("file://{}", tp.display()));
                }
            }
            Err(_) => {
                // warn!("{e}");
                ui.painter()
                    .rect_filled(image_rect, rounding, Color32::from_gray(80).to_opaque());
                ui.painter().text(
                    image_rect.center(),
                    Align2::CENTER_CENTER,
                    icon_path
                        .extension()
                        .map(|e| e.to_string_lossy().to_string().to_uppercase())
                        .unwrap_or_default(),
                    FontId::proportional(25.),
                    Color32::WHITE,
                );
            }
        }
    }

    let text = icon_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut job = LayoutJob::simple(
        text.clone(),
        FontId::proportional(13.),
        ui.style().visuals.text_color(),
        THUMB_SIZE[0] as f32 * 10.,
    );
    job.halign = Align::Center;

    // the generic hover effect, a rect over everything
    if response.hovered() {
        ui.painter()
            .rect_filled(image_rect, rounding, Color32::from_white_alpha(5));

        let text_pos = image_rect.expand(6.).center_bottom();

        let mut job = LayoutJob::simple(
            text,
            FontId::proportional(13.),
            ui.style().visuals.text_color(),
            THUMB_SIZE[0] as f32,
        );
        job.halign = Align::Center;
        let galley = ui.painter().layout_job(job);
        let painter = ui
            .ctx()
            .layer_painter(LayerId::new(Order::Tooltip, "Folder captions".into()))
            .with_clip_rect(ui.clip_rect());

        let c = ui.style().visuals.extreme_bg_color;
        let mut right_bottom = image_rect.right_bottom();
        right_bottom.y += galley.rect.height() + 14.;
        let r = Rect::from_two_pos(image_rect.left_bottom(), right_bottom).expand(1.);
        painter.rect_filled(r, rounding, c);
        painter.galley(text_pos, galley, Color32::RED);
    } else {
        job.wrap = TextWrapping::truncate_at_width(THUMB_SIZE[0] as f32);
        let galley = ui.painter().layout_job(job);
        ui.painter()
            .galley(image_rect.expand(6.).center_bottom(), galley, Color32::RED);
    }
    response
}
