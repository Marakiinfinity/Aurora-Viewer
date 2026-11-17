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
    /// The demo's window width, given once the window exists.
    demo_width: Option<f32>,
    /// The toast of the creators' cards applied one after another: a
    /// delivery of several boxes, or the first login after cards arrived,
    /// says so once.
    batch: Option<Batch>,
}

/// Cards applied in a row share one toast for this long after the last.
const BATCH_WINDOW: Duration = Duration::from_secs(60);

struct Batch {
    notification: u64,
    cards: usize,
    items: usize,
    last: Instant,
}

/// `AURORA_DEMO_WARDROBE=cards|browse|huds|fit|creator[,thin|compact|medium|wide]`
/// (docs/TESTING.md): the scenario, and the width of the window.
fn demo_scenario() -> Option<(String, Option<f32>)> {
    let value = std::env::var("AURORA_DEMO_WARDROBE").ok().filter(|scenario| !scenario.is_empty())?;
    let (scenario, layout) = value.split_once(',').unwrap_or((value.as_str(), ""));
    let width = match layout {
        "thin" => Some(400.0),
        "compact" => Some(600.0),
        "medium" => Some(860.0),
        "wide" => Some(1180.0),
        _ => None,
    };
    Some((scenario.to_owned(), width))
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
        let Some((scenario, width)) = demo_scenario() else { return };
        self.wardrobe_cards.demo_width = width;
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
        if let Some(release) = seeded.release {
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

    /// A notecard's contents arrived. Returns true when the read was the
    /// Wardrobe's own, which no other window waits for.
    pub(crate) fn wardrobe_preview(&mut self, item: Uuid, result: &Result<Vec<u8>, String>) -> bool {
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
