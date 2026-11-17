//! The small dialogs of the Wardrobe (docs/WARDROBE.md, sections 7 and 8):
//! the categories of an item, what it was created for, its note and its
//! picture, "Same product as…", its HUDs, a store linked to a creator, and
//! the items left unsorted. Everything set here is the user's own value: it
//! wins over any card and stays on this computer.

use super::*;
use crate::world::wardrobe::categories::{MAX_CATEGORIES, MAX_SUBTYPES, OTHER, Tag};
use crate::world::wardrobe::store::{Source, Store};
use crate::world::wardrobe::{card, matching};

pub(super) enum Dialog {
    Categories {
        item: Uuid,
        tags: Vec<Tag>,
    },
    Fits {
        item: Uuid,
        fits: String,
        kit: String,
    },
    Note {
        item: Uuid,
        text: String,
    },
    Picture {
        item: Uuid,
    },
    SameProduct {
        item: Uuid,
        search: String,
    },
    LinkHud {
        item: Uuid,
        search: String,
    },
    LinkStore {
        creator: Uuid,
        name: String,
        marketplace: String,
        mainstore: String,
    },
    Unsorted {
        search: String,
        category: usize,
    },
}

impl Dialog {
    pub(super) fn categories(item: Uuid, tags: &[Tag]) -> Self {
        Dialog::Categories { item, tags: tags.to_vec() }
    }

    pub(super) fn fits(item: Uuid, fits: &[String], kit: &[String]) -> Self {
        Dialog::Fits {
            item,
            fits: fits.join(", "),
            kit: kit.join(", "),
        }
    }

    pub(super) fn note(item: Uuid, text: &str) -> Self {
        Dialog::Note {
            item,
            text: text.to_owned(),
        }
    }

    pub(super) fn picture(item: Uuid) -> Self {
        Dialog::Picture { item }
    }

    pub(super) fn same_product(item: Uuid) -> Self {
        Dialog::SameProduct {
            item,
            search: String::new(),
        }
    }

    pub(super) fn link_hud(item: Uuid) -> Self {
        Dialog::LinkHud {
            item,
            search: String::new(),
        }
    }

    pub(super) fn link_store(creator: Uuid) -> Self {
        let known = wardrobe::with(|shared| shared.wardrobe.catalogue.stores.get(&creator).cloned());
        Dialog::LinkStore {
            creator,
            name: known.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
            marketplace: known.as_ref().and_then(|s| s.marketplace.clone()).unwrap_or_default(),
            mainstore: known.and_then(|s| s.mainstore).unwrap_or_default(),
        }
    }

    pub(super) fn unsorted() -> Self {
        Dialog::Unsorted {
            search: String::new(),
            category: 0,
        }
    }
}

/// What a dialog's buttons answered.
enum Answer {
    Open,
    Save,
    /// Back to what the card says.
    Reset,
    Close,
}

fn buttons(ui: &mut egui::Ui, p: &Palette, save: &str, can_save: bool, reset: Option<&str>) -> Answer {
    ui.add_space(10.0);
    let mut list = vec![widgets::DialogButton {
        icon: "",
        label: aurora_i18n::tr!("common-cancel"),
        kind: widgets::ActionKind::Flat,
        enabled: true,
    }];
    if let Some(reset) = reset {
        list.push(widgets::DialogButton {
            icon: "",
            label: reset,
            kind: widgets::ActionKind::Flat,
            enabled: true,
        });
    }
    list.push(widgets::DialogButton {
        icon: "",
        label: save,
        kind: widgets::ActionKind::Primary,
        enabled: can_save,
    });
    match widgets::dialog_buttons(ui, p, &list, 28.0) {
        Some(0) => Answer::Close,
        Some(n) if n + 1 == list.len() => Answer::Save,
        Some(_) => Answer::Reset,
        None => Answer::Open,
    }
}

fn names(list: &str) -> Vec<String> {
    list.split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .take(card::MAX_NAMES)
        .map(str::to_owned)
        .collect()
}

/// A list of items to pick one from, filtered by `search`. Returns the one
/// clicked.
fn pick_list(ui: &mut egui::Ui, p: &Palette, search: &mut String, rows: &[(Uuid, String, String)]) -> Option<Uuid> {
    widgets::clearable_search_box(ui, p, search, aurora_i18n::tr!("wardrobe-search-names"), ui.available_width());
    ui.add_space(6.0);
    let needle = search.to_lowercase();
    let mut picked = None;
    egui::ScrollArea::vertical()
        .id_salt("wardrobe_pick")
        .max_height(260.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let shown: Vec<_> = rows
                .iter()
                .filter(|(_, name, _)| needle.is_empty() || name.to_lowercase().contains(&needle))
                .take(200)
                .collect();
            if shown.is_empty() {
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-empty-filter"))
                        .size(12.0)
                        .color(p.muted),
                );
            }
            for (id, name, sub) in shown {
                let response = widgets::rail_row(ui, p, "", p.muted, name, None, false, 0);
                let response = if sub.is_empty() { response } else { response.on_hover_text(sub) };
                if response.clicked() {
                    picked = Some(*id);
                }
            }
        });
    picked
}

