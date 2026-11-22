//! The Wardrobe mode of Aurora's inventory window (docs/WARDROBE.md,
//! section 10): a rail of categories, cards with a picture, filter chips,
//! Browse by, a search with scopes, the selection bar, Quick Look and Try
//! on. It shows the local Wardrobe (`world/wardrobe`), which never leaves
//! the PC. Aurora's own feature; wearing goes through the inventory's own
//! actions, with the same rules as its menus (`context_catalog.rs`).
//! Its parts: the tiles and what a filter keeps (`model`), the grid
//! (`grid`), the review sheet of a card (`review`), the small dialogs
//! (`dialogs`), Quick Look (`quick_look`), the item tooltip (`tips`) and
//! the creator tool (`creator`).

pub mod creator;
mod dialogs;
mod framing;
mod grid;
pub mod model;
mod pictures;
mod quick_look;
pub mod review;
mod stores;
pub mod tips;

use super::*;
use crate::ui::{menu, widgets};
use crate::world::appearance::Action as Wear;
use crate::world::wardrobe::{self, categories, fit::Fit, sort, store::SortBy};
use egui::{Rect, Sense, Vec2};
use model::{Browse, Created, Filter, Hit, Model, Rail, Scope, Tile};

/// Body widths from which the Wardrobe changes its layout.
const COMPACT: f32 = 420.0;
const MEDIUM: f32 = 700.0;
const WIDE: f32 = 1020.0;
/// The side of a tile's picture, in points (Ctrl + wheel).
pub const TILE_MIN: u16 = 72;
pub const TILE_MAX: u16 = 200;
const NOTE_SECONDS: u64 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Width {
    Thin,
    Compact,
    Medium,
    Wide,
}

impl Width {
    fn of(width: f32) -> Self {
        if width >= WIDE {
            Width::Wide
        } else if width >= MEDIUM {
            Width::Medium
        } else if width >= COMPACT {
            Width::Compact
        } else {
            Width::Thin
        }
    }
}

/// The Wardrobe's own settings, kept with the inventory's.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// The inventory window shows the Wardrobe rather than the list.
    pub open: bool,
    /// The copies of one product show as one card.
    pub stacks: bool,
    pub tile: u16,
    /// The item card tooltip, and the one-line hints on buttons and chips.
    pub tips: bool,
    pub hints: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            open: false,
            stacks: true,
            tile: 112,
            tips: true,
            hints: false,
        }
    }
}

/// A line over the selection bar: what just happened, with Undo.
struct Note {
    text: String,
    undo: bool,
    at: std::time::Instant,
}

/// An item worn to try it on.
struct Trying {
    item: Uuid,
    /// What was worn before the try.
    before: HashSet<Uuid>,
    /// The item was taken off again: what it replaced goes back on once
    /// the outfit settles.
    reverting: bool,
    /// The item was seen worn, and when the try began: taking it off by
    /// hand reverts the try, and one whose item was never worn ends by
    /// itself.
    seen_worn: bool,
    started: std::time::Instant,
}

/// A try whose item never shows as worn is given up after this long.
const TRY_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Default)]
pub struct State {
    pub model: Model,
    pub filter: Filter,
    visible: Vec<(usize, Hit)>,
    visible_key: Option<(u64, Filter, SortBy, bool, usize)>,
    sort: SortBy,
    reversed: bool,
    pub selected: Option<Uuid>,
    /// The stack opened in place: its copies show one card each.
    open_stack: Option<Uuid>,
    quick_look: Option<Uuid>,
    /// Quick Look opened this frame: the Space that opened it does not
    /// close it.
    quick_opened_now: bool,
    try_on: bool,
    trying: Option<Trying>,
    note: Option<Note>,
    review: Option<review::Review>,
    /// A card whose review was asked from a toast or a menu.
    pending_review: Option<Uuid>,
    dialog: Option<dialogs::Dialog>,
    pub creator: creator::State,
    /// Creators whose name the window shows: asked of the app.
    pub wanted_names: HashSet<Uuid>,
    /// An arrow key moved the selection this frame: the grid scrolls to it.
    moved_by_key: bool,
    /// "Show in Wardrobe": the item to show once the tiles are built, and
    /// the grid scrolling to the selection once it did.
    pending_show: Option<Uuid>,
    reveal: bool,
    /// A picture being chosen for an item, from a file or the clipboard.
    picture_job: Option<(framing::Target, pictures::Job)>,
    /// The picture read, being framed; then the framed one being saved.
    framing: Option<framing::Framing>,
    save_job: Option<(framing::Target, pictures::SaveJob)>,
    /// The store being made or edited, and the list of stores.
    store_edit: Option<stores::Edit>,
    stores: stores::List,
    /// The card whose right-click menu is open, and where.
    menu: Option<(Uuid, egui::Pos2)>,
    /// The names of the copies of an open stack, by item.
    copy_names: std::collections::HashMap<Uuid, String>,
    /// "Show original in the inventory": the window goes back to its list.
    pub leave: bool,
    /// A creator's profile was asked: opened by the app.
    pub profile: Option<Uuid>,
}

impl State {
    /// Opens the review sheet of a card at the next frame.
    pub fn review_card(&mut self, card: Uuid) {
        self.pending_review = Some(card);
    }

    /// "Show in Wardrobe", from a menu of the list or of the world.
    pub fn show(&mut self, item: Uuid) {
        self.pending_show = Some(item);
    }

    pub fn wants_to_show(&self) -> bool {
        self.pending_show.is_some()
    }

    pub fn forget_show(&mut self) {
        self.pending_show = None;
    }

    /// "Add to Wardrobe…" from the list: the categories dialog, opened on
    /// what the item's type or name already tells.
    pub fn add_by_hand(&mut self, inv: &Inventory, item: Uuid) {
        let guess: Vec<categories::Tag> = inv
            .items
            .get(&item)
            .and_then(|it| sort::place(it, false, false))
            .into_iter()
            .collect();
        self.dialog = Some(dialogs::Dialog::categories(item, &guess));
    }

    /// The review sheet is open, in whatever style the window has: its
    /// creators' names are asked.
    pub fn shows_names(&self) -> bool {
        self.review.is_some() || self.pending_review.is_some() || self.store_edit.is_some()
    }

    /// `AURORA_DEMO_WARDROBE=…,…,quick|review|categories|register|store|stores`: one of the
    /// Wardrobe's overlays opened for a capture.
    pub fn demo_show(&mut self, what: &str, inv: &Inventory) {
        let first = self.visible.first().map(|(n, _)| self.model.tiles[*n].clone());
        match (what, first) {
            ("quick", Some(tile)) => {
                self.selected = Some(tile.item);
                self.open_quick_look(tile.item);
            }
            ("categories", Some(tile)) => self.dialog = Some(dialogs::Dialog::categories(tile.item, &tile.tags)),
            ("register", Some(tile)) => {
                let detected = inv.items.get(&tile.item).and_then(wardrobe::fit::preset_of);
                self.dialog = Some(dialogs::Dialog::register(tile.item, categories::Part::Body, &tile.kit, detected));
            }
            ("store", Some(tile)) => self.store_edit = Some(stores::Edit::for_item(tile.item, tile.creator, tile.store_key)),
            ("stores", _) => self.stores.open = true,
            ("review", _) => {
                // the card written by someone else, else any card
                let cards = wardrobe::scan::card_notecards(inv);
                let waiting = wardrobe::with(|shared| {
                    cards
                        .iter()
                        .copied()
                        .find(|id| matches!(shared.wardrobe.account.cards.get(id), Some(wardrobe::store::CardState::Waiting)))
                });
                if let Some(card) = waiting.or(cards.first().copied()) {
                    self.review_card(card);
                }
            }
            _ => {}
        }
    }

    fn open_quick_look(&mut self, item: Uuid) {
        self.quick_look = Some(item);
        self.quick_opened_now = true;
    }

    fn say(&mut self, text: String, undo: bool) {
        self.note = Some(Note {
            text,
            undo,
            at: std::time::Instant::now(),
        });
    }

    /// The tile of the selection.
    fn chosen(&self) -> Option<&Tile> {
        self.selected.and_then(|id| self.model.tile_of(&id))
    }
}

/// The icon of a category, the same in the rail, the tiles and the menus.
pub(super) fn category_icon(key: &str) -> &'static str {
    match key {
        "tops" => "shirt-filled",
        "bottoms" => "walk",
        "full-body" => "hanger-2-filled",
        "shoes" => "shoe",
        "hair" => "scissors",
        "heads" => "mood-smile-filled",
        "body-parts" => "man-filled",
        "accessories" => "diamond-filled",
        "huds" => "adjustments-filled",
        _ => "tag-filled",
    }
}

