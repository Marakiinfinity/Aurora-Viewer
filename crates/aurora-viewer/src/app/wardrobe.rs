//! The Wardrobe's side of the app (docs/WARDROBE.md, section 7): its files
//! are opened at login, the Wardrobe cards of the inventory are found and
//! read one at a time, the creator's cards are applied at once with a toast
//! that offers Review and Undo, the others wait for the review sheet.
//! Nothing here is sent to the grid but the read of a notecard the user
//! owns (the same request as opening it).

use super::*;
use crate::world::notifications::{Data, Kind, Response};
use crate::world::wardrobe::{self, Loaded, card, matching, scan, store::CardState};
use uuid::Uuid;

/// LLAssetType::AT_NOTECARD.
const ASSET_NOTECARD: i32 = 7;
/// A read that got no answer is given up after this long.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// The inventory is looked over for cards at most this often.
const SCAN_EVERY: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(crate) struct Cards {
    /// The notecard being read, and since when.
    reading: Option<(Uuid, Instant)>,
    /// Notecards named like a card that are not one, or could not be read.
    not_cards: HashSet<Uuid>,
    scanned: Option<(u64, u64)>,
    scanned_at: Option<Instant>,
    /// The demo's window width, given once the window exists, and the
    /// overlay it opens once the Wardrobe is drawn.
    demo_width: Option<f32>,
    demo_show: Option<String>,
    /// The toast of the creators' cards applied one after another: a
    /// delivery of several boxes, or the first login after cards arrived,
    /// says so once.
    batch: Option<Batch>,
    /// The avatar's framings for the pictures of worn items.
    look: Look,
}

/// The look is captured again at most this often while worn cards show.
const LOOK_EVERY: Duration = Duration::from_millis(500);

/// A look built off the frame thread: its key, the shape it was built
/// with, the snapshot (kept to reuse its decoded meshes and textures) and
/// the framings.
type Built = (
    u64,
    Vec<u8>,
    crate::shape_editor::preview::Snapshot,
    ui::inventory::wardrobe::look::Looks,
);

/// The pictures of worn items (`ui/inventory/wardrobe/look.rs`): the
/// avatar's look is captured like the shape editor's hints
/// (`shape_editor::preview`), decoded and framed on a worker, and built
/// again only when the bakes, the worn meshes, their textures or the shape
/// change.
struct Look {
    snapshot: Option<Arc<crate::shape_editor::preview::Snapshot>>,
    built: Option<(u64, Vec<u8>)>,
    pending: bool,
    checked: Option<Instant>,
    tx: std::sync::mpsc::Sender<Built>,
    rx: std::sync::mpsc::Receiver<Built>,
}

impl Default for Look {
    fn default() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self {
            snapshot: None,
            built: None,
            pending: false,
            checked: None,
            tx,
            rx,
        }
    }
}

/// The framings of one look: the avatar's parts and joints, centred on
/// each framing's joints.
fn frame_look(
    parts: Vec<aurora_render::preview::Part>,
    joints: &[Mat4],
    rig: &crate::scene::anim::Rig,
) -> ui::inventory::wardrobe::look::Looks {
    use crate::world::wardrobe::look::SHOTS;
    SHOTS
        .iter()
        .map(|shot| {
            let aim = shot.aim();
            let points: Vec<Vec3> = aim
                .joints
                .iter()
                .filter_map(|name| rig.names.get(*name).and_then(|j| joints.get(*j)))
                .map(|m| m.w_axis.truncate())
                .collect();
            let center = if points.is_empty() {
                Vec3::Z
            } else {
                points.iter().copied().sum::<Vec3>() / points.len() as f32
            } + Vec3::Z * aim.lift;
            (
                *shot,
                Arc::new(aurora_render::preview::Model {
                    parts: parts.clone(),
                    center,
                    radius: 1.0,
                }),
            )
        })
        .collect()
}

/// Cards applied in a row share one toast for this long after the last.
const BATCH_WINDOW: Duration = Duration::from_secs(60);

struct Batch {
    notification: u64,
    cards: usize,
    items: usize,
    last: Instant,
}

