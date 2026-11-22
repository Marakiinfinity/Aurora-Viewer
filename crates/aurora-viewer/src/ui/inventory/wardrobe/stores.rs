//! The stores of the Wardrobe (docs/WARDROBE.md, section 8): the list of
//! every store the PC knows, and the sheet that makes or edits one (name,
//! Marketplace page, main store, logo) and links it to one item or to all
//! the items of a creator. A store read from a creator's card is theirs to
//! describe: it is shown, never edited here.

use super::framing::Target;
use super::*;
use crate::world::wardrobe::card;
use crate::world::wardrobe::store::{Source, Store};

/// A store being made or edited.
pub(super) struct Edit {
    /// The record edited; `None` makes a new one.
    key: Option<Uuid>,
    /// The item the sheet was opened from, and its creator: what Save
    /// links to the store. `None` from the list of stores.
    item: Option<Uuid>,
    creator: Uuid,
    name: String,
    marketplace: String,
    mainstore: String,
    logo: Option<Uuid>,
    pub(super) logo_file: Option<String>,
    /// Every item of the creator, or this item alone.
    all: bool,
    /// A creator's own store, from their card: shown as it is.
    theirs: bool,
}

impl Edit {
    fn fill(&mut self, store: &Store) {
        self.key = Some(store.key);
        self.name.clone_from(&store.name);
        self.marketplace = store.marketplace.clone().unwrap_or_default();
        self.mainstore = store.mainstore.clone().unwrap_or_default();
        self.logo = store.logo;
        self.logo_file.clone_from(&store.logo_file);
        self.theirs = store.source == Source::Card;
    }

    fn blank(item: Option<Uuid>, creator: Uuid) -> Self {
        Edit {
            key: None,
            item,
            creator,
            name: String::new(),
            marketplace: String::new(),
            mainstore: String::new(),
            logo: None,
            logo_file: None,
            all: !creator.is_nil(),
            theirs: false,
        }
    }

    /// "Link a store…" on an item: its store when it has one, else a new one.
    pub(super) fn for_item(item: Uuid, creator: Uuid, store: Option<Uuid>) -> Self {
        let mut edit = Edit::blank(Some(item), creator);
        wardrobe::with(|shared| {
            if let Some(store) = store.and_then(|key| shared.wardrobe.catalogue.stores.get(&key)) {
                edit.fill(store);
            }
            // linked on its own already: the sheet says so
            if shared.wardrobe.account.bindings.get(&item).is_some_and(|b| b.store.is_some()) {
                edit.all = false;
            }
        });
        edit
    }

    /// A store of the list, or a new one.
    pub(super) fn for_store(store: Option<&Store>) -> Self {
        let mut edit = Edit::blank(None, Uuid::nil());
        if let Some(store) = store {
            edit.fill(store);
        }
        edit
    }
}

/// The list of stores: its search text.
#[derive(Default)]
pub(super) struct List {
    pub(super) open: bool,
    search: String,
}

/// The mark of a store by its record: logo, else initials.
fn mark(
    ui: &egui::Ui,
    p: &Palette,
    rect: Rect,
    name: &str,
    logo: Option<Uuid>,
    logo_file: Option<&String>,
    st: &mut InventoryUi,
    images: &Images,
) {
    let tile = model::Tile {
        store: Some(name.to_owned()),
        logo,
        logo_file: logo_file.cloned(),
        ..model::Tile::default()
    };
    grid::store_mark(ui, p, rect, &tile, images, &mut st.wanted_images);
}

type Images = std::collections::HashMap<Uuid, egui::TextureHandle>;