/// "T-shirt", "Shirt + Jacket": the subtypes of a tag as the interface
/// names them, the category alone when the subtype is Other.
pub(super) fn tag_label(tag: &categories::Tag) -> String {
    let named: Vec<&str> = tag
        .1
        .iter()
        .filter(|s| s.as_str() != categories::OTHER)
        .map(|s| categories::subtype_label(tag.category(), s))
        .collect();
    if named.is_empty() {
        categories::category_label(tag.category()).to_owned()
    } else {
        named.join(" + ")
    }
}

/// The line that says what an item was created for.
pub(super) fn fit_text(fit: &Fit) -> String {
    match fit {
        Fit::Fits(kit) => aurora_i18n::tr!("wardrobe-fit-fits", kit = kit.as_str()),
        Fit::Everyone => aurora_i18n::tr!("wardrobe-fit-everyone").to_owned(),
        Fit::NotFor {
            kit, nothing_worn: true, ..
        } => aurora_i18n::tr!("wardrobe-fit-needs", kit = kit.as_str()),
        Fit::NotFor { kit, .. } => aurora_i18n::tr!("wardrobe-fit-made-for", kit = kit.as_str()),
        Fit::Unknown(kit) => aurora_i18n::tr!("wardrobe-fit-unknown", kit = kit.as_str()),
    }
}

/// Whether the inventory's rules let `tile` be worn or taken off now: the
/// same conditions as the Wear and Take off rows of its menu
/// (`context_catalog.rs`, LLWearableBridge / LLObjectBridge::buildContextMenu).
pub(super) fn can_wear(inv: &Inventory, facts: &Facts, item: Uuid) -> bool {
    !facts.appearance_busy
        && !facts.worn.contains(&item)
        && inv.items.get(&item).is_some_and(|it| matches!(it.asset_type, 5 | 6 | 13))
        && facts.rlva.wear(inv, item, 0).replace
}

pub(super) fn can_add(inv: &Inventory, facts: &Facts, item: Uuid) -> bool {
    !facts.appearance_busy
        && !facts.worn.contains(&item)
        && inv.items.get(&item).is_some_and(|it| matches!(it.asset_type, 5 | 6))
        && facts.rlva.wear(inv, item, 0).add
}

pub(super) fn can_take_off(inv: &Inventory, facts: &Facts, item: Uuid) -> bool {
    !facts.appearance_busy
        && facts.worn.contains(&item)
        && inv.items.get(&item).is_some_and(|it| matches!(it.asset_type, 5 | 6))
        && facts.rlva.can_remove(item)
}

/// Wears `item` in place of what holds its slot, and notes the wear in the
/// Wardrobe (last worn, no longer new). A fit mismatch is said, never
/// refused.
pub(super) fn wear(st: &mut InventoryUi, inv: &Inventory, facts: &Facts, item: Uuid, replace: bool, actions: &mut Vec<InvAction>) {
    let allowed = if replace {
        can_wear(inv, facts, item)
    } else {
        can_add(inv, facts, item)
    };
    if !allowed {
        return;
    }
    actions.push(InvAction::Appearance(Wear::WearItem { item, replace, point: 0 }));
    wardrobe::with(|shared| {
        shared.wardrobe.change(false, |txn| {
            if txn.account().bindings.contains_key(&item) {
                let binding = txn.binding(item);
                binding.last_worn = wardrobe::now();
                binding.new = false;
            }
        });
    });
    if let Some(Fit::NotFor { kit, part, nothing_worn }) = st.wardrobe.model.tile_of(&item).map(|t| t.fit.clone()) {
        let worn = wardrobe::with(|shared| shared.index.kits.worn().find(|k| k.part == part).map(|k| k.name.clone()));
        let text = match worn.filter(|_| !nothing_worn) {
            Some(worn) => aurora_i18n::tr!("wardrobe-fit-notice", kit = kit.as_str(), worn = worn.as_str()),
            None => aurora_i18n::tr!("wardrobe-fit-notice-none", kit = kit.as_str()),
        };
        st.wardrobe.say(text, false);
    }
}

fn take_off(inv: &Inventory, facts: &Facts, item: Uuid, actions: &mut Vec<InvAction>) {
    if can_take_off(inv, facts, item) {
        actions.push(InvAction::Appearance(Wear::RemoveItems(vec![item])));
    }
}

/// HUDs are added, never worn in place of another
/// (LLAppearanceMgr::wearItemOnAvatar with replace false,
/// indra/newview/llappearancemgr.cpp): one already on the same screen
/// point stays. Returns how many could not be added.
fn wear_huds(inv: &Inventory, facts: &Facts, huds: &[Uuid], actions: &mut Vec<InvAction>) -> usize {
    let wanted: Vec<Uuid> = huds.iter().copied().filter(|id| !facts.worn.contains(id)).collect();
    let allowed: Vec<Uuid> = wanted.iter().copied().filter(|id| can_add(inv, facts, *id)).collect();
    let refused = wanted.len() - allowed.len();
    match allowed.as_slice() {
        [] => {}
        [one] => actions.push(InvAction::Appearance(Wear::WearItem {
            item: *one,
            replace: false,
            point: 0,
        })),
        _ => actions.push(InvAction::Appearance(Wear::WearItems {
            items: allowed,
            replace: false,
            point: 0,
        })),
    }
    refused
}

fn detach_huds(inv: &Inventory, facts: &Facts, huds: &[Uuid], actions: &mut Vec<InvAction>) {
    let worn: Vec<Uuid> = huds.iter().copied().filter(|id| can_take_off(inv, facts, *id)).collect();
    if !worn.is_empty() {
        actions.push(InvAction::Appearance(Wear::RemoveItems(worn)));
    }
}

/// The Wardrobe mode: the layout of its width.
#[allow(clippy::too_many_arguments)]
pub(super) fn contents(
    ui: &mut egui::Ui,
    p: &Palette,
    inv: &mut Inventory,
    st: &mut InventoryUi,
    prefs: &mut InventoryPreferences,
    facts: &Facts,
    images: &std::collections::HashMap<Uuid, egui::TextureHandle>,
    actions: &mut Vec<InvAction>,
) {
    let width = Width::of(ui.available_width());
    let ready = wardrobe::with(|shared| {
        if !shared.wardrobe.ready() {
            return false;
        }
        shared.index.update(&shared.wardrobe, inv, &facts.worn);
        st.wardrobe.model.update(shared, inv, &facts.worn, prefs.wardrobe.stacks);
        st.wardrobe.sort = shared.wardrobe.account.view.sort;
        st.wardrobe.reversed = shared.wardrobe.account.view.reversed;
        true
    });
    if !ready {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(egui::RichText::new(aurora_i18n::tr!("wardrobe-loading")).color(p.muted)));
        return;
    }
    try_on_step(st, inv, facts, actions);
    refresh_visible(st, facts);
    // a selection that left the Wardrobe is dropped
    // "Show in Wardrobe": every filter aside, the item selected and in sight
    if let Some(item) = st.wardrobe.pending_show.take() {
        match st.wardrobe.model.tile_of(&item).map(|tile| (tile.item, tile.copies.len())) {
            Some((main, copies)) => {
                let w = &mut st.wardrobe;
                w.filter = Filter::default();
                w.open_stack = (copies > 1 && main != item).then_some(main);
                w.selected = Some(item);
                w.reveal = true;
                w.visible_key = None;
                refresh_visible(st, facts);
            }
            None => st.wardrobe.say(aurora_i18n::tr!("wardrobe-show-missing").to_owned(), false),
        }
    }
    if st.wardrobe.selected.is_some_and(|id| st.wardrobe.model.tile_of(&id).is_none()) {
        st.wardrobe.selected = None;
    }
    let area = ui.available_rect_before_wrap();
    const GAP: f32 = 8.0;
    let rail_w = match width {
        Width::Thin => 0.0,
        Width::Compact => 40.0,
        Width::Medium | Width::Wide => 178.0,
    };
    let pane_w = if width == Width::Wide && prefs.show_details { 280.0 } else { 0.0 };
    let rail = Rect::from_min_size(area.min, Vec2::new(rail_w, area.height()));
    let pane = Rect::from_min_max(egui::pos2(area.right() - pane_w, area.top()), area.max);
    let content = Rect::from_min_max(
        egui::pos2(if rail_w > 0.0 { rail.right() + GAP } else { area.left() }, area.top()),
        egui::pos2(if pane_w > 0.0 { pane.left() - GAP } else { area.right() }, area.bottom()),
    );
    let child = |ui: &mut egui::Ui, rect: Rect, salt: &str| {
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .id_salt(salt)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        child
    };
    if rail_w > 0.0 {
        let mut rail_ui = child(ui, rail, "wardrobe_rail");
        rail_column(&mut rail_ui, p, st, facts, width);
    }
    let mut content_ui = child(ui, content, "wardrobe_content");
    middle(&mut content_ui, p, inv, st, prefs, facts, images, actions, width);
    if pane_w > 0.0 {
        let mut pane_ui = child(ui, pane, "wardrobe_details");
        details(&mut pane_ui, p, inv, st, facts, images, actions);
    }
    ui.advance_cursor_after_rect(area);
    quick_look::show(ui.ctx(), p, inv, st, facts, images, actions);
}

