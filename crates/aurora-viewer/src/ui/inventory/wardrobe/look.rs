//! The picture of a worn item drawn from the user's own avatar as it looks
//! now, framed on the part the item covers (`world::wardrobe::look`).
//! Aurora's own; the Legacy Viewer has no Wardrobe. The app builds the
//! avatar's framings off the frame thread (`app/wardrobe.rs`,
//! `wardrobe_look_tick`) while a worn card is on screen; the cards draw them
//! through the GPU preview pass, without a readback. Every card of one
//! framing and size shares one drawing.
//!
//! The look is also kept: once a look has stayed the same for a moment,
//! each worn item's framing is drawn once more offscreen, read back without
//! waiting for the GPU (`aurora_render::preview::capture`) and saved as the
//! item's picture (`Binding::auto_picture`), in the account's picture
//! folder like the pictures the user chooses. Nothing is uploaded. The item
//! shows it once it is taken off.

use super::pictures;
use crate::world::wardrobe::{self, categories::Tag, look::Shot};
use aurora_render::preview::{Captured, Model, View};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// The avatar's framings, built together from one look.
pub type Looks = HashMap<Shot, Arc<Model>>;

/// A look is kept once it has not changed for this long: the bakes and
/// textures of a fresh outfit arrive over a few seconds.
const SETTLE: Duration = Duration::from_secs(4);

#[derive(Default)]
pub struct Live {
    /// The framings of the look built last, the key of that look and when
    /// it arrived.
    pub looks: Option<Arc<Looks>>,
    key: u64,
    since: Option<Instant>,
    /// The setting (`Preferences::look`), copied each frame.
    pub enabled: bool,
    /// A worn card asked for a picture this frame: the app keeps the look
    /// up to date while it does.
    wanted: Cell<bool>,
    /// The look each item's picture was kept from, this session.
    kept: HashMap<Uuid, u64>,
    keeping: Option<Keeping>,
}

/// The picture of one item on its way to its file.
enum Keeping {
    Drawing {
        item: Uuid,
        key: u64,
        answer: std::sync::mpsc::Receiver<Option<Captured>>,
    },
    Saving {
        item: Uuid,
        key: u64,
        job: pictures::SaveJob,
    },
}

impl Live {
    /// Read and cleared by the app once a frame.
    pub fn take_wanted(&self) -> bool {
        self.wanted.replace(false)
    }

    /// A new look was built: returns the one it replaces.
    pub fn arrived(&mut self, looks: Looks, key: u64) -> Option<Arc<Looks>> {
        self.key = key;
        self.since = Some(Instant::now());
        self.looks.replace(Arc::new(looks))
    }

    /// Once a frame while the Wardrobe shows: keeps the look of the worn
    /// items whose picture was not kept from it yet, one at a time.
    pub fn keep(&mut self, ctx: &egui::Context, p: &crate::theme::Palette, worn: &[(Uuid, Option<Tag>)]) {
        // the look stays up to date while something worn is listed, in
        // sight or not
        if self.enabled && !worn.is_empty() {
            self.wanted.set(true);
        }
        match self.keeping.take() {
            Some(Keeping::Drawing { item, key, answer }) => match answer.try_recv() {
                Ok(Some(captured)) => match image::RgbaImage::from_raw(captured.size[0], captured.size[1], captured.rgba) {
                    Some(image) => {
                        self.keeping = Some(Keeping::Saving {
                            item,
                            key,
                            job: pictures::save(ctx, pictures::Kind::Item, image),
                        });
                    }
                    None => {
                        self.kept.insert(item, key);
                    }
                },
                Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // not drawn (a lost device…): not asked again for this look
                    self.kept.insert(item, key);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.keeping = Some(Keeping::Drawing { item, key, answer });
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
            },
            Some(Keeping::Saving { item, key, job }) => match job.try_recv() {
                Ok(Ok(name)) => {
                    self.kept.insert(item, key);
                    log::info!(
                        "Wardrobe: kept the look of a worn item ({} items kept this session)",
                        self.kept.len()
                    );
                    set_kept(item, name);
                }
                Ok(Err(error)) => {
                    log::warn!("Cannot keep the look of a Wardrobe item: {error}");
                    self.kept.insert(item, key);
                }
                Err(crossbeam_channel::TryRecvError::Empty) => self.keeping = Some(Keeping::Saving { item, key, job }),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.kept.insert(item, key);
                }
            },
            None => {}
        }
        if self.keeping.is_some() || !self.enabled {
            return;
        }
        let (Some(looks), Some(since)) = (&self.looks, self.since) else {
            return;
        };
        let wait = SETTLE.saturating_sub(since.elapsed());
        if !wait.is_zero() {
            ctx.request_repaint_after(wait);
            return;
        }
        let key = self.key;
        let next = worn.iter().find_map(|(item, tag)| {
            let shot = Shot::of(tag.as_ref())?;
            (self.kept.get(item) != Some(&key)).then_some((*item, shot))
        });
        let Some((item, shot)) = next else {
            return;
        };
        let Some(model) = looks.get(&shot).cloned() else {
            self.kept.insert(item, key);
            return;
        };
        self.keeping = Some(Keeping::Drawing {
            item,
            key,
            answer: aurora_render::preview::capture(model, view(shot, p), pictures::SIDE),
        });
        ctx.request_repaint();
    }
}

/// How a framing is seen, on the screen and in a kept picture.
fn view(shot: Shot, p: &crate::theme::Palette) -> View {
    let aim = shot.aim();
    View {
        yaw: aim.yaw,
        pitch: 0.05,
        distance: aim.distance,
        background: p.raised.to_normalized_gamma_f32(),
        ambient: [0.42; 3],
        ..Default::default()
    }
}

/// The item's kept look is the new file; the one it replaces is deleted
/// when no item shows it any more.
fn set_kept(item: Uuid, name: String) {
    let old = wardrobe::with(|shared| {
        shared.wardrobe.change(false, |txn| {
            let binding = txn.binding(item);
            let old = binding.auto_picture.replace(name);
            old.filter(|old| {
                !txn.account()
                    .bindings
                    .values()
                    .any(|b| b.auto_picture.as_ref() == Some(old) || b.picture_file.as_ref() == Some(old))
            })
        })
    });
    if let Some(old) = old {
        pictures::discard(pictures::Kind::Item, &old);
    }
}

/// Draws the look of a worn item in `rect`. Returns false when there is
/// nothing to draw yet (the look is being built, or the item is a HUD).
pub fn paint(ui: &egui::Ui, p: &crate::theme::Palette, rect: egui::Rect, tag: Option<&Tag>, live: &Live) -> bool {
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
    // one drawing per framing and size: the cards of a grid share it
    let size = (rect.width().round() as i32, rect.height().round() as i32);
    let id = egui::Id::unique(("wardrobe-look", shot, size));
    aurora_render::preview::paint_in(ui.painter(), rect, id, model, view(shot, p));
    true
}
