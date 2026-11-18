//! Framing a picture before it is kept (docs/WARDROBE.md, section 10): the
//! picture read from a file or the clipboard shows in a square, dragged to
//! pan and zoomed with the wheel or the slider, and the square is what the
//! card or the store's logo keeps.

use super::*;

/// What the picture is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    /// The picture of a card.
    Item(Uuid),
    /// The logo of the store being edited.
    Logo,
}

impl Target {
    pub(super) fn kind(self) -> pictures::Kind {
        match self {
            Target::Item(_) => pictures::Kind::Item,
            Target::Logo => pictures::Kind::Logo,
        }
    }
}

pub(super) struct Framing {
    pub(super) target: Target,
    image: std::sync::Arc<image::RgbaImage>,
    texture: egui::TextureHandle,
    /// The middle of the frame in the picture, 0 to 1 on each axis.
    center: [f32; 2],
    /// 1 shows the whole shorter side.
    zoom: f32,
}

const MAX_ZOOM: f32 = 6.0;

impl Framing {
    pub(super) fn new(ctx: &egui::Context, target: Target, image: std::sync::Arc<image::RgbaImage>) -> Self {
        Framing {
            target,
            texture: pictures::preview(ctx, &image),
            image,
            center: [0.5, 0.5],
            zoom: 1.0,
        }
    }

    /// The part of the picture the frame shows, as a share of each axis.
    fn span(&self) -> [f32; 2] {
        let (w, h) = (self.image.width().max(1) as f32, self.image.height().max(1) as f32);
        let side = w.min(h) / self.zoom;
        [side / w, side / h]
    }

    fn clamp(&mut self) {
        self.zoom = self.zoom.clamp(1.0, MAX_ZOOM);
        let span = self.span();
        for (center, span) in self.center.iter_mut().zip(span) {
            *center = center.clamp(span / 2.0, 1.0 - span / 2.0);
        }
    }
}

/// What the framing dialog answered.
pub(super) enum Answer {
    Open,
    Cancel,
    /// The square to keep.
    Keep(image::RgbaImage),
}

pub(super) fn show(ctx: &egui::Context, p: &Palette, framing: &mut Framing) -> Answer {
    let mut answer = Answer::Open;
    let modal = widgets::Modal::new(egui::Id::unique("wardrobe_framing")).show(ctx, |ui| {
        ui.set_width(340.0);
        widgets::section_header(ui, p, "photo-filled", aurora_i18n::tr!("wardrobe-frame-title"));
        ui.label(
            egui::RichText::new(aurora_i18n::tr!("wardrobe-frame-body"))
                .size(12.0)
                .color(p.muted),
        );
        ui.add_space(6.0);
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(ui.available_width().min(340.0)), Sense::click_and_drag());
        // drag pans: the picture follows the pointer
        let span = framing.span();
        if response.dragged() {
            let delta = response.drag_delta();
            framing.center[0] -= delta.x / rect.width() * span[0];
            framing.center[1] -= delta.y / rect.height() * span[1];
        }
        // the wheel zooms, over the picture
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                framing.zoom *= (scroll * 0.004).exp();
                ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
            }
        }
        framing.clamp();
        let span = framing.span();
        let uv = Rect::from_min_max(
            egui::pos2(framing.center[0] - span[0] / 2.0, framing.center[1] - span[1] / 2.0),
            egui::pos2(framing.center[0] + span[0] / 2.0, framing.center[1] + span[1] / 2.0),
        );
        let radius = ui.visuals().window_corner_radius;
        ui.painter().rect_filled(rect, radius, p.raised);
        egui::Image::new(&framing.texture)
            .uv(uv)
            .corner_radius(radius)
            .tint(widgets::PICTURE_TINT)
            .paint_at(ui, rect);
        ui.painter()
            .rect_stroke(rect, radius, egui::Stroke::new(1.0, p.violet_light), egui::StrokeKind::Inside);
        response.on_hover_cursor(egui::CursorIcon::Grab);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            widgets::glyph(ui, "search-filled", p.muted, 13.0);
            ui.spacing_mut().slider_width = ui.available_width() - 60.0;
            ui.add(egui::Slider::new(&mut framing.zoom, 1.0..=MAX_ZOOM).show_value(false));
            if widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-frame-reset")).clicked() {
                framing.zoom = 1.0;
                framing.center = [0.5, 0.5];
            }
        });
        ui.add_space(10.0);
        let buttons = [
            widgets::DialogButton {
                icon: "",
                label: aurora_i18n::tr!("common-cancel"),
                kind: widgets::ActionKind::Flat,
                enabled: true,
            },
            widgets::DialogButton {
                icon: "",
                label: aurora_i18n::tr!("wardrobe-frame-keep"),
                kind: widgets::ActionKind::Primary,
                enabled: true,
            },
        ];
        match widgets::dialog_buttons(ui, p, &buttons, 28.0) {
            Some(0) => answer = Answer::Cancel,
            Some(_) => answer = Answer::Keep(pictures::framed(&framing.image, framing.center, framing.zoom)),
            None => {}
        }
    });
    if modal.should_close() && matches!(answer, Answer::Open) {
        answer = Answer::Cancel;
    }
    answer
}