/// The review sheet and the dialogs, drawn whatever the window shows: the
/// list's menu and a card's toast open them too.
pub(super) fn overlays(
    ctx: &egui::Context,
    p: &Palette,
    inv: &Inventory,
    st: &mut InventoryUi,
    facts: &Facts,
    images: &std::collections::HashMap<Uuid, egui::TextureHandle>,
) {
    review::show(ctx, p, inv, st, facts, images);
    dialogs::show(ctx, p, inv, st, images);
    stores::list(ctx, p, st, images);
    stores::sheet(ctx, p, st, facts, images);
    pictures_step(ctx, p, st);
}

/// A picture on its way: read from a file or the clipboard, framed by the
/// user, saved, then given to the card or to the store being edited.
fn pictures_step(ctx: &egui::Context, p: &Palette, st: &mut InventoryUi) {
    if let Some((target, job)) = &st.wardrobe.picture_job
        && let Ok(read) = job.try_recv()
    {
        let target = *target;
        st.wardrobe.picture_job = None;
        match read {
            Ok(Some(image)) => st.wardrobe.framing = Some(framing::Framing::new(ctx, target, image)),
            Ok(None) => {}
            Err(reason) => st.wardrobe.say(reason, false),
        }
    }
    if let Some(open) = &mut st.wardrobe.framing {
        match framing::show(ctx, p, open) {
            framing::Answer::Open => {}
            framing::Answer::Cancel => st.wardrobe.framing = None,
            framing::Answer::Keep(image) => {
                let target = open.target;
                st.wardrobe.save_job = Some((target, pictures::save(ctx, target.kind(), image)));
                st.wardrobe.framing = None;
            }
        }
    }
    if let Some((target, job)) = &st.wardrobe.save_job
        && let Ok(saved) = job.try_recv()
    {
        let target = *target;
        st.wardrobe.save_job = None;
        match (saved, target) {
            (Ok(file), framing::Target::Item(item)) => {
                dialogs::set_picture(item, None, Some(file));
                if matches!(st.wardrobe.dialog, Some(dialogs::Dialog::Picture { .. })) {
                    st.wardrobe.dialog = None;
                }
            }
            (Ok(file), framing::Target::Logo) => match &mut st.wardrobe.store_edit {
                Some(edit) => edit.logo_file = Some(file),
                // the sheet was closed meanwhile: the file is nobody's
                None => pictures::discard(pictures::Kind::Logo, &file),
            },
            (Err(reason), _) => st.wardrobe.say(reason, false),
        }
    }
}

/// The tiles shown: the filter and the order applied, when either or the
/// model changed.
fn refresh_visible(st: &mut InventoryUi, facts: &Facts) {
    let w = &mut st.wardrobe;
    let key = (w.model.generation, w.filter.clone(), w.sort, w.reversed, facts.names.len());
    if w.visible_key.as_ref() == Some(&key) {
        return;
    }
    let same = |a: &str, b: &str| wardrobe::with(|shared| shared.index.kits.same(a, b));
    w.visible = model::visible(&w.model, &w.filter, w.sort, w.reversed, &facts.names, same, wardrobe::now());
    w.visible_key = Some(key);
}

/// Try on: the item worn at a click is taken off again by Revert, then
/// what it had replaced goes back on once the outfit settled.
fn try_on_step(st: &mut InventoryUi, inv: &Inventory, facts: &Facts, actions: &mut Vec<InvAction>) {
    let Some(trying) = &mut st.wardrobe.trying else { return };
    if !trying.reverting {
        let worn = facts.worn.contains(&trying.item);
        trying.seen_worn |= worn;
        if worn || facts.appearance_busy {
            return;
        }
        if trying.seen_worn {
            // taken off by hand (the bar, a menu, a double click): the same
            // as Revert, what the item replaced goes back on
            trying.reverting = true;
        } else if trying.started.elapsed() > TRY_WAIT {
            // never worn at all (a refusal): nothing to put back
            st.wardrobe.trying = None;
            return;
        } else {
            return;
        }
    }
    if facts.appearance_busy {
        return;
    }
    // a body part cannot be taken off: what it replaced is worn in its place
    let gone = !facts.worn.contains(&trying.item) || inv.items.get(&trying.item).is_some_and(|it| it.asset_type == 13);
    if !gone {
        return;
    }
    let back: Vec<Uuid> = trying
        .before
        .iter()
        .copied()
        .filter(|id| !facts.worn.contains(id) && inv.items.contains_key(id))
        .collect();
    if !back.is_empty() {
        actions.push(InvAction::Appearance(Wear::WearItems {
            items: back,
            replace: false,
            point: 0,
        }));
    }
    st.wardrobe.trying = None;
}

/// What a click on a card does besides selecting it, when Try on is armed.
fn try_on(st: &mut InventoryUi, inv: &Inventory, facts: &Facts, item: Uuid, actions: &mut Vec<InvAction>) {
    if st.wardrobe.trying.is_some() || !can_wear(inv, facts, item) {
        return;
    }
    st.wardrobe.trying = Some(Trying {
        item,
        before: facts.worn.clone(),
        reverting: false,
        seen_worn: false,
        started: std::time::Instant::now(),
    });
    wear(st, inv, facts, item, true, actions);
}

/// The rail: All, Wearing now (wide), the nine categories with their
/// counts, or the entries of Browse by.
fn rail_column(ui: &mut egui::Ui, p: &Palette, st: &mut InventoryUi, facts: &Facts, width: Width) {
    let labelled = width != Width::Compact;
    let w = &mut st.wardrobe;
    let mut choose: Option<Rail> = None;
    let mut pick: Option<Option<String>> = None;
    if labelled {
        // Browse by: what the sidebar lists
        let label = match w.filter.browse {
            Browse::Category => aurora_i18n::tr!("wardrobe-browse-category"),
            Browse::Creator => aurora_i18n::tr!("wardrobe-browse-creator"),
            Browse::Set => aurora_i18n::tr!("wardrobe-browse-set"),
            Browse::Event => aurora_i18n::tr!("wardrobe-browse-event"),
        };
        let mut browse = w.filter.browse;
        let button = egui::Button::new(
            egui::RichText::new(format!("{} · {label}  ⏷", aurora_i18n::tr!("wardrobe-browse-by")))
                .size(11.5)
                .color(p.muted),
        );
        menu::button(ui, p, button, |ui| {
            for (way, icon, key) in [
                (Browse::Category, "tag-filled", "wardrobe-browse-category"),
                (Browse::Creator, "user-filled", "wardrobe-browse-creator"),
                (Browse::Set, "folder-filled", "wardrobe-browse-set"),
                (Browse::Event, "calendar-event-filled", "wardrobe-browse-event"),
            ] {
                if menu::check(ui, p, icon, aurora_i18n::text(key), browse == way, true) {
                    browse = way;
                }
            }
        });
        if browse != w.filter.browse {
            w.filter.browse = browse;
            w.filter.pick = None;
        }
        ui.add_space(4.0);
    }
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        let all = w.filter.rail == Rail::All && w.filter.pick.is_none();
        if rail_entry(
            ui,
            p,
            "layout-grid-filled",
            aurora_i18n::tr!("wardrobe-all"),
            Some(w.model.tiles.len()),
            all,
            false,
            labelled,
        )
        .clicked()
        {
            choose = Some(Rail::All);
            pick = Some(None);
        }
        if width == Width::Wide || width == Width::Medium {
            let on = w.filter.rail == Rail::Wearing;
            if rail_entry(
                ui,
                p,
                "circle-check-filled",
                aurora_i18n::tr!("wardrobe-wearing-now"),
                Some(w.model.worn),
                on,
                false,
                labelled,
            )
            .clicked()
            {
                choose = Some(Rail::Wearing);
            }
        }
        ui.add_space(4.0);
        if w.filter.browse == Browse::Category || !labelled {
            for category in categories::WARDROBE {
                let on = matches!(&w.filter.rail, Rail::Category(key, _) if key == category.key);
                let count = w.model.counts.get(category.key).copied().unwrap_or(0);
                let dot = w.model.new_in.contains(category.key);
                let response = rail_entry(
                    ui,
                    p,
                    category_icon(category.key),
                    aurora_i18n::text(category.label),
                    Some(count),
                    on,
                    dot,
                    labelled,
                );
                if response.clicked() {
                    choose = Some(Rail::Category(category.key.to_owned(), None));
                }
            }
        } else {
            let groups = w.model.groups(w.filter.browse, &facts.names);
            if groups.is_empty() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-browse-empty"))
                        .size(11.5)
                        .color(p.muted),
                );
            }
            for group in groups.iter() {
                if let Some(creator) = group.creator.filter(|_| group.label.is_empty()) {
                    w.wanted_names.insert(creator);
                }
                let on = w.filter.pick.as_deref() == Some(group.key.as_str());
                let label = if group.label.is_empty() { "…" } else { group.label.as_str() };
                let icon = match w.filter.browse {
                    Browse::Creator if group.sub.is_some() || group.logo.is_some() => "building-store",
                    Browse::Creator => "user-filled",
                    Browse::Set => "folder-filled",
                    _ => "calendar-event-filled",
                };
                let response = rail_entry(ui, p, icon, label, Some(group.count), on, false, true);
                let response = match &group.sub {
                    Some(sub) => response.on_hover_text(sub),
                    None => response,
                };
                if response.clicked() {
                    pick = Some(if on { None } else { Some(group.key.clone()) });
                }
            }
        }
    });
    if let Some(rail) = choose {
        w.filter.rail = rail;
        w.open_stack = None;
    }
    if let Some(pick) = pick {
        w.filter.pick = pick;
    }
}

