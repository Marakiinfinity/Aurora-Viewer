//! The picture of a worn item drawn from the user's own avatar as it looks
//! now, framed on the part the item covers (`world::wardrobe::look`).
//! Aurora's own; the Legacy Viewer has no Wardrobe. The app builds the
//! avatar's framings off the frame thread (`app/wardrobe.rs`,
//! `wardrobe_look_tick`) while a worn card is on screen; the cards draw them
//! through the GPU preview pass, without a readback. Every card of one
//! framing and size shares one drawing.

use crate::world::wardrobe::look::Shot;
use aurora_render::preview::{Model, View};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;

/// The avatar's framings, built together from one look.
pub type Looks = HashMap<Shot, Arc<Model>>;

#[derive(Default)]
pub struct Live {
    /// The framings of the look built last.
    pub looks: Option<Arc<Looks>>,
    /// The setting (`Preferences::look`), copied each frame.
    pub enabled: bool,
    /// A worn card asked for a picture this frame: the app keeps the look
    /// up to date while it does.
    wanted: Cell<bool>,
}

impl Live {
    /// Read and cleared by the app once a frame.
    pub fn take_wanted(&self) -> bool {
        self.wanted.replace(false)
    }
}

/// Draws the look of a worn item in `rect`. Returns false when there is
/// nothing to draw yet (the look is being built, or the item is a HUD).
pub fn paint(
    ui: &egui::Ui,
    p: &crate::theme::Palette,
    rect: egui::Rect,
    tag: Option<&crate::world::wardrobe::categories::Tag>,
    live: &Live,
) -> bool {
    if !live.enabled {
        return false;
    }
    let Some(shot) = Shot::of(tag) else {
        return false;
    };
    live.wanted.set(true);
    let Some(model) = live.looks.as_ref().and_then(|looks| looks.get(&shot)).cloned() else {
        return false;
    };
    let aim = shot.aim();
    let view = View {
        yaw: aim.yaw,
        pitch: 0.05,
        distance: aim.distance,
        background: p.raised.to_normalized_gamma_f32(),
        ambient: [0.42; 3],
        ..Default::default()
    };
    // one drawing per framing and size: the cards of a grid share it
    let size = (rect.width().round() as i32, rect.height().round() as i32);
    let id = egui::Id::unique(("wardrobe-look", shot, size));
    aurora_render::preview::paint_in(ui.painter(), rect, id, model, view);
    true
}