/// The list of every store, with how many items each has.
pub(super) fn list(ctx: &egui::Context, p: &Palette, st: &mut InventoryUi, images: &Images) {
    if !st.wardrobe.stores.open || st.wardrobe.store_edit.is_some() {
        return;
    }
    let stores: Vec<Store> = wardrobe::with(|shared| {
        let mut stores: Vec<Store> = shared.wardrobe.catalogue.stores.values().cloned().collect();
        stores.sort_by_key(|s| s.name.to_lowercase());
        stores
    });
    let mut edit: Option<Edit> = None;
    let mut close = false;
    let modal = widgets::Modal::new(egui::Id::unique("wardrobe_stores")).show(ctx, |ui| {
        ui.set_width(400.0);
        widgets::section_header(ui, p, "building-store", aurora_i18n::tr!("wardrobe-stores-title"));
        ui.label(
            egui::RichText::new(aurora_i18n::tr!("wardrobe-stores-body"))
                .size(12.0)
                .color(p.muted),
        );
        ui.add_space(6.0);
        let width = ui.available_width();
        ui.horizontal(|ui| {
            widgets::clearable_search_box(
                ui,
                p,
                &mut st.wardrobe.stores.search,
                aurora_i18n::tr!("wardrobe-stores-search"),
                width,
            )
        });
        ui.add_space(6.0);
        let needle = st.wardrobe.stores.search.to_lowercase();
        egui::ScrollArea::vertical()
            .id_salt("wardrobe_stores_list")
            .max_height(320.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let shown: Vec<&Store> = stores
                    .iter()
                    .filter(|s| needle.is_empty() || s.name.to_lowercase().contains(&needle))
                    .collect();
                if shown.is_empty() {
                    ui.label(
                        egui::RichText::new(aurora_i18n::tr!("wardrobe-stores-none"))
                            .size(12.0)
                            .color(p.muted),
                    );
                }
                for store in shown {
                    let count = st.wardrobe.model.tiles.iter().filter(|t| t.store_key == Some(store.key)).count();
                    let (row, response) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
                    if response.hovered() {
                        ui.painter().rect_filled(row, ui.visuals().widgets.inactive.corner_radius, p.raised);
                    }
                    let at = Rect::from_min_size(row.left_center() + Vec2::new(4.0, -12.0), Vec2::splat(24.0));
                    mark(ui, p, at, &store.name, store.logo, store.logo_file.as_ref(), st, images);
                    ui.painter().text(
                        egui::pos2(at.right() + 8.0, row.center().y - 7.0),
                        egui::Align2::LEFT_CENTER,
                        &store.name,
                        egui::FontId::proportional(12.5),
                        p.ink,
                    );
                    let source = if store.source == Source::Card {
                        aurora_i18n::tr!("wardrobe-stores-theirs")
                    } else {
                        aurora_i18n::tr!("wardrobe-stores-yours")
                    };
                    ui.painter().text(
                        egui::pos2(at.right() + 8.0, row.center().y + 8.0),
                        egui::Align2::LEFT_CENTER,
                        format!("{source} · {}", aurora_i18n::tr!("wardrobe-count", count = count)),
                        egui::FontId::proportional(10.5),
                        p.muted,
                    );
                    if response.clicked() {
                        edit = Some(Edit::for_store(Some(store)));
                    }
                }
            });
        ui.add_space(10.0);
        let buttons = [
            widgets::DialogButton {
                icon: "",
                label: aurora_i18n::tr!("common-close"),
                kind: widgets::ActionKind::Flat,
                enabled: true,
            },
            widgets::DialogButton {
                icon: "plus",
                label: aurora_i18n::tr!("wardrobe-stores-new"),
                kind: widgets::ActionKind::Primary,
                enabled: true,
            },
        ];
        match widgets::dialog_buttons(ui, p, &buttons, 28.0) {
            Some(0) => close = true,
            Some(_) => edit = Some(Edit::for_store(None)),
            None => {}
        }
    });
    if let Some(edit) = edit {
        st.wardrobe.store_edit = Some(edit);
    } else if close || modal.should_close() {
        st.wardrobe.stores.open = false;
    }
}

enum Answer {
    Open,
    Close,
    Save,
    /// The item or the creator leaves the store; the store stays.
    Unlink,
    /// The store itself is deleted, from the list.
    Delete,
}