/// One row of the rail; an icon alone at the compact width, with its name
/// in a tooltip. `dot`: the category holds new items.
#[allow(clippy::too_many_arguments)]
fn rail_entry(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: &str,
    label: &str,
    count: Option<usize>,
    on: bool,
    dot: bool,
    labelled: bool,
) -> egui::Response {
    let tint = if on { p.violet_light } else { p.muted };
    let response = if labelled {
        widgets::rail_row(ui, p, icon, tint, label, count, on, 0)
    } else {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(ui.available_width().min(40.0)), Sense::click());
        let radius = ui.visuals().widgets.inactive.corner_radius;
        if on {
            ui.painter().rect_filled(rect.shrink(2.0), radius, p.violet.gamma_multiply(0.22));
        } else if response.hovered() {
            ui.painter().rect_filled(rect.shrink(2.0), radius, p.raised);
        }
        widgets::paint_glyph(ui.painter(), icon, rect.center(), 18.0, if on { p.violet_light } else { p.muted });
        response.on_hover_text(label)
    };
    if dot {
        // one pulse when the category first shows its new items
        let t = ui.ctx().animate_bool_with_time(response.id.with("new-dot"), true, 1.0);
        let pulse = (1.0 - t) * 3.0;
        let center = egui::pos2(response.rect.right() - 7.0, response.rect.top() + 7.0);
        ui.painter().circle_filled(
            center,
            3.0 + pulse,
            p.violet.gamma_multiply(1.0 - 0.6 * (1.0 - t).min(1.0) * pulse.min(1.0)),
        );
    }
    response
}

/// The middle: the tools, the chips, the grid, the note and the selection
/// bar.
#[allow(clippy::too_many_arguments)]
fn middle(
    ui: &mut egui::Ui,
    p: &Palette,
    inv: &mut Inventory,
    st: &mut InventoryUi,
    prefs: &mut InventoryPreferences,
    facts: &Facts,
    images: &std::collections::HashMap<Uuid, egui::TextureHandle>,
    actions: &mut Vec<InvAction>,
    width: Width,
) {
    tools_row(ui, p, st, prefs, width);
    ui.add_space(4.0);
    if width == Width::Thin {
        category_strip(ui, p, st, facts);
    } else {
        chips_row(ui, p, st, facts, width, prefs.wardrobe.hints);
    }
    subtype_row(ui, p, st);
    ui.add_space(4.0);
    let note_h = if st.wardrobe.note.is_some() || st.wardrobe.trying.is_some() {
        30.0
    } else {
        0.0
    };
    let bar_h = 52.0;
    let room = ui.available_rect_before_wrap();
    let split = (room.bottom() - bar_h - note_h).max(room.top() + 60.0);
    let list = Rect::from_min_max(room.min, egui::pos2(room.right(), split - 6.0));
    let foot = Rect::from_min_max(egui::pos2(room.left(), split), room.max);
    let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list).id_salt("wardrobe_grid_room"));
    list_ui.set_clip_rect(list.intersect(ui.clip_rect()));
    grid::show(&mut list_ui, p, inv, st, prefs, facts, images, actions, width);
    let mut foot_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(foot)
            .id_salt("wardrobe_foot")
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    foot_ui.set_clip_rect(foot.intersect(ui.clip_rect()));
    note_line(&mut foot_ui, p, st, inv, facts, actions);
    selection_bar(&mut foot_ui, p, inv, st, facts, images, actions, width);
    ui.advance_cursor_after_rect(room);
}