pub(super) fn show(
    ctx: &egui::Context,
    p: &Palette,
    inv: &Inventory,
    st: &mut InventoryUi,
    facts: &Facts,
    images: &std::collections::HashMap<Uuid, egui::TextureHandle>,
) {
    let Some(mut dialog) = st.wardrobe.dialog.take() else { return };
    let mut answer = Answer::Open;
    let item_name = |id: &Uuid| inv.items.get(id).map_or(String::new(), |it| it.name.clone());
    let modal = widgets::Modal::new(egui::Id::unique("wardrobe_dialog")).show(ctx, |ui| {
        ui.set_width(380.0);
        match &mut dialog {
            Dialog::Categories { item, tags } => {
                widgets::section_header(ui, p, "tag-filled", aurora_i18n::tr!("wardrobe-menu-categories"));
                ui.label(egui::RichText::new(item_name(item)).size(12.0).color(p.muted));
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .id_salt("wardrobe_categories")
                    .max_height(340.0)
                    .show(ui, |ui| {
                        for category in categories::WARDROBE {
                            let at = tags.iter().position(|t| t.category() == category.key);
                            let mut on = at.is_some();
                            ui.horizontal(|ui| {
                                let changed = widgets::checkbox(ui, p, &mut on, aurora_i18n::text(category.label)).changed();
                                if changed && on && tags.len() < MAX_CATEGORIES {
                                    tags.push(Tag(category.key.to_owned(), Vec::new()));
                                } else if changed
                                    && !on
                                    && let Some(at) = at
                                {
                                    tags.remove(at);
                                }
                                match at {
                                    Some(0) => {
                                        ui.label(
                                            egui::RichText::new(aurora_i18n::tr!("wardrobe-categories-main"))
                                                .size(10.5)
                                                .color(p.violet_light),
                                        );
                                    }
                                    Some(at)
                                        if on
                                            && widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-categories-make-main")).clicked() =>
                                    {
                                        let tag = tags.remove(at);
                                        tags.insert(0, tag);
                                    }
                                    _ => {}
                                }
                            });
                            let Some(tag) = tags.iter_mut().find(|t| t.category() == category.key) else {
                                continue;
                            };
                            ui.horizontal_wrapped(|ui| {
                                ui.add_space(22.0);
                                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                                for subtype in category.subtypes.iter().filter(|s| s.key != OTHER) {
                                    let chosen = tag.1.iter().any(|s| s == subtype.key);
                                    if widgets::filter_pill(ui, p, "", aurora_i18n::text(subtype.label), chosen).clicked() {
                                        if chosen {
                                            tag.1.retain(|s| s != subtype.key);
                                        } else if tag.1.len() < MAX_SUBTYPES {
                                            tag.1.push(subtype.key.to_owned());
                                        }
                                    }
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-categories-caps"))
                        .size(11.0)
                        .color(p.muted),
                );
                answer = buttons(
                    ui,
                    p,
                    aurora_i18n::tr!("common-save"),
                    !tags.is_empty(),
                    Some(aurora_i18n::tr!("wardrobe-reset-card")),
                );
            }
            Dialog::Fits { item, fits, kit } => {
                widgets::section_header(ui, p, "man-filled", aurora_i18n::tr!("wardrobe-menu-fits"));
                ui.label(egui::RichText::new(item_name(item)).size(12.0).color(p.muted));
                ui.add_space(6.0);
                ui.label(egui::RichText::new(aurora_i18n::tr!("wardrobe-fits-field")).size(12.0).color(p.ink));
                ui.add(
                    egui::TextEdit::singleline(fits)
                        .desired_width(f32::INFINITY)
                        .hint_text(aurora_i18n::tr!("wardrobe-fits-hint")),
                );
                // the kits the account knows, a click away
                let known = wardrobe::with(|shared| {
                    shared
                        .index
                        .kits
                        .list
                        .iter()
                        .filter(|k| k.worn || k.owned)
                        .map(|k| (k.name.clone(), k.worn))
                        .collect::<Vec<_>>()
                });
                if !known.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                        for (name, worn) in known.iter().take(24) {
                            let listed = names(fits).iter().any(|n| n.eq_ignore_ascii_case(name));
                            if widgets::filter_pill(ui, p, if *worn { "check" } else { "" }, name, listed).clicked() {
                                let mut list = names(fits);
                                if listed {
                                    list.retain(|n| !n.eq_ignore_ascii_case(name));
                                } else {
                                    list.push(name.clone());
                                }
                                *fits = list.join(", ");
                            }
                        }
                    });
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(aurora_i18n::tr!("wardrobe-kit-field")).size(12.0).color(p.ink));
                ui.add(
                    egui::TextEdit::singleline(kit)
                        .desired_width(f32::INFINITY)
                        .hint_text(aurora_i18n::tr!("wardrobe-kit-hint")),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-fits-note"))
                        .size(11.0)
                        .color(p.muted),
                );
                answer = buttons(
                    ui,
                    p,
                    aurora_i18n::tr!("common-save"),
                    true,
                    Some(aurora_i18n::tr!("wardrobe-reset-card")),
                );
            }
            Dialog::Note { item, text } => {
                widgets::section_header(ui, p, "pencil-filled", aurora_i18n::tr!("wardrobe-menu-note"));
                ui.label(egui::RichText::new(item_name(item)).size(12.0).color(p.muted));
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::singleline(text)
                        .desired_width(f32::INFINITY)
                        .char_limit(card::MAX_NOTE),
                );
                answer = buttons(
                    ui,
                    p,
                    aurora_i18n::tr!("common-save"),
                    true,
                    Some(aurora_i18n::tr!("wardrobe-reset-card")),
                );
            }
            Dialog::Picture { item } => {
                widgets::section_header(ui, p, "photo-filled", aurora_i18n::tr!("wardrobe-menu-picture"));
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-picture-body"))
                        .size(12.0)
                        .color(p.muted),
                );
                ui.add_space(6.0);
                // the textures of the item's own folder: the pictures a
                // product box ships
                let folder = inv.items.get(item).map(|it| it.parent);
                let textures: Vec<&aurora_net::inventory::InvItem> = folder
                    .and_then(|f| inv.folders.get(&f))
                    .into_iter()
                    .flat_map(|f| f.items.iter())
                    .filter_map(|id| inv.items.get(id))
                    .filter(|it| it.asset_type == 0 && !it.asset_id.is_nil())
                    .collect();
                let mut chosen: Option<Option<Uuid>> = None;
                if textures.is_empty() {
                    ui.label(
                        egui::RichText::new(aurora_i18n::tr!("wardrobe-picture-none"))
                            .size(12.0)
                            .color(p.muted),
                    );
                }
                egui::ScrollArea::vertical()
                    .id_salt("wardrobe_pictures")
                    .max_height(280.0)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(6.0);
                            for texture in &textures {
                                let (rect, response) = ui.allocate_exact_size(Vec2::splat(80.0), Sense::click());
                                let radius = ui.visuals().window_corner_radius;
                                ui.painter().rect_filled(rect, radius, p.raised);
                                match images.get(&texture.asset_id) {
                                    Some(handle) => widgets::square_picture(ui, rect, handle),
                                    None => {
                                        st.wanted_images.insert(texture.asset_id);
                                    }
                                }
                                if response.hovered() {
                                    ui.painter().rect_stroke(
                                        rect,
                                        radius,
                                        egui::Stroke::new(1.0, p.violet_light),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                if response.on_hover_text(&texture.name).clicked() {
                                    chosen = Some(Some(texture.id));
                                }
                            }
                        });
                    });
                ui.add_space(6.0);
                if widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-picture-own")).clicked() {
                    chosen = Some(None);
                }
                if let Some(picture) = chosen {
                    let item = *item;
                    wardrobe::with(|shared| {
                        shared.wardrobe.change(false, |txn| {
                            let binding = txn.binding(item);
                            binding.picture = picture;
                            binding.picture_by_user = picture.is_some();
                        })
                    });
                    answer = Answer::Close;
                } else {
                    answer = buttons(ui, p, aurora_i18n::tr!("common-close"), true, None);
                }
            }
            Dialog::SameProduct { item, search } => {
                widgets::section_header(ui, p, "copy-filled", aurora_i18n::tr!("wardrobe-menu-same-product"));
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-same-body", name = item_name(item)))
                        .size(12.0)
                        .color(p.muted),
                );
                ui.add_space(6.0);
                let rows: Vec<(Uuid, String, String)> = st
                    .wardrobe
                    .model
                    .tiles
                    .iter()
                    .filter(|t| t.pack.is_some() && !t.copies.contains(item))
                    .map(|t| (t.item, t.name.clone(), t.pack.clone().unwrap_or_default()))
                    .collect();
                if let Some(other) = pick_list(ui, p, search, &rows) {
                    let item = *item;
                    let joined = wardrobe::with(|shared| shared.wardrobe.change(true, |txn| matching::same_product(txn, item, other)));
                    if joined {
                        st.wardrobe
                            .say(aurora_i18n::tr!("wardrobe-same-done", name = item_name(&other)), true);
                    }
                    answer = Answer::Close;
                } else {
                    answer = buttons(ui, p, aurora_i18n::tr!("common-close"), true, None);
                }
            }
            Dialog::LinkHud { item, search } => {
                widgets::section_header(ui, p, "adjustments-filled", aurora_i18n::tr!("wardrobe-menu-link-hud"));
                let item = *item;
                let current = st.wardrobe.model.tile_of(&item).map(|t| t.huds.clone()).unwrap_or_default();
                // the HUDs the rules found: each can be said not to be one
                for hud in &current {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(item_name(hud)).size(12.0).color(p.ink));
                        if widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-hud-not")).clicked() {
                            let hud = *hud;
                            wardrobe::with(|shared| {
                                shared.wardrobe.change(false, |txn| {
                                    let account = txn.account_mut();
                                    account.hud_links.entry(item).or_default().retain(|id| *id != hud);
                                    let excluded = account.hud_excluded.entry(item).or_default();
                                    if !excluded.contains(&hud) {
                                        excluded.push(hud);
                                    }
                                })
                            });
                        }
                    });
                }
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-hud-link-body"))
                        .size(12.0)
                        .color(p.muted),
                );
                let folder = inv.items.get(&item).map(|it| it.parent);
                let mut rows: Vec<(Uuid, String, String)> = inv
                    .items
                    .values()
                    .filter(|it| it.asset_type == 6 && it.id != item && !current.contains(&it.id))
                    .filter(|it| {
                        Some(it.parent) == folder || (!search.is_empty() && inv.folders.get(&it.parent).is_some_and(|f| !f.library))
                    })
                    .map(|it| {
                        (
                            it.id,
                            it.name.clone(),
                            inv.folders.get(&it.parent).map(|f| f.info.name.clone()).unwrap_or_default(),
                        )
                    })
                    .collect();
                rows.sort_by_key(|row| crate::natural_order::key(&row.1));
                if let Some(hud) = pick_list(ui, p, search, &rows) {
                    wardrobe::with(|shared| {
                        shared.wardrobe.change(false, |txn| {
                            let account = txn.account_mut();
                            account.hud_excluded.entry(item).or_default().retain(|id| *id != hud);
                            let links = account.hud_links.entry(item).or_default();
                            if !links.contains(&hud) {
                                links.push(hud);
                            }
                        })
                    });
                }
                answer = buttons(ui, p, aurora_i18n::tr!("common-close"), true, None);
            }
            Dialog::LinkStore {
                creator,
                name,
                marketplace,
                mainstore,
            } => {
                let who = facts.names.get(creator).map_or("…", String::as_str);
                widgets::section_header(
                    ui,
                    p,
                    "building-store",
                    &aurora_i18n::tr!("wardrobe-store-link-title", creator = who),
                );
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-store-link-body"))
                        .size(12.0)
                        .color(p.muted),
                );
                ui.add_space(6.0);
                ui.label(egui::RichText::new(aurora_i18n::tr!("wardrobe-store-name")).size(12.0).color(p.ink));
                ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY).char_limit(64));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-store-marketplace"))
                        .size(12.0)
                        .color(p.ink),
                );
                ui.add(
                    egui::TextEdit::singleline(marketplace)
                        .desired_width(f32::INFINITY)
                        .hint_text("https://marketplace.secondlife.com/stores/…"),
                );
                let bad_market = !marketplace.trim().is_empty() && !card::valid_marketplace(marketplace.trim());
                if bad_market {
                    ui.label(
                        egui::RichText::new(aurora_i18n::tr!("wardrobe-store-bad-marketplace"))
                            .size(11.0)
                            .color(p.warn),
                    );
                }
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-review-mainstore"))
                        .size(12.0)
                        .color(p.ink),
                );
                ui.add(
                    egui::TextEdit::singleline(mainstore)
                        .desired_width(f32::INFINITY)
                        .hint_text("secondlife://Region/128/128/25"),
                );
                let bad_store = !mainstore.trim().is_empty() && !card::valid_mainstore(mainstore.trim());
                if bad_store {
                    ui.label(
                        egui::RichText::new(aurora_i18n::tr!("wardrobe-store-bad-mainstore"))
                            .size(11.0)
                            .color(p.warn),
                    );
                }
                let count = st.wardrobe.model.tiles.iter().filter(|t| t.creator == *creator).count();
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-store-link-all", count = count))
                        .size(11.0)
                        .color(p.muted),
                );
                answer = buttons(
                    ui,
                    p,
                    aurora_i18n::tr!("common-save"),
                    !name.trim().is_empty() && !bad_market && !bad_store,
                    Some(aurora_i18n::tr!("wardrobe-store-unlink")),
                );
            }
            Dialog::Unsorted { search, category } => {
                widgets::section_header(ui, p, "plus", aurora_i18n::tr!("wardrobe-unsorted-title"));
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-unsorted-body"))
                        .size(12.0)
                        .color(p.muted),
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                    for (n, entry) in categories::WARDROBE.iter().enumerate() {
                        if widgets::filter_pill(ui, p, category_icon(entry.key), aurora_i18n::text(entry.label), *category == n).clicked() {
                            *category = n;
                        }
                    }
                });
                ui.add_space(6.0);
                let rows: Vec<(Uuid, String, String)> = st
                    .wardrobe
                    .model
                    .unsorted
                    .iter()
                    .filter_map(|id| inv.items.get(id))
                    .map(|it| {
                        (
                            it.id,
                            it.name.clone(),
                            inv.folders.get(&it.parent).map(|f| f.info.name.clone()).unwrap_or_default(),
                        )
                    })
                    .collect();
                if let Some(item) = pick_list(ui, p, search, &rows) {
                    let key = categories::WARDROBE[(*category).min(categories::WARDROBE.len() - 1)].key;
                    add(item, vec![Tag::new(key, OTHER)]);
                }
                answer = buttons(ui, p, aurora_i18n::tr!("common-close"), true, None);
            }
        }
    });
    if modal.should_close() && matches!(answer, Answer::Open) {
        answer = Answer::Close;
    }
    match answer {
        Answer::Open => st.wardrobe.dialog = Some(dialog),
        Answer::Close => {}
        Answer::Save | Answer::Reset => {
            let reset = matches!(answer, Answer::Reset);
            wardrobe::with(|shared| {
                shared.wardrobe.change(false, |txn| match dialog {
                    Dialog::Categories { item, tags } => {
                        // an item added by hand joins the Wardrobe here
                        let fresh = !txn.account().bindings.contains_key(&item);
                        let binding = txn.binding(item);
                        binding.categories = (!reset).then(|| categories::normalise(tags));
                        if fresh {
                            binding.added = wardrobe::now();
                            binding.new = true;
                        }
                    }
                    Dialog::Fits { item, fits, kit } => {
                        let binding = txn.binding(item);
                        binding.fits = (!reset).then(|| names(&fits));
                        binding.kit = (!reset && !names(&kit).is_empty()).then(|| names(&kit));
                    }
                    Dialog::Note { item, text } => {
                        let text: String = text.trim().chars().take(card::MAX_NOTE).collect();
                        txn.binding(item).note = (!reset && !text.is_empty()).then_some(text);
                    }
                    Dialog::LinkStore { creator, .. } if reset => {
                        if txn.catalogue().stores.get(&creator).is_some_and(|s| s.source == Source::User) {
                            txn.remove_store(creator);
                        }
                    }
                    Dialog::LinkStore {
                        creator,
                        name,
                        marketplace,
                        mainstore,
                    } => {
                        // the creator's own card is never replaced by a link
                        if !txn.catalogue().stores.get(&creator).is_some_and(|s| s.source == Source::Card) {
                            let filled = |text: String| Some(text.trim().to_owned()).filter(|t| !t.is_empty());
                            txn.set_store(Store {
                                key: creator,
                                name: name.trim().to_owned(),
                                marketplace: filled(marketplace),
                                mainstore: filled(mainstore),
                                source: Source::User,
                                ..Store::default()
                            });
                        }
                    }
                    _ => {}
                })
            });
        }
    }
}

/// Adds an item to the Wardrobe by hand, with the categories the user
/// chose.
pub(in super::super) fn add(item: Uuid, tags: Vec<Tag>) {
    wardrobe::with(|shared| {
        shared.wardrobe.change(true, |txn| {
            let fresh = !txn.account().bindings.contains_key(&item);
            let binding = txn.binding(item);
            binding.categories = Some(categories::normalise(tags));
            if fresh {
                binding.added = wardrobe::now();
                binding.new = true;
            }
        })
    });
}