/// The sheet of one store.
pub(super) fn sheet(ctx: &egui::Context, p: &Palette, st: &mut InventoryUi, facts: &Facts, images: &Images) {
    let Some(mut edit) = st.wardrobe.store_edit.take() else { return };
    let mut answer = Answer::Open;
    // the sheet names the creator it links
    if !edit.creator.is_nil() {
        st.wardrobe.wanted_names.insert(edit.creator);
    }
    // a sheet under the framing dialog waits for it
    let framing = st.wardrobe.framing.is_some() || st.wardrobe.picture_job.is_some();
    let known: Vec<Store> = wardrobe::with(|shared| {
        let mut stores: Vec<Store> = shared
            .wardrobe
            .catalogue
            .stores
            .values()
            .filter(|s| Some(s.key) != edit.key)
            .cloned()
            .collect();
        stores.sort_by_key(|s| s.name.to_lowercase());
        stores
    });
    let linked = edit.item.is_some_and(|item| {
        wardrobe::with(|shared| {
            shared.wardrobe.account.bindings.get(&item).is_some_and(|b| b.store.is_some())
                || shared.wardrobe.catalogue.creator_stores.contains_key(&edit.creator)
        })
    });
    let modal = widgets::Modal::new(egui::Id::unique("wardrobe_store")).show(ctx, |ui| {
        ui.set_width(400.0);
        let heading = match (edit.item, edit.key) {
            (Some(_), _) => aurora_i18n::tr!("wardrobe-store-link").trim_end_matches('…').to_owned(),
            (None, Some(_)) => aurora_i18n::tr!("wardrobe-store-edit").to_owned(),
            (None, None) => aurora_i18n::tr!("wardrobe-stores-new").to_owned(),
        };
        widgets::section_header(ui, p, "building-store", &heading);
        if edit.theirs {
            ui.label(
                egui::RichText::new(aurora_i18n::tr!("wardrobe-store-theirs"))
                    .size(12.0)
                    .color(p.warn),
            );
        } else {
            ui.label(
                egui::RichText::new(if crate::world::wardrobe::CARDS {
                    aurora_i18n::tr!("wardrobe-store-link-body")
                } else {
                    aurora_i18n::tr!("wardrobe-store-link-body-local")
                })
                .size(12.0)
                .color(p.muted),
            );
        }
        ui.add_space(6.0);
        // a store already known: linked in one click
        if edit.item.is_some() && !known.is_empty() {
            let button = egui::Button::new(egui::RichText::new(format!("{}  ⏷", aurora_i18n::tr!("wardrobe-store-existing"))).size(12.0));
            menu::button(ui, p, button, |ui| {
                for store in known.iter().take(80) {
                    if menu::item(ui, p, "building-store", &store.name) {
                        edit.fill(store);
                    }
                }
            });
            ui.add_space(4.0);
        }
        ui.add_enabled_ui(!edit.theirs, |ui| {
            ui.horizontal(|ui| {
                // the logo, and where a new one comes from
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
                if edit.name.trim().is_empty() && edit.logo.is_none() && edit.logo_file.is_none() {
                    ui.painter().rect_filled(rect, ui.visuals().window_corner_radius, p.raised);
                    widgets::paint_glyph(ui.painter(), "building-store", rect.center(), 22.0, p.muted_dim);
                } else {
                    mark(ui, p, rect, edit.name.trim(), edit.logo, edit.logo_file.as_ref(), st, images);
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(egui::RichText::new(aurora_i18n::tr!("wardrobe-store-logo")).size(12.0).color(p.ink));
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                        for (from, key) in [
                            (pictures::From::File, "wardrobe-picture-file"),
                            (pictures::From::Clipboard, "wardrobe-picture-paste"),
                        ] {
                            if ui
                                .add_enabled(!framing, egui::Button::new(egui::RichText::new(aurora_i18n::text(key)).size(12.0)))
                                .clicked()
                            {
                                st.wardrobe.picture_job = Some((Target::Logo, pictures::choose(ui.ctx(), from)));
                            }
                        }
                        if (edit.logo_file.is_some() || edit.logo.is_some())
                            && widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-store-logo-remove")).clicked()
                        {
                            edit.logo_file = None;
                            edit.logo = None;
                        }
                    });
                    ui.label(
                        egui::RichText::new(aurora_i18n::tr!("wardrobe-store-logo-note"))
                            .size(10.5)
                            .color(p.muted),
                    );
                });
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(aurora_i18n::tr!("wardrobe-store-name")).size(12.0).color(p.ink));
            ui.add(
                egui::TextEdit::singleline(&mut edit.name)
                    .desired_width(f32::INFINITY)
                    .char_limit(64),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(aurora_i18n::tr!("wardrobe-store-marketplace"))
                    .size(12.0)
                    .color(p.ink),
            );
            ui.add(
                egui::TextEdit::singleline(&mut edit.marketplace)
                    .desired_width(f32::INFINITY)
                    .hint_text("https://marketplace.secondlife.com/stores/…"),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(aurora_i18n::tr!("wardrobe-review-mainstore"))
                    .size(12.0)
                    .color(p.ink),
            );
            ui.add(
                egui::TextEdit::singleline(&mut edit.mainstore)
                    .desired_width(f32::INFINITY)
                    .hint_text("secondlife://Region/128/128/25"),
            );
        });
        let bad_market = !edit.marketplace.trim().is_empty() && !card::valid_marketplace(edit.marketplace.trim());
        let bad_store = !edit.mainstore.trim().is_empty() && !card::valid_mainstore(edit.mainstore.trim());
        if bad_market {
            ui.label(
                egui::RichText::new(aurora_i18n::tr!("wardrobe-store-bad-marketplace"))
                    .size(11.0)
                    .color(p.warn),
            );
        }
        if bad_store {
            ui.label(
                egui::RichText::new(aurora_i18n::tr!("wardrobe-store-bad-mainstore"))
                    .size(11.0)
                    .color(p.warn),
            );
        }
        // what the store is linked to
        if edit.item.is_some() {
            ui.add_space(8.0);
            let count = st.wardrobe.model.tiles.iter().filter(|t| t.creator == edit.creator).count();
            let who = facts.names.get(&edit.creator).map_or("…", String::as_str);
            if edit.creator.is_nil() {
                edit.all = false;
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-store-scope-unknown"))
                        .size(11.5)
                        .color(p.muted),
                );
            } else {
                widgets::radio_value(
                    ui,
                    p,
                    &mut edit.all,
                    true,
                    aurora_i18n::tr!("wardrobe-store-scope-all", creator = who, count = count),
                );
                widgets::radio_value(ui, p, &mut edit.all, false, aurora_i18n::tr!("wardrobe-store-scope-one"));
            }
        }
        ui.add_space(10.0);
        let can_save = !edit.name.trim().is_empty() && !bad_market && !bad_store && !framing;
        let mut buttons = vec![widgets::DialogButton {
            icon: "",
            label: aurora_i18n::tr!("common-cancel"),
            kind: widgets::ActionKind::Flat,
            enabled: true,
        }];
        let second = if edit.item.is_some() && linked {
            Some((aurora_i18n::tr!("wardrobe-store-unlink"), widgets::ActionKind::Flat, Answer::Unlink))
        } else if edit.item.is_none() && edit.key.is_some() {
            Some((
                aurora_i18n::tr!("wardrobe-store-delete"),
                widgets::ActionKind::Danger,
                Answer::Delete,
            ))
        } else {
            None
        };
        if let Some((label, kind, _)) = &second {
            buttons.push(widgets::DialogButton {
                icon: "",
                label,
                kind: *kind,
                enabled: true,
            });
        }
        buttons.push(widgets::DialogButton {
            icon: "",
            label: aurora_i18n::tr!("common-save"),
            kind: widgets::ActionKind::Primary,
            enabled: can_save,
        });
        match widgets::dialog_buttons(ui, p, &buttons, 28.0) {
            Some(0) => answer = Answer::Close,
            Some(n) if n + 1 == buttons.len() => answer = Answer::Save,
            Some(_) => answer = second.map_or(Answer::Close, |(_, _, answer)| answer),
            None => {}
        }
    });
    if modal.should_close() && matches!(answer, Answer::Open) && !framing {
        answer = Answer::Close;
    }
    match answer {
        Answer::Open => st.wardrobe.store_edit = Some(edit),
        Answer::Close => {}
        Answer::Save => save(st, edit),
        Answer::Unlink => {
            wardrobe::with(|shared| {
                shared.wardrobe.change(false, |txn| {
                    if let Some(item) = edit.item
                        && txn.account().bindings.get(&item).is_some_and(|b| b.store.is_some())
                    {
                        txn.binding(item).store = None;
                    } else {
                        txn.link_creator(edit.creator, None);
                    }
                })
            });
        }
        Answer::Delete => {
            if let Some(key) = edit.key {
                wardrobe::with(|shared| shared.wardrobe.change(true, |txn| txn.remove_store(key)));
                st.wardrobe
                    .say(aurora_i18n::tr!("wardrobe-store-deleted", name = edit.name.trim()), true);
            }
        }
    }
}