/// `AURORA_DEMO_WARDROBE=cards|browse|huds|fit|creator[,thin|compact|medium|wide[,quick|review|categories]]`
/// (docs/TESTING.md): the scenario, and the width of the window.
fn demo_scenario() -> Option<(String, Option<f32>, Option<String>)> {
    let value = std::env::var("AURORA_DEMO_WARDROBE").ok().filter(|scenario| !scenario.is_empty())?;
    let mut parts = value.split(',');
    let scenario = parts.next().unwrap_or_default();
    let layout = parts.next().unwrap_or_default();
    let show = parts.next().map(str::to_owned);
    let width = match layout {
        "thin" => Some(400.0),
        "compact" => Some(600.0),
        "medium" => Some(860.0),
        "wide" => Some(1180.0),
        _ => None,
    };
    Some((scenario.to_owned(), width, show))
}

/// What the app does with a card once it is read.
enum Outcome {
    Applied(matching::Applied),
    Waiting { by: Uuid, creator: Option<Uuid> },
    Nothing,
}

impl App {
    /// The files of the account that logs in; the demo keeps the Wardrobe
    /// in memory.
    pub(crate) fn wardrobe_login(&mut self, agent: Uuid) {
        self.wardrobe_cards = Cards::default();
        wardrobe::open((!self.demo).then(|| {
            (
                crate::settings::config_dir()
                    .join(wardrobe::store::CATALOGUE_FOLDER)
                    .join(wardrobe::store::CATALOGUE_FILE),
                crate::settings::account_dir(&agent).join(wardrobe::store::ACCOUNT_FILE),
            )
        }));
    }

    /// The offline Wardrobe: made-up products and their cards, the
    /// inventory window open on the Wardrobe mode. `cards` lets the app
    /// read the cards one by one, with their toasts; the other scenarios
    /// start with the creators' cards applied.
    pub(crate) fn demo_wardrobe(&mut self) {
        if !self.demo {
            return;
        }
        let Some((scenario, width, show)) = demo_scenario() else { return };
        self.wardrobe_cards.demo_width = width;
        self.wardrobe_cards.demo_show = show;
        let agent = self.world.agent_id;
        crate::world::inventory::demo::seed(&mut self.world.inventory, agent);
        let seeded = wardrobe::demo::seed(&mut self.world.inventory, agent, &scenario);
        self.world.inventory.generation += 1;
        if let Some(e) = self.world.apply(NetEvent::Names(wardrobe::demo::names())) {
            self.on_app_event(e);
        }
        for (note, text) in &seeded.cards {
            if let Ok(data) = aurora_net::inventory::notecard::Notecard::default().encode(text) {
                self.demo_documents.insert(*note, data);
            }
        }
        // what the scenario shows as worn: the layers join the current
        // outfit, the objects are attached (too small to see), a HUD on a
        // HUD point
        crate::world::appearance::demo_wear(&mut self.world.inventory, agent, &seeded.worn);
        let mut body_point = 0;
        let mut hud_point = 30;
        let objects: Vec<_> = seeded
            .worn
            .iter()
            .filter_map(|id| self.world.inventory.items.get(id))
            .filter(|item| item.asset_type == 6)
            .map(|item| {
                let point = if wardrobe::matching::names_hud(&item.name) {
                    hud_point += 1;
                    hud_point
                } else {
                    body_point += 1;
                    body_point
                };
                let mut object = crate::demo::hud::object(point, false);
                object.full_id = Uuid::from_u128(item.id.as_u128() + 0x9_0000);
                object.scale = Vec3::splat(0.001);
                object.name_values = format!("AttachItemID STRING RW SV {}", item.id);
                object
            })
            .collect();
        if let Some(e) = self.world.apply(NetEvent::ObjectUpdates {
            handle: crate::demo::HANDLE,
            objects,
        }) {
            self.on_app_event(e);
        }
        if scenario != "cards" {
            wardrobe::with(|shared| wardrobe::demo::apply(shared, &self.world.inventory, &seeded));
        }
        self.settings.inventory.wardrobe.open = true;
        self.settings.classic_inventory = false;
        self.panels.inventory = true;
        if let Some(release) = seeded.release.filter(|_| wardrobe::CARDS) {
            self.inventory_ui.wardrobe.creator.choose(&self.world.inventory, release, agent);
        }
    }