/// Search with its scope, the order, Auto-sort, Try on and the menu of the
/// rest.
fn tools_row(ui: &mut egui::Ui, p: &Palette, st: &mut InventoryUi, prefs: &mut InventoryPreferences, width: Width) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let buttons = if width == Width::Thin { 4.0 } else { 7.0 };
        let field = (ui.available_width() - buttons * 28.0).max(90.0);
        let hint = match st.wardrobe.filter.scope {
            Scope::Everything => aurora_i18n::tr!("wardrobe-search-everything"),
            Scope::Names => aurora_i18n::tr!("wardrobe-search-names"),
            Scope::Stores => aurora_i18n::tr!("wardrobe-search-stores"),
            Scope::Events => aurora_i18n::tr!("wardrobe-search-events"),
            Scope::Folders => aurora_i18n::tr!("wardrobe-search-folders"),
            Scope::Descriptions => aurora_i18n::tr!("wardrobe-search-descriptions"),
        };
        let search = widgets::clearable_search_box(ui, p, &mut st.wardrobe.filter.text, hint, field);
        search.on_hover_text(aurora_i18n::tr!("wardrobe-search-tip"));
        let mut scope = st.wardrobe.filter.scope;
        widgets::icon_menu(ui, p, "filter-filled", aurora_i18n::tr!("wardrobe-search-scope"), |ui| {
            for (way, key) in Scope::ALL.into_iter().zip([
                "wardrobe-scope-everything",
                "wardrobe-scope-names",
                "wardrobe-scope-stores",
                "wardrobe-scope-events",
                "wardrobe-scope-folders",
                "wardrobe-scope-descriptions",
            ]) {
                if menu::check(ui, p, "", aurora_i18n::text(key), scope == way, true) {
                    scope = way;
                }
            }
        });
        st.wardrobe.filter.scope = scope;
        let (mut sort, mut reversed) = (st.wardrobe.sort, st.wardrobe.reversed);
        widgets::icon_menu(ui, p, "arrows-sort", aurora_i18n::tr!("wardrobe-sort"), |ui| {
            for (way, key) in [
                (SortBy::Added, "wardrobe-sort-added"),
                (SortBy::LastWorn, "wardrobe-sort-last-worn"),
                (SortBy::Name, "wardrobe-sort-name"),
                (SortBy::Store, "wardrobe-sort-store"),
                (SortBy::Category, "wardrobe-sort-category"),
            ] {
                if menu::check(ui, p, "", aurora_i18n::text(key), sort == way, true) {
                    sort = way;
                }
            }
            menu::separator(ui, p);
            menu::toggle(ui, p, "arrows-sort", aurora_i18n::tr!("wardrobe-sort-reversed"), &mut reversed);
        });
        if (sort, reversed) != (st.wardrobe.sort, st.wardrobe.reversed) {
            wardrobe::with(|shared| {
                shared.wardrobe.change(false, |txn| {
                    let view = &mut txn.account_mut().view;
                    view.sort = sort;
                    view.reversed = reversed;
                })
            });
        }
        // Auto-sort: files what the viewer can place without a card
        let waiting = st.wardrobe.model.sortable;
        let tip = if waiting > 0 {
            aurora_i18n::tr!("wardrobe-auto-sort-tip", count = waiting)
        } else {
            aurora_i18n::tr!("wardrobe-auto-sort-none").to_owned()
        };
        if widgets::icon_button(ui, p, "wand", &tip, waiting > 0) {
            let filed = wardrobe::with(|shared| {
                let placed = shared.index.sortable.clone();
                shared.wardrobe.change(true, |txn| sort::apply(txn, &placed))
            });
            let parts: Vec<String> = filed
                .by_category
                .iter()
                .map(|(key, count)| format!("{count} {}", categories::category_label(key)))
                .collect();
            st.wardrobe.say(
                aurora_i18n::tr!("wardrobe-auto-sort-filed", count = filed.total, list = parts.join(", ")),
                true,
            );
        }
        if widgets::icon_toggle(
            ui,
            p,
            "hanger-2-filled",
            aurora_i18n::tr!("wardrobe-try-on-tip"),
            st.wardrobe.try_on,
        ) {
            st.wardrobe.try_on = !st.wardrobe.try_on;
        }
        // the creator tool: a button of its own where there is room, the
        // menu's first row everywhere
        if wardrobe::CARDS
            && width != Width::Thin
            && widgets::icon_button(ui, p, "file-text-filled", aurora_i18n::tr!("wardrobe-creator-open"), true)
        {
            st.wardrobe.creator.open = true;
        }
        let mut stacks = prefs.wardrobe.stacks;
        let mut tips = prefs.wardrobe.tips;
        let mut hints = prefs.wardrobe.hints;
        let mut creator = false;
        let mut unsorted = false;
        let mut stores = false;
        widgets::icon_menu(ui, p, "dots-filled", aurora_i18n::tr!("wardrobe-more"), |ui| {
            if wardrobe::CARDS && menu::item(ui, p, "file-text-filled", aurora_i18n::tr!("wardrobe-creator-open")) {
                creator = true;
            }
            let count = st.wardrobe.model.unsorted.len();
            if menu::item_if(ui, p, "plus", &aurora_i18n::tr!("wardrobe-unsorted", count = count), count > 0) {
                unsorted = true;
            }
            if menu::item(ui, p, "building-store", aurora_i18n::tr!("wardrobe-stores-open")) {
                stores = true;
            }
            menu::separator(ui, p);
            menu::toggle(ui, p, "stack-2-filled", aurora_i18n::tr!("wardrobe-pref-stacks"), &mut stacks);
            menu::toggle(ui, p, "info-circle-filled", aurora_i18n::tr!("wardrobe-pref-tips"), &mut tips);
            menu::toggle(ui, p, "info-circle-filled", aurora_i18n::tr!("wardrobe-pref-hints"), &mut hints);
        });
        if (stacks, tips, hints) != (prefs.wardrobe.stacks, prefs.wardrobe.tips, prefs.wardrobe.hints) {
            prefs.wardrobe.stacks = stacks;
            prefs.wardrobe.tips = tips;
            prefs.wardrobe.hints = hints;
            st.preferences_dirty = true;
        }
        if creator {
            st.wardrobe.creator.open = true;
        }
        if unsorted {
            st.wardrobe.dialog = Some(dialogs::Dialog::unsorted());
        }
        if stores {
            st.wardrobe.stores.open = true;
        }
    });
}

/// The status chips, "Created for" and the rest of the filters.
fn chips_row(ui: &mut egui::Ui, p: &Palette, st: &mut InventoryUi, facts: &Facts, width: Width, hints: bool) {
    let w = &mut st.wardrobe;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        // the mouse hints: one line that explains a chip, when asked for
        let hint = |response: egui::Response, key: &'static str| {
            if hints {
                response.on_hover_text(aurora_i18n::text(key))
            } else {
                response
            }
        };
        if hint(
            widgets::filter_pill(ui, p, "check", aurora_i18n::tr!("wardrobe-chip-worn"), w.filter.worn),
            "wardrobe-chip-worn-tip",
        )
        .clicked()
        {
            w.filter.worn = !w.filter.worn;
            w.filter.not_worn = false;
        }
        if hint(
            widgets::filter_pill(ui, p, "sparkles-filled", aurora_i18n::tr!("wardrobe-chip-new"), w.filter.new),
            "wardrobe-chip-new-tip",
        )
        .clicked()
        {
            w.filter.new = !w.filter.new;
        }
        if hint(
            widgets::filter_pill(
                ui,
                p,
                "heart-filled",
                aurora_i18n::tr!("wardrobe-chip-favorites"),
                w.filter.favorites,
            ),
            "wardrobe-chip-favorites-tip",
        )
        .clicked()
        {
            w.filter.favorites = !w.filter.favorites;
        }
        if hint(
            widgets::filter_pill(ui, p, "adjustments-filled", aurora_i18n::tr!("wardrobe-chip-hud"), w.filter.has_hud),
            "wardrobe-chip-hud-tip",
        )
        .clicked()
        {
            w.filter.has_hud = !w.filter.has_hud;
        }
        created_for(ui, p, w, width);
        more_filters(ui, p, w, &facts.names);
    });
}

/// "Created for": one kit at a time, grouped by part, with what is worn.
fn created_for(ui: &mut egui::Ui, p: &Palette, w: &mut State, width: Width) {
    let label = match &w.filter.created {
        Created::Any => aurora_i18n::tr!("wardrobe-created-for").to_owned(),
        Created::Mine => aurora_i18n::tr!("wardrobe-created-mine").to_owned(),
        Created::Everyone => aurora_i18n::tr!("wardrobe-fit-everyone").to_owned(),
        Created::Kit(kit) => aurora_i18n::tr!("wardrobe-created-kit", kit = kit.as_str()),
    };
    let on = w.filter.created != Created::Any;
    let mut chosen: Option<Created> = None;
    // the wide window shows the usual choices side by side
    if width == Width::Wide {
        for (way, key) in [
            (Created::Mine, "wardrobe-created-mine"),
            (Created::Everyone, "wardrobe-fit-everyone"),
        ] {
            if widgets::filter_pill(ui, p, "", aurora_i18n::text(key), w.filter.created == way)
                .on_hover_text(aurora_i18n::tr!("wardrobe-created-tip"))
                .clicked()
            {
                chosen = Some(if w.filter.created == way { Created::Any } else { way });
            }
        }
    }
    let kits = w.model.kits.clone();
    let current = w.filter.created.clone();
    // lit while a kit is chosen, like the pills beside it
    let button = egui::Button::new(egui::RichText::new(format!("{label}  ⏷")).size(12.0)).selected(on);
    menu::button(ui, p, button, |ui| {
        if menu::check(ui, p, "", aurora_i18n::tr!("wardrobe-created-any"), current == Created::Any, true) {
            chosen = Some(Created::Any);
        }
        if menu::check(
            ui,
            p,
            "check",
            aurora_i18n::tr!("wardrobe-created-mine"),
            current == Created::Mine,
            true,
        ) {
            chosen = Some(Created::Mine);
        }
        if menu::check(
            ui,
            p,
            "",
            aurora_i18n::tr!("wardrobe-fit-everyone"),
            current == Created::Everyone,
            true,
        ) {
            chosen = Some(Created::Everyone);
        }
        let mut part = None;
        for kit in &kits {
            if part != Some(kit.part) {
                part = Some(kit.part);
                menu::separator(ui, p);
                ui.label(egui::RichText::new(kit.part.label()).size(11.0).color(p.muted));
            }
            let name = if kit.worn {
                aurora_i18n::tr!("wardrobe-kit-wearing", kit = kit.name.as_str(), count = kit.count)
            } else {
                aurora_i18n::tr!("wardrobe-kit-count", kit = kit.name.as_str(), count = kit.count)
            };
            let on = matches!(&current, Created::Kit(k) if k.eq_ignore_ascii_case(&kit.name));
            if menu::check(ui, p, if kit.worn { "check" } else { "" }, &name, on, true) {
                chosen = Some(Created::Kit(kit.name.clone()));
            }
        }
    });
    if let Some(created) = chosen {
        w.filter.created = created;
    }
}