/// Saves the store and links what the sheet was opened from. Said in the
/// Wardrobe's line, so that the change is seen even when the card shown
/// does not change.
fn save(st: &mut InventoryUi, edit: Edit) {
    let filled = |text: &str| Some(text.trim().to_owned()).filter(|t| !t.is_empty());
    let key = edit.key.unwrap_or_else(Uuid::new_v4);
    let name = edit.name.trim().to_owned();
    let old_logo = wardrobe::with(|shared| {
        shared.wardrobe.change(false, |txn| {
            let before = txn.catalogue().stores.get(&key).cloned();
            // a creator's own store is theirs: linked to, never rewritten
            if !edit.theirs {
                txn.set_store(Store {
                    key,
                    name: name.clone(),
                    logo: edit.logo,
                    logo_file: edit.logo_file.clone(),
                    marketplace: filled(&edit.marketplace),
                    mainstore: filled(&edit.mainstore),
                    group: before.as_ref().and_then(|s| s.group),
                    source: Source::User,
                    card_version: None,
                });
            }
            if let Some(item) = edit.item {
                if edit.all && !edit.creator.is_nil() {
                    txn.link_creator(edit.creator, Some(key));
                    if txn.account().bindings.get(&item).is_some_and(|b| b.store.is_some()) {
                        txn.binding(item).store = None;
                    }
                } else if txn.account().bindings.contains_key(&item) {
                    txn.binding(item).store = Some(key);
                }
            }
            // the logo this save replaced, when no store shows it any more
            before
                .and_then(|s| s.logo_file)
                .filter(|file| Some(file) != edit.logo_file.as_ref())
                .filter(|file| !txn.catalogue().stores.values().any(|s| s.logo_file.as_ref() == Some(file)))
        })
    });
    if let Some(file) = old_logo {
        pictures::discard(pictures::Kind::Logo, &file);
    }
    st.wardrobe
        .say(aurora_i18n::tr!("wardrobe-store-saved", name = name.as_str()), false);
}