    /// Every frame: takes the files once read, remembers the worn HUDs,
    /// looks for cards to read and applies those that found their items.
    pub(crate) fn wardrobe_tick(&mut self) {
        if self.world.agent_id.is_nil() {
            return;
        }
        if self.frame_count > 4
            && let Some(width) = self.wardrobe_cards.demo_width.take()
        {
            ui::widgets::size_floater(&self.egui_ctx, "inventory", egui::vec2(width, 640.0));
        }
        // once the cards are read and the Wardrobe is drawn
        if self.frame_count > 240
            && let Some(show) = self.wardrobe_cards.demo_show.take()
        {
            self.inventory_ui.wardrobe.demo_show(&show, &self.world.inventory);
        }
        let ready = wardrobe::with(|shared| {
            shared.wardrobe.poll();
            shared.wardrobe.ready()
        });
        if !ready {
            return;
        }
        if self.wardrobe_cards.reading.is_some_and(|(_, since)| since.elapsed() > READ_TIMEOUT)
            && let Some((item, _)) = self.wardrobe_cards.reading.take()
        {
            self.wardrobe_cards.not_cards.insert(item);
        }
        let revision = wardrobe::with(|shared| shared.wardrobe.revision);
        let key = (self.world.inventory.generation, revision);
        if self.wardrobe_cards.scanned == Some(key) || self.wardrobe_cards.scanned_at.is_some_and(|at| at.elapsed() < SCAN_EVERY) {
            return;
        }
        self.wardrobe_cards.scanned = Some(key);
        self.wardrobe_cards.scanned_at = Some(Instant::now());
        self.wardrobe_remember_huds();
        // a renamed or redelivered copy joins its product by its asset
        wardrobe::with(|shared| {
            if shared.wardrobe.catalogue.products.values().any(|p| !p.asset_ids.is_empty()) {
                let items = scan::eligible(&self.world.inventory);
                shared.wardrobe.change(false, |txn| matching::rebind_by_asset(txn, &items));
            }
        });
        if !wardrobe::CARDS {
            return;
        }
        // the cards already read whose items were not there yet
        let waiting: Vec<Uuid> = wardrobe::with(|shared| {
            shared
                .cards
                .keys()
                .filter(|id| !shared.wardrobe.account.cards.contains_key(*id))
                .copied()
                .collect()
        });
        for item in waiting {
            self.wardrobe_card_ready(item);
        }
        if self.wardrobe_cards.reading.is_some() || !self.world.inventory.can_preview(ASSET_NOTECARD) {
            return;
        }
        let next = wardrobe::with(|shared| {
            scan::card_notecards(&self.world.inventory)
                .into_iter()
                .find(|id| {
                    !shared.cards.contains_key(id)
                        && !shared.wardrobe.account.cards.contains_key(id)
                        && !self.wardrobe_cards.not_cards.contains(id)
                })
                .and_then(|id| self.world.inventory.items.get(&id).cloned())
        });
        if let Some(item) = next {
            self.wardrobe_cards.reading = Some((item.id, Instant::now()));
            self.send(NetCommand::PreviewInventoryItem(item));
        }
    }

    /// The objects worn on a HUD attachment point are remembered: rule 4 of
    /// the HUD rules, and what Auto-sort files under HUDs.
    fn wardrobe_remember_huds(&mut self) {
        let worn: Vec<Uuid> = self
            .world
            .worn_attachment_points()
            .into_iter()
            .filter(|(_, point)| wardrobe::huds::hud_point(*point))
            .map(|(item, _)| item)
            .collect();
        if worn.is_empty() {
            return;
        }
        wardrobe::with(|shared| {
            if worn.iter().any(|id| !shared.wardrobe.account.worn_huds.contains(id)) {
                shared.wardrobe.change(false, |txn| txn.account_mut().worn_huds.extend(worn));
            }
        });
    }