/// Event, store and the remaining "Show" choices.
fn more_filters(ui: &mut egui::Ui, p: &Palette, w: &mut State, names: &std::collections::HashMap<Uuid, String>) {
    let events = w.model.events.clone();
    let stores = w.model.groups(Browse::Creator, names);
    let on = w.filter.event.is_some()
        || w.filter.store.is_some()
        || w.filter.not_worn
        || w.filter.this_week
        || w.filter.demos != model::Demos::All;
    let button =
        egui::Button::new(egui::RichText::new(format!("{}  ⏷", aurora_i18n::tr!("wardrobe-more-filters"))).size(12.0)).selected(on);
    let filter = &mut w.filter;
    menu::button(ui, p, button, |ui| {
        if menu::check(ui, p, "", aurora_i18n::tr!("wardrobe-filter-not-worn"), filter.not_worn, true) {
            filter.not_worn = !filter.not_worn;
            filter.worn = false;
        }
        if menu::check(ui, p, "", aurora_i18n::tr!("wardrobe-filter-this-week"), filter.this_week, true) {
            filter.this_week = !filter.this_week;
        }
        menu::separator(ui, p);
        for (way, key) in [
            (model::Demos::All, "wardrobe-demos-all"),
            (model::Demos::Hide, "wardrobe-demos-hide"),
            (model::Demos::Only, "wardrobe-demos-only"),
        ] {
            if menu::check(ui, p, "", aurora_i18n::text(key), filter.demos == way, true) {
                filter.demos = way;
            }
        }
        menu::submenu(
            ui,
            p,
            "calendar-event-filled",
            aurora_i18n::tr!("wardrobe-browse-event"),
            !events.is_empty(),
            |ui| {
                if menu::check(ui, p, "", aurora_i18n::tr!("wardrobe-created-any"), filter.event.is_none(), true) {
                    filter.event = None;
                }
                for event in &events {
                    if menu::check(ui, p, "", event, filter.event.as_ref() == Some(event), true) {
                        filter.event = Some(event.clone());
                    }
                }
            },
        );
        menu::submenu(
            ui,
            p,
            "building-store",
            aurora_i18n::tr!("wardrobe-filter-store"),
            !stores.is_empty(),
            |ui| {
                if menu::check(ui, p, "", aurora_i18n::tr!("wardrobe-created-any"), filter.store.is_none(), true) {
                    filter.store = None;
                }
                for store in stores.iter().filter(|s| !s.label.is_empty()).take(40) {
                    let label = format!("{} · {}", store.label, store.count);
                    if menu::check(ui, p, "", &label, filter.store.as_ref() == Some(&store.key), true) {
                        filter.store = Some(store.key.clone());
                    }
                }
            },
        );
        menu::separator(ui, p);
        if menu::item(ui, p, "x", aurora_i18n::tr!("wardrobe-filter-clear")) {
            let (rail, browse, pick, text, scope) = (
                filter.rail.clone(),
                filter.browse,
                filter.pick.clone(),
                filter.text.clone(),
                filter.scope,
            );
            *filter = Filter {
                rail,
                browse,
                pick,
                text,
                scope,
                ..Filter::default()
            };
        }
    });
}

/// The thin window: the categories as a strip of pills, and one Filters
/// menu for everything else.
fn category_strip(ui: &mut egui::Ui, p: &Palette, st: &mut InventoryUi, facts: &Facts) {
    let w = &mut st.wardrobe;
    egui::ScrollArea::horizontal()
        .id_salt("wardrobe_strip")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if widgets::filter_pill(
                    ui,
                    p,
                    "layout-grid-filled",
                    aurora_i18n::tr!("wardrobe-all"),
                    w.filter.rail == Rail::All,
                )
                .clicked()
                {
                    w.filter.rail = Rail::All;
                }
                if widgets::filter_pill(
                    ui,
                    p,
                    "circle-check-filled",
                    aurora_i18n::tr!("wardrobe-chip-worn"),
                    w.filter.rail == Rail::Wearing,
                )
                .clicked()
                {
                    w.filter.rail = Rail::Wearing;
                }
                for category in categories::WARDROBE {
                    let on = matches!(&w.filter.rail, Rail::Category(key, _) if key == category.key);
                    if widgets::filter_pill(ui, p, category_icon(category.key), aurora_i18n::text(category.label), on).clicked() {
                        w.filter.rail = Rail::Category(category.key.to_owned(), None);
                    }
                }
            });
        });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (flag, icon, key) in [
            (&mut w.filter.new, "sparkles-filled", "wardrobe-chip-new"),
            (&mut w.filter.favorites, "heart-filled", "wardrobe-chip-favorites"),
            (&mut w.filter.has_hud, "adjustments-filled", "wardrobe-chip-hud"),
        ] {
            if widgets::filter_pill(ui, p, icon, aurora_i18n::text(key), *flag).clicked() {
                *flag = !*flag;
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
        created_for(ui, p, w, Width::Thin);
        more_filters(ui, p, w, &facts.names);
    });
}

/// The subtypes of the chosen category, as pills.
fn subtype_row(ui: &mut egui::Ui, p: &Palette, st: &mut InventoryUi) {
    let w = &mut st.wardrobe;
    let Rail::Category(key, chosen) = w.filter.rail.clone() else {
        return;
    };
    let Some(category) = categories::category(&key) else { return };
    let in_use = w.model.subtypes.get(&key).map(Vec::as_slice).unwrap_or_default();
    let used: Vec<&categories::Subtype> = category.subtypes.iter().filter(|s| in_use.iter().any(|u| u == s.key)).collect();
    if used.len() < 2 {
        return;
    }
    ui.add_space(4.0);
    egui::ScrollArea::horizontal()
        .id_salt("wardrobe_subtypes")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if widgets::filter_pill(ui, p, "", aurora_i18n::tr!("wardrobe-all-of"), chosen.is_none()).clicked() {
                    w.filter.rail = Rail::Category(key.clone(), None);
                }
                for subtype in used {
                    let on = chosen.as_deref() == Some(subtype.key);
                    if widgets::filter_pill(ui, p, "", aurora_i18n::text(subtype.label), on).clicked() {
                        w.filter.rail = Rail::Category(key.clone(), (!on).then(|| subtype.key.to_owned()));
                    }
                }
            });
        });
}

/// What just happened, with Undo; or the try on in progress, with Revert
/// and Keep.
fn note_line(ui: &mut egui::Ui, p: &Palette, st: &mut InventoryUi, inv: &Inventory, facts: &Facts, actions: &mut Vec<InvAction>) {
    if st.wardrobe.note.as_ref().is_some_and(|n| n.at.elapsed().as_secs() > NOTE_SECONDS) {
        st.wardrobe.note = None;
    }
    if let Some(trying) = &st.wardrobe.trying {
        let name = inv.items.get(&trying.item).map_or("", |it| it.name.as_str());
        let (item, reverting) = (trying.item, trying.reverting);
        let mut revert = false;
        let mut keep = false;
        ui.horizontal(|ui| {
            ui.set_height(26.0);
            widgets::glyph(ui, "hanger-2-filled", p.violet_light, 14.0);
            ui.label(
                egui::RichText::new(aurora_i18n::tr!("wardrobe-trying", name = name))
                    .size(12.0)
                    .color(p.ink),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                keep = widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-try-keep")).clicked();
                revert = ui
                    .add_enabled(
                        !reverting,
                        egui::Button::new(egui::RichText::new(aurora_i18n::tr!("wardrobe-try-revert")).size(12.0)),
                    )
                    .clicked();
            });
        });
        if keep {
            st.wardrobe.trying = None;
        } else if revert {
            if inv.items.get(&item).is_some_and(|it| it.asset_type != 13) {
                take_off(inv, facts, item, actions);
            }
            if let Some(trying) = &mut st.wardrobe.trying {
                trying.reverting = true;
            }
        }
        return;
    }
    let Some(note) = &st.wardrobe.note else { return };
    let (text, undo) = (note.text.clone(), note.undo);
    let mut close = false;
    let mut undone = false;
    ui.horizontal(|ui| {
        ui.set_height(26.0);
        widgets::glyph(ui, "info-circle-filled", p.teal, 14.0);
        ui.add(egui::Label::new(egui::RichText::new(text).size(12.0).color(p.ink)).truncate());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            close = widgets::icon_button(ui, p, "x", aurora_i18n::tr!("common-close"), true);
            if undo && wardrobe::with(|shared| shared.wardrobe.can_undo()) {
                undone = widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-undo")).clicked();
            }
        });
    });
    if undone {
        wardrobe::with(|shared| shared.wardrobe.undo());
    }
    if close || undone {
        st.wardrobe.note = None;
    }
}