    /// Keeps the avatar's framings up to date while a worn card of the
    /// Wardrobe is on screen (`ui/inventory/wardrobe/look.rs`).
    pub(crate) fn wardrobe_look_tick(&mut self) {
        let live = &mut self.inventory_ui.wardrobe.live;
        while let Ok((key, values, snapshot, looks)) = self.wardrobe_cards.look.rx.try_recv() {
            self.wardrobe_cards.look.pending = false;
            let old = self.wardrobe_cards.look.snapshot.replace(Arc::new(snapshot));
            self.scene.jobs.discard(old);
            // the shape is part of the look a picture is kept from
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(&(key, &values), &mut hasher);
            let old = live.arrived(looks, std::hash::Hasher::finish(&hasher));
            self.scene.jobs.discard(old);
            self.wardrobe_cards.look.built = Some((key, values));
        }
        if !live.take_wanted() || self.world.agent_id.is_nil() {
            return;
        }
        let look = &mut self.wardrobe_cards.look;
        if look.pending || look.checked.is_some_and(|at| at.elapsed() < LOOK_EVERY) {
            return;
        }
        look.checked = Some(Instant::now());
        let input = crate::shape_editor::preview::Input::capture(&self.world, &self.scene);
        let values = self.world.server_visual_params().to_vec();
        if look
            .built
            .as_ref()
            .is_some_and(|(key, built)| *key == input.key && *built == values)
        {
            self.inventory_ui.wardrobe.live.confirm();
            return;
        }
        look.pending = true;
        let key = input.key;
        let previous = look.snapshot.clone();
        let lib = self.world.avatar_lib.clone();
        let tx = look.tx.clone();
        let ctx = self.egui_ctx.clone();
        self.scene.jobs.run(move || {
            let snapshot = input.prepare(previous);
            let (parts, joints) = snapshot.build(&lib, &values, &Default::default());
            let looks = frame_look(parts, &joints, &lib.rig);
            let _ = tx.send((key, values, snapshot, looks));
            ctx.request_repaint();
        });
    }

    /// A notecard's contents arrived. Returns true when the read was the
    /// Wardrobe's own, which no other window waits for.
    pub(crate) fn wardrobe_preview(&mut self, item: Uuid, result: &Result<Vec<u8>, String>) -> bool {
        if !wardrobe::CARDS {
            return false;
        }
        let own = self.wardrobe_cards.reading.is_some_and(|(id, _)| id == item);
        if own {
            // the next card is read at once
            self.wardrobe_cards.reading = None;
            self.wardrobe_cards.scanned_at = None;
        }
        let Some(notecard) = self.world.inventory.items.get(&item).filter(|it| it.asset_type == ASSET_NOTECARD) else {
            return own;
        };
        let creator = notecard.creator;
        let read = result
            .as_ref()
            .ok()
            .and_then(|bytes| aurora_net::inventory::notecard::Notecard::decode(bytes).ok())
            .and_then(|note| {
                let parsed = card::parse(&note.text)?;
                // only what the next owner may copy, modify and transfer
                let embedded = note
                    .embedded()
                    .filter(|it| crate::world::inventory::actions::wardrobe_embedded_kept(it))
                    .map(|it| (it.name.clone(), it.asset_id))
                    .collect();
                Some(Loaded {
                    card: parsed,
                    creator,
                    embedded,
                })
            });
        match read {
            Some(loaded) => {
                // a notecard opened by the user that turns out to be a card
                // is taken the same way, and read again when it changed
                wardrobe::with(|shared| shared.cards.insert(item, loaded));
                self.wardrobe_cards.scanned = None;
                self.wardrobe_card_ready(item);
            }
            None if own => {
                self.wardrobe_cards.not_cards.insert(item);
            }
            None => {}
        }
        own
    }

    /// A card that is read meets the items of its folder: applied when the
    /// creator wrote it, announced as waiting otherwise.
    fn wardrobe_card_ready(&mut self, item: Uuid) {
        let Some(folder) = self.world.inventory.items.get(&item).map(|it| it.parent) else {
            return;
        };
        let folder_name = self
            .world
            .inventory
            .folders
            .get(&folder)
            .map(|f| f.info.name.clone())
            .unwrap_or_default();
        let scope = scan::card_scope(&self.world.inventory, folder);
        let outcome = wardrobe::with(|shared| {
            let Some(loaded) = shared.cards.get(&item) else {
                return Outcome::Nothing;
            };
            if shared.wardrobe.account.cards.contains_key(&item) {
                return Outcome::Nothing;
            }
            let plan = matching::plan(&loaded.card, item, loaded.creator, &folder_name, &scope, &shared.wardrobe.catalogue);
            if plan.item_count() == 0 {
                // the box is not unpacked yet: looked at again when the
                // inventory changes
                return Outcome::Nothing;
            }
            if plan.trusted {
                let (card, embedded) = (loaded.card.clone(), loaded.embedded.clone());
                Outcome::Applied(
                    shared
                        .wardrobe
                        .change(true, |txn| matching::apply(txn, &card, &plan, matching::Groups::ALL, &embedded)),
                )
            } else {
                let by = loaded.creator;
                shared.wardrobe.change(false, |txn| txn.set_card(item, CardState::Waiting));
                Outcome::Waiting {
                    by,
                    creator: plan.items_creator,
                }
            }
        });
        match outcome {
            Outcome::Applied(done) => {
                // cards applied in a row: the toast of the first counts them
                let listed = self.wardrobe_cards.batch.as_ref().is_some_and(|b| {
                    b.last.elapsed() < BATCH_WINDOW && self.world.notifications.list.iter().any(|n| n.id == b.notification)
                });
                if listed && let Some(batch) = self.wardrobe_cards.batch.as_mut() {
                    batch.cards += 1;
                    batch.items += done.added + done.updated;
                    batch.last = Instant::now();
                    let (id, cards, items) = (batch.notification, batch.cards, batch.items);
                    if let Some(n) = self.world.notifications.list.iter_mut().find(|n| n.id == id) {
                        n.title = aurora_i18n::tr!("wardrobe-cards-title", cards = cards);
                        n.data = Data::WardrobeCard { card: item, applied: true };
                    }
                    self.world
                        .notifications
                        .update_body(id, aurora_i18n::tr!("wardrobe-cards-applied", count = items));
                    return;
                }
                let title = done.store.clone().unwrap_or_else(|| done.pack.clone());
                let mut body = if done.added > 0 {
                    aurora_i18n::tr!("wardrobe-card-applied", count = done.added)
                } else {
                    aurora_i18n::tr!("wardrobe-card-updated", count = done.updated)
                };
                if done.replaced_user_store {
                    body.push(' ');
                    body.push_str(aurora_i18n::tr!("wardrobe-card-replaced-store"));
                }
                let notification =
                    self.world
                        .notifications
                        .push(Kind::Inventory, title, body, Data::WardrobeCard { card: item, applied: true });
                self.wardrobe_cards.batch = Some(Batch {
                    notification,
                    cards: 1,
                    items: done.added + done.updated,
                    last: Instant::now(),
                });
            }
            Outcome::Waiting { by, creator } => {
                self.world.social.want_name(by);
                if let Some(creator) = creator {
                    self.world.social.want_name(creator);
                }
                self.world.notifications.push(
                    Kind::Inventory,
                    aurora_i18n::tr!("wardrobe-card-waiting-title"),
                    aurora_i18n::tr!("wardrobe-card-waiting"),
                    Data::WardrobeCard {
                        card: item,
                        applied: false,
                    },
                );
            }
            Outcome::Nothing => {}
        }
    }

    /// The answer to a card's toast: Review opens the sheet, the other
    /// button undoes an applied card or ignores a waiting one. Returns true
    /// when the notification was a card's.
    pub(crate) fn wardrobe_answer(&mut self, id: u64, response: &Response) -> bool {
        let Some((card, applied)) = self.world.notifications.list.iter().find_map(|n| match n.data {
            Data::WardrobeCard { card, applied } if n.id == id => Some((card, applied)),
            _ => None,
        }) else {
            return false;
        };
        // the cards one toast counted: Review shows what is new in the
        // Wardrobe, Undo takes every one of them back
        let several = self
            .wardrobe_cards
            .batch
            .as_ref()
            .filter(|b| b.notification == id && b.cards > 1)
            .map(|b| b.cards);
        match response {
            Response::Open if several.is_some() => {
                self.panels.inventory = true;
                self.settings.inventory.wardrobe.open = true;
                self.inventory_ui.wardrobe.filter.new = true;
            }
            Response::Open => {
                self.panels.inventory = true;
                self.inventory_ui.wardrobe.review_card(card);
            }
            Response::Decline if applied => {
                wardrobe::with(|shared| {
                    for _ in 0..several.unwrap_or(1) {
                        shared.wardrobe.undo();
                    }
                });
                self.wardrobe_cards.batch = None;
            }
            Response::Decline => {
                wardrobe::with(|shared| shared.wardrobe.change(false, |txn| txn.set_card(card, CardState::Ignored)));
            }
            _ => {}
        }
        let _ = self.world.respond_notification(id, Response::Dismiss);
        true
    }
}