/// The selection bar: picture, name, store, the fit, the HUD button, Wear
/// or Take off, and the rest.
#[allow(clippy::too_many_arguments)]
fn selection_bar(
    ui: &mut egui::Ui,
    p: &Palette,
    inv: &Inventory,
    st: &mut InventoryUi,
    facts: &Facts,
    images: &std::collections::HashMap<Uuid, egui::TextureHandle>,
    actions: &mut Vec<InvAction>,
    width: Width,
) {
    let Some(tile) = st.wardrobe.chosen().cloned() else {
        ui.add_space(14.0);
        ui.label(
            egui::RichText::new(aurora_i18n::tr!("wardrobe-count", count = st.wardrobe.visible.len()))
                .size(12.0)
                .color(p.muted),
        );
        return;
    };
    let item = st.wardrobe.selected.unwrap_or(tile.item);
    let worn = facts.worn.contains(&item);
    ui.horizontal(|ui| {
        ui.set_height(48.0);
        ui.spacing_mut().item_spacing.x = 8.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::hover());
        grid::paint_picture(ui, p, rect, &tile, images, &mut st.wanted_images, 1.0);
        // the store's mark, in the corner of the picture
        let mark = Rect::from_min_size(rect.right_bottom() - Vec2::splat(16.0), Vec2::splat(16.0));
        grid::store_mark(ui, p, mark, &tile, images, &mut st.wanted_images);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.add_space(4.0);
            let name = inv.items.get(&item).map_or(tile.name.as_str(), |it| it.name.as_str());
            ui.add(egui::Label::new(egui::RichText::new(name).size(13.0).strong().color(p.ink)).truncate());
            // the store, else the creator's name
            let mut line = tile
                .store
                .clone()
                .or_else(|| facts.names.get(&tile.creator).cloned())
                .unwrap_or_default();
            if width != Width::Thin {
                if !line.is_empty() {
                    line.push_str(" · ");
                }
                line.push_str(&fit_text(&tile.fit));
            }
            let color = if tile.fit.fits() { p.muted } else { p.warn };
            ui.add(egui::Label::new(egui::RichText::new(line).size(11.5).color(color)).truncate());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let mut menu_open = false;
            widgets::icon_menu(ui, p, "dots-filled", aurora_i18n::tr!("wardrobe-more"), |ui| {
                menu_open = true;
                tile_menu(ui, p, inv, st, facts, &tile, item, actions);
            });
            let _ = menu_open;
            let size = Vec2::new(if width == Width::Thin { 84.0 } else { 104.0 }, 28.0);
            if worn {
                let enabled = can_take_off(inv, facts, item);
                if widgets::action_button_if(
                    ui,
                    p,
                    "x",
                    aurora_i18n::tr!("wardrobe-take-off"),
                    widgets::ActionKind::Flat,
                    size,
                    enabled,
                )
                .clicked()
                {
                    take_off(inv, facts, item, actions);
                }
            } else {
                let enabled = can_wear(inv, facts, item);
                let button = ui
                    .add_enabled_ui(enabled, |ui| {
                        widgets::primary_button_sized(ui, p, aurora_i18n::tr!("common-wear"), size)
                    })
                    .inner;
                if button.on_hover_text(aurora_i18n::tr!("wardrobe-wear-tip")).clicked() {
                    wear(st, inv, facts, item, true, actions);
                }
            }
            if !tile.huds.is_empty() && width != Width::Thin {
                hud_button(ui, p, inv, st, facts, &tile, actions);
            }
            if width == Width::Wide || width == Width::Medium {
                let quick = widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-quick-look"))
                    .on_hover_text(aurora_i18n::tr!("wardrobe-quick-look-tip"));
                if quick.clicked() {
                    st.wardrobe.open_quick_look(item);
                }
            }
        });
    });
}

/// Wear HUD, Detach HUD, or "N HUDs" with a menu naming each one.
fn hud_button(
    ui: &mut egui::Ui,
    p: &Palette,
    inv: &Inventory,
    st: &mut InventoryUi,
    facts: &Facts,
    tile: &Tile,
    actions: &mut Vec<InvAction>,
) {
    let all_worn = tile.huds.iter().all(|id| facts.worn.contains(id));
    if let [hud] = tile.huds.as_slice() {
        let label = if all_worn {
            aurora_i18n::tr!("wardrobe-hud-detach")
        } else {
            aurora_i18n::tr!("wardrobe-hud-wear")
        };
        if widgets::flat_button(ui, p, label).clicked() {
            if all_worn {
                detach_huds(inv, facts, &[*hud], actions);
            } else if wear_huds(inv, facts, &[*hud], actions) > 0 {
                st.wardrobe.say(aurora_i18n::tr!("wardrobe-hud-refused").to_owned(), false);
            }
        }
        return;
    }
    let label = aurora_i18n::tr!("wardrobe-hud-count", count = tile.huds.len());
    let button = egui::Button::new(egui::RichText::new(format!("{label}  ⏷")).size(12.0));
    let mut refused = false;
    menu::button(ui, p, button, |ui| {
        if menu::item_if(ui, p, "plus", aurora_i18n::tr!("wardrobe-hud-wear-all"), !all_worn) {
            refused = wear_huds(inv, facts, &tile.huds, actions) > 0;
        }
        menu::separator(ui, p);
        for hud in &tile.huds {
            let Some(hud_item) = inv.items.get(hud) else { continue };
            let kind = wardrobe::huds::kind(&hud_item.name, &tile.name).unwrap_or_else(|| hud_item.name.clone());
            let worn = facts.worn.contains(hud);
            if menu::check(ui, p, "adjustments-filled", &kind, worn, true) {
                if worn {
                    detach_huds(inv, facts, &[*hud], actions);
                } else {
                    refused = wear_huds(inv, facts, &[*hud], actions) > 0;
                }
            }
        }
    });
    if refused {
        st.wardrobe.say(aurora_i18n::tr!("wardrobe-hud-refused").to_owned(), false);
    }
}

/// The menu of a card: the right click, and the bar's "More".
#[allow(clippy::too_many_arguments)]
pub(super) fn tile_menu(
    ui: &mut egui::Ui,
    p: &Palette,
    inv: &Inventory,
    st: &mut InventoryUi,
    facts: &Facts,
    tile: &Tile,
    item: Uuid,
    actions: &mut Vec<InvAction>,
) {
    let worn = facts.worn.contains(&item);
    if worn {
        if menu::item_if(ui, p, "x", aurora_i18n::tr!("wardrobe-take-off"), can_take_off(inv, facts, item)) {
            take_off(inv, facts, item, actions);
        }
    } else {
        if menu::item_if(ui, p, "shirt-filled", aurora_i18n::tr!("common-wear"), can_wear(inv, facts, item)) {
            wear(st, inv, facts, item, true, actions);
        }
        if menu::item_if(ui, p, "plus", aurora_i18n::tr!("wardrobe-add"), can_add(inv, facts, item)) {
            wear(st, inv, facts, item, false, actions);
        }
    }
    if !tile.huds.is_empty() {
        let all_worn = tile.huds.iter().all(|id| facts.worn.contains(id));
        if all_worn {
            if menu::item(ui, p, "adjustments-filled", aurora_i18n::tr!("wardrobe-hud-detach")) {
                detach_huds(inv, facts, &tile.huds, actions);
            }
        } else if menu::item(ui, p, "adjustments-filled", aurora_i18n::tr!("wardrobe-hud-wear-all"))
            && wear_huds(inv, facts, &tile.huds, actions) > 0
        {
            st.wardrobe.say(aurora_i18n::tr!("wardrobe-hud-refused").to_owned(), false);
        }
    }
    if menu::item_keys(ui, p, "eye-filled", aurora_i18n::tr!("wardrobe-quick-look"), "Space") {
        st.wardrobe.open_quick_look(item);
    }
    menu::separator(ui, p);
    if menu::item(ui, p, "tag-filled", aurora_i18n::tr!("wardrobe-menu-categories")) {
        st.wardrobe.dialog = Some(dialogs::Dialog::categories(item, &tile.tags));
    }
    if menu::item(ui, p, "man-filled", aurora_i18n::tr!("wardrobe-menu-fits")) {
        st.wardrobe.dialog = Some(dialogs::Dialog::fits(item, &tile.fits));
    }
    if menu::item(ui, p, "photo-filled", aurora_i18n::tr!("wardrobe-menu-picture")) {
        st.wardrobe.dialog = Some(dialogs::Dialog::picture(item));
    }
    if menu::item(ui, p, "pencil-filled", aurora_i18n::tr!("wardrobe-menu-note")) {
        st.wardrobe.dialog = Some(dialogs::Dialog::note(item, tile.note.as_deref().unwrap_or("")));
    }
    let favorite = wardrobe::with(|shared| shared.wardrobe.account.bindings.get(&item).is_some_and(|b| b.favorite));
    if menu::check(ui, p, "heart-filled", aurora_i18n::tr!("wardrobe-menu-favorite"), favorite, true) {
        wardrobe::with(|shared| shared.wardrobe.change(false, |txn| txn.binding(item).favorite = !favorite));
    }
    // a kit others are created for: a body, a head, ears… said in one
    // place, with the preset the item's name tells
    if menu::check(
        ui,
        p,
        "man-filled",
        aurora_i18n::tr!("wardrobe-menu-register"),
        !tile.kit.is_empty(),
        true,
    ) {
        let detected = inv.items.get(&item).and_then(wardrobe::fit::preset_of);
        let part = wardrobe::with(|shared| shared.wardrobe.entry(item).map_or(categories::Part::Other, |entry| entry.part()));
        st.wardrobe.dialog = Some(dialogs::Dialog::register(item, part, &tile.kit, detected));
    }
    if menu::check(ui, p, "tag-filled", aurora_i18n::tr!("wardrobe-tip-demo"), tile.demo, true) {
        let demo = !tile.demo;
        wardrobe::with(|shared| shared.wardrobe.change(false, |txn| txn.binding(item).demo = Some(demo)));
    }
    if tile.product.is_some() && menu::item(ui, p, "copy-filled", aurora_i18n::tr!("wardrobe-menu-separate")) {
        wardrobe::with(|shared| shared.wardrobe.change(true, |txn| wardrobe::matching::separate(txn, item)));
        st.wardrobe
            .say(aurora_i18n::tr!("wardrobe-separated", name = tile.name.as_str()), true);
    }
    if menu::item(ui, p, "copy-filled", aurora_i18n::tr!("wardrobe-menu-same-product")) {
        st.wardrobe.dialog = Some(dialogs::Dialog::same_product(item));
    }
    if menu::item(ui, p, "link-filled", aurora_i18n::tr!("wardrobe-menu-link-hud")) {
        st.wardrobe.dialog = Some(dialogs::Dialog::link_hud(item));
    }
    menu::separator(ui, p);
    store_rows(ui, p, st, tile, item);
    menu::separator(ui, p);
    if menu::item(ui, p, "folder-filled", aurora_i18n::tr!("wardrobe-menu-show-original")) {
        actions.push(InvAction::ShowMain(item));
        st.wardrobe.leave = true;
    }
    if menu::item(ui, p, "trash-filled", aurora_i18n::tr!("wardrobe-menu-remove")) {
        wardrobe::with(|shared| shared.wardrobe.change(true, |txn| txn.remove_binding(item)));
        st.wardrobe
            .say(aurora_i18n::tr!("wardrobe-removed", name = tile.name.as_str()), true);
    }
}

/// The store of an item: its Marketplace page and its main store, through
/// the viewer's link rules, or "Link a store…" and the creator's profile.
pub(super) fn store_rows(ui: &mut egui::Ui, p: &Palette, st: &mut InventoryUi, tile: &Tile, item: Uuid) {
    if let Some(url) = &tile.marketplace
        && menu::item(ui, p, "external-link-filled", aurora_i18n::tr!("wardrobe-store-marketplace"))
    {
        crate::ui::context::request(ui.ctx(), crate::ui::web_view::clicked_link_action(url));
    }
    if let Some(url) = &tile.mainstore
        && menu::item(ui, p, "building-store", aurora_i18n::tr!("wardrobe-store-visit"))
    {
        crate::ui::context::request(ui.ctx(), crate::ui::web_view::clicked_link_action(url));
    }
    // a store the creator's card set is theirs to describe
    if (tile.store.is_none() || tile.store_by_user) && menu::item(ui, p, "link-filled", aurora_i18n::tr!("wardrobe-store-link")) {
        st.wardrobe.store_edit = Some(stores::Edit::for_item(item, tile.creator, tile.store_key));
    }
    if menu::item(ui, p, "building-store", aurora_i18n::tr!("wardrobe-stores-open")) {
        st.wardrobe.stores.open = true;
    }
    if !tile.creator.is_nil() && menu::item(ui, p, "user-filled", aurora_i18n::tr!("wardrobe-store-profile")) {
        st.wardrobe.profile = Some(tile.creator);
    }
}

/// The details pane of the wide window: the selection in full.
fn details(
    ui: &mut egui::Ui,
    p: &Palette,
    inv: &Inventory,
    st: &mut InventoryUi,
    facts: &Facts,
    images: &std::collections::HashMap<Uuid, egui::TextureHandle>,
    actions: &mut Vec<InvAction>,
) {
    let Some(tile) = st.wardrobe.chosen().cloned() else {
        wearing_now(ui, p, st);
        return;
    };
    let item = st.wardrobe.selected.unwrap_or(tile.item);
    st.wardrobe.wanted_names.insert(tile.creator);
    egui::ScrollArea::vertical()
        .id_salt("wardrobe_details_scroll")
        .auto_shrink(false)
        .show(ui, |ui| {
            let side = ui.available_width().min(260.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
            grid::paint_picture(ui, p, rect, &tile, images, &mut st.wanted_images, 1.0);
            ui.add_space(8.0);
            tips::lines(ui, p, &tile, item, inv, facts, true);
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                if let Some(url) = &tile.marketplace
                    && widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-store-marketplace")).clicked()
                {
                    crate::ui::context::request(ui.ctx(), crate::ui::web_view::clicked_link_action(url));
                }
                if let Some(url) = &tile.mainstore
                    && widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-store-visit")).clicked()
                {
                    crate::ui::context::request(ui.ctx(), crate::ui::web_view::clicked_link_action(url));
                }
                if (tile.store.is_none() || tile.store_by_user)
                    && widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-store-link")).clicked()
                {
                    st.wardrobe.store_edit = Some(stores::Edit::for_item(item, tile.creator, tile.store_key));
                }
                if widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-store-profile")).clicked() {
                    st.wardrobe.profile = Some(tile.creator);
                }
            });
            let _ = actions;
        });
}

/// Wearing now, head to toe: one slot per category, an empty one opens its
/// category.
fn wearing_now(ui: &mut egui::Ui, p: &Palette, st: &mut InventoryUi) {
    let w = &mut st.wardrobe;
    widgets::section_header(ui, p, "circle-check-filled", aurora_i18n::tr!("wardrobe-wearing-now"));
    const SLOTS: [&str; 8] = [
        "hair",
        "heads",
        "tops",
        "bottoms",
        "full-body",
        "shoes",
        "body-parts",
        "accessories",
    ];
    let worn_in = |key: &str| -> Vec<&Tile> {
        w.model
            .tiles
            .iter()
            .filter(|t| t.worn && t.main().is_some_and(|m| m.category() == key))
            .collect()
    };
    let filled = SLOTS.iter().filter(|key| !worn_in(key).is_empty()).count();
    ui.label(
        egui::RichText::new(aurora_i18n::tr!("wardrobe-slots", filled = filled, total = SLOTS.len()))
            .size(11.5)
            .color(p.muted),
    );
    ui.add_space(4.0);
    let mut open: Option<String> = None;
    let mut select: Option<Uuid> = None;
    egui::ScrollArea::vertical()
        .id_salt("wardrobe_wearing")
        .auto_shrink(false)
        .show(ui, |ui| {
            for key in SLOTS {
                let worn = worn_in(key);
                let label = categories::category_label(key);
                let text = match worn.as_slice() {
                    [] => aurora_i18n::tr!("wardrobe-slot-empty").to_owned(),
                    [one] => one.name.clone(),
                    [one, rest @ ..] => aurora_i18n::tr!("wardrobe-slot-several", name = one.name.as_str(), count = rest.len()),
                };
                let response = widgets::rail_row(
                    ui,
                    p,
                    category_icon(key),
                    if worn.is_empty() { p.muted_dim } else { p.teal },
                    label,
                    None,
                    false,
                    0,
                )
                .on_hover_text(&text);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text)
                            .size(11.5)
                            .color(if worn.is_empty() { p.muted_dim } else { p.ink }),
                    )
                    .truncate(),
                );
                ui.add_space(4.0);
                if response.clicked() {
                    match worn.first() {
                        Some(tile) => select = Some(tile.item),
                        None => open = Some(key.to_owned()),
                    }
                }
            }
        });
    if let Some(key) = open {
        w.filter.rail = Rail::Category(key, None);
    }
    if let Some(item) = select {
        w.selected = Some(item);
    }
}
