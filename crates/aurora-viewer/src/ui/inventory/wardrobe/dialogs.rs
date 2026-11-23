//! The small dialogs of the Wardrobe (docs/WARDROBE.md, sections 7 and 8):
//! the categories of an item, what it was created for, its note and its
//! picture, "Same product as…", its HUDs, a store linked to a creator, and
//! the items left unsorted. Everything set here is the user's own value: it
//! wins over any card and stays on this computer.

use super::*;
use crate::world::wardrobe::categories::{MAX_CATEGORIES, MAX_SUBTYPES, OTHER, Part, Tag};
use crate::world::wardrobe::{card, matching};

pub(super) enum Dialog {
    Categories {
        item: Uuid,
        tags: Vec<Tag>,
    },
    Fits {
        item: Uuid,
        fits: String,
    },
    /// "Register as body, head or body part…": the item is a kit others are
    /// created for.
    Register {
        item: Uuid,
        part: Part,
        name: String,
        sizes: String,
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
    Unsorted {
        search: String,
        category: usize,
    },
}

impl Dialog {
    pub(super) fn categories(item: Uuid, tags: &[Tag]) -> Self {
        Dialog::Categories { item, tags: tags.to_vec() }
    }

    pub(super) fn fits(item: Uuid, fits: &[String]) -> Self {
        Dialog::Fits {
            item,
            fits: fits.join(", "),
        }
    }

    /// Opened on what the item already says, else on what its name tells.
    pub(super) fn register(item: Uuid, part: Part, kit: &[String], detected: Option<(&str, Part)>) -> Self {
        let (name, part) = match (kit.first(), detected) {
            (Some(name), _) => (name.clone(), part),
            (None, Some((name, part))) => (name.to_owned(), part),
            (None, None) => (String::new(), if part == Part::Other { Part::Body } else { part }),
        };
        Dialog::Register {
            item,
            part,
            name,
            sizes: kit.iter().skip(1).cloned().collect::<Vec<_>>().join(", "),
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

/// A menu row's label as a dialog's title: without its ellipsis.
fn title(label: &str) -> &str {
    label.trim_end_matches(['…', '.']).trim_end()
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
/// Whether a name holds a lower-case needle, without a copy of the name
/// for a needle of plain letters.
fn holds(name: &str, needle: &str) -> bool {
    if needle.is_ascii() {
        name.as_bytes()
            .windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
    } else {
        name.to_lowercase().contains(needle)
    }
}

fn pick_list(ui: &mut egui::Ui, p: &Palette, search: &mut String, rows: &[(Uuid, String, String)]) -> Option<Uuid> {
    // in a row of its own: the field centres its text in the height it is
    // given, and a column gives it all of it
    let width = ui.available_width();
    ui.horizontal(|ui| widgets::clearable_search_box(ui, p, search, aurora_i18n::tr!("wardrobe-search-names"), width));
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
    images: &std::collections::HashMap<Uuid, egui::TextureHandle>,
) {
    let Some(mut dialog) = st.wardrobe.dialog.take() else { return };
    let mut answer = Answer::Open;
    let item_name = |id: &Uuid| inv.items.get(id).map_or(String::new(), |it| it.name.clone());
    let modal = widgets::Modal::new(egui::Id::unique("wardrobe_dialog")).show(ctx, |ui| {
        ui.set_width(380.0);
        match &mut dialog {
            Dialog::Categories { item, tags } => {
                widgets::section_header(ui, p, "tag-filled", title(aurora_i18n::tr!("wardrobe-menu-categories")));
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
                                        // a click chooses this type in place of
                                        // the one set; with Ctrl or Shift it
                                        // joins it, up to two
                                        let adding = ui.input(|i| i.modifiers.command || i.modifiers.shift);
                                        if chosen {
                                            tag.1.retain(|s| s != subtype.key);
                                        } else if adding && tag.1.len() < MAX_SUBTYPES {
                                            tag.1.push(subtype.key.to_owned());
                                        } else {
                                            tag.1 = vec![subtype.key.to_owned()];
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
            Dialog::Fits { item, fits } => {
                widgets::section_header(ui, p, "man-filled", title(aurora_i18n::tr!("wardrobe-menu-fits")));
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
            Dialog::Register { item, part, name, sizes } => {
                widgets::section_header(ui, p, "man-filled", title(aurora_i18n::tr!("wardrobe-menu-register")));
                ui.label(egui::RichText::new(item_name(item)).size(12.0).color(p.muted));
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-register-body"))
                        .size(12.0)
                        .color(p.muted),
                );
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                    for kind in [
                        Part::Body,
                        Part::Head,
                        Part::Ears,
                        Part::Hands,
                        Part::Feet,
                        Part::Tail,
                        Part::Wings,
                        Part::AddOn,
                        Part::Other,
                    ] {
                        if widgets::filter_pill(ui, p, "", kind.label(), *part == kind).clicked() {
                            *part = kind;
                        }
                    }
                });
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-register-name"))
                        .size(12.0)
                        .color(p.ink),
                );
                ui.add(
                    egui::TextEdit::singleline(name)
                        .desired_width(f32::INFINITY)
                        .hint_text(aurora_i18n::tr!("wardrobe-register-name-hint"))
                        .char_limit(64),
                );
                // the bodies and heads the viewer knows, a click away
                let detected = inv.items.get(item).and_then(wardrobe::fit::preset_of);
                let presets: Vec<(&str, Part)> = wardrobe::fit::presets().into_iter().filter(|(_, kind)| kind == part).collect();
                if !presets.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                        for (preset, _) in presets {
                            let found = detected.is_some_and(|(known, _)| known == preset);
                            let on = name.trim().eq_ignore_ascii_case(preset);
                            if widgets::filter_pill(ui, p, if found { "check" } else { "" }, preset, on).clicked() {
                                *name = preset.to_owned();
                            }
                        }
                    });
                }
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-register-sizes"))
                        .size(12.0)
                        .color(p.ink),
                );
                ui.add(
                    egui::TextEdit::singleline(sizes)
                        .desired_width(f32::INFINITY)
                        .hint_text(aurora_i18n::tr!("wardrobe-register-sizes-hint")),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-register-note"))
                        .size(11.0)
                        .color(p.muted),
                );
                answer = buttons(
                    ui,
                    p,
                    aurora_i18n::tr!("common-save"),
                    !name.trim().is_empty(),
                    Some(aurora_i18n::tr!("wardrobe-register-undo")),
                );
            }
            Dialog::Note { item, text } => {
                widgets::section_header(ui, p, "pencil-filled", title(aurora_i18n::tr!("wardrobe-menu-note")));
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
                widgets::section_header(ui, p, "photo-filled", title(aurora_i18n::tr!("wardrobe-menu-picture")));
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
                // a picture of the PC: kept on this computer, never uploaded
                let working = st.wardrobe.picture_job.is_some();
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(6.0, 4.0);
                    for (from, icon, key) in [
                        (pictures::From::File, "folder-filled", "wardrobe-picture-file"),
                        (pictures::From::Clipboard, "copy-filled", "wardrobe-picture-paste"),
                    ] {
                        let label = aurora_i18n::text(key);
                        let size = Vec2::new(widgets::action_button_fit(ui, icon, label), 26.0);
                        if widgets::action_button_if(ui, p, icon, label, widgets::ActionKind::Flat, size, !working).clicked() {
                            st.wardrobe.picture_job = Some((framing::Target::Item(*item), pictures::choose(ui.ctx(), from)));
                        }
                    }
                    if widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-picture-own")).clicked() {
                        chosen = Some(None);
                    }
                });
                if working {
                    ui.label(
                        egui::RichText::new(aurora_i18n::tr!("wardrobe-picture-working"))
                            .size(11.5)
                            .color(p.muted),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(aurora_i18n::tr!("wardrobe-picture-local"))
                            .size(11.0)
                            .color(p.muted),
                    );
                }
                // Ctrl+V pastes a copied picture
                if !working && ui.input(|i| i.events.iter().any(|event| matches!(event, egui::Event::Paste(_)))) {
                    st.wardrobe.picture_job = Some((framing::Target::Item(*item), pictures::choose(ui.ctx(), pictures::From::Clipboard)));
                }
                if let Some(picture) = chosen {
                    let item = *item;
                    set_picture(item, picture, None);
                    answer = Answer::Close;
                } else {
                    answer = buttons(ui, p, aurora_i18n::tr!("common-close"), true, None);
                }
            }
            Dialog::SameProduct { item, search } => {
                widgets::section_header(ui, p, "copy-filled", title(aurora_i18n::tr!("wardrobe-menu-same-product")));
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-same-body", name = item_name(item)))
                        .size(12.0)
                        .color(p.muted),
                );
                ui.add_space(6.0);
                // a copy has its product's creator: only that creator's
                // other items are offered, with or without a card
                let creator = inv.items.get(item).map(|it| it.creator);
                let mut rows: Vec<(Uuid, String, String)> = st
                    .wardrobe
                    .model
                    .tiles
                    .iter()
                    .filter(|t| Some(t.creator) == creator && !t.copies.contains(item))
                    .map(|t| (t.item, t.name.clone(), t.pack.clone().unwrap_or_else(|| t.folder_name.clone())))
                    .collect();
                rows.sort_by_key(|row| crate::natural_order::key(&row.1));
                if rows.is_empty() {
                    ui.label(
                        egui::RichText::new(aurora_i18n::tr!("wardrobe-same-empty"))
                            .size(12.0)
                            .color(p.warn),
                    );
                }
                if let Some(other) = pick_list(ui, p, search, &rows) {
                    let joined = match (inv.items.get(item), inv.items.get(&other)) {
                        (Some(copy), Some(product)) => {
                            wardrobe::with(|shared| shared.wardrobe.change(true, |txn| matching::same_product(txn, copy, product)))
                        }
                        _ => false,
                    };
                    if joined {
                        // the two now show as one card: opened, the copy chosen
                        st.wardrobe.show(*item);
                        st.wardrobe
                            .say(aurora_i18n::tr!("wardrobe-same-done", name = item_name(&other)), true);
                    } else {
                        st.wardrobe.say(aurora_i18n::tr!("wardrobe-same-refused").to_owned(), false);
                    }
                    answer = Answer::Close;
                } else {
                    answer = buttons(ui, p, aurora_i18n::tr!("common-close"), true, None);
                }
            }
            Dialog::LinkHud { item, search } => {
                widgets::section_header(ui, p, "adjustments-filled", title(aurora_i18n::tr!("wardrobe-menu-link-hud")));
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
                // only the names the search holds are taken from the whole
                // inventory, every frame: never all of its objects
                let needle = search.trim().to_lowercase();
                let mut rows: Vec<(Uuid, String, String)> = inv
                    .items
                    .values()
                    .filter(|it| it.asset_type == 6 && it.id != item && !current.contains(&it.id))
                    .filter(|it| {
                        Some(it.parent) == folder
                            || (!needle.is_empty() && holds(&it.name, &needle) && inv.folders.get(&it.parent).is_some_and(|f| !f.library))
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
                    Dialog::Fits { item, fits } => {
                        txn.binding(item).fits = (!reset).then(|| names(&fits));
                    }
                    Dialog::Register { item, part, name, sizes } => {
                        let binding = txn.binding(item);
                        if reset {
                            binding.kit = None;
                            binding.part = None;
                        } else {
                            let mut kit = vec![name.trim().to_owned()];
                            kit.extend(names(&sizes).into_iter().filter(|size| !size.eq_ignore_ascii_case(name.trim())));
                            binding.kit = Some(kit);
                            binding.part = Some(part);
                            // the category that goes with the part, unless
                            // the user already chose one
                            let tag = match part {
                                Part::Body => Some(("body-parts", "mesh-body")),
                                Part::Head => Some(("heads", "mesh-head")),
                                Part::Ears => Some(("body-parts", "ears")),
                                Part::Hands => Some(("body-parts", "hands")),
                                Part::Feet => Some(("body-parts", "feet")),
                                Part::Tail => Some(("body-parts", "tail")),
                                Part::Wings => Some(("body-parts", "wings")),
                                Part::AddOn => Some(("body-parts", "add-on")),
                                Part::Other => None,
                            };
                            if let Some((category, subtype)) = tag
                                && binding.categories.is_none()
                            {
                                binding.categories = Some(vec![Tag::new(category, subtype)]);
                            }
                        }
                    }
                    Dialog::Note { item, text } => {
                        let text: String = text.trim().chars().take(card::MAX_NOTE).collect();
                        txn.binding(item).note = (!reset && !text.is_empty()).then_some(text);
                    }
                    _ => {}
                })
            });
        }
    }
}

/// The picture the user gives an item: a texture item of the inventory, a
/// file of the PC, or neither (the item's own image). A file that no item
/// shows any more, nor a change that can be undone, is deleted.
pub(super) fn set_picture(item: Uuid, texture: Option<Uuid>, file: Option<String>) {
    let old = wardrobe::with(|shared| {
        let old = shared.wardrobe.change(false, |txn| {
            let binding = txn.binding(item);
            let old = std::mem::replace(&mut binding.picture_file, file.clone());
            binding.picture = texture;
            binding.picture_by_user = texture.is_some() || file.is_some();
            old
        });
        old.filter(|name| !shared.wardrobe.picture_in_use(name))
    });
    if let Some(name) = old {
        pictures::discard(pictures::Kind::Item, &name);
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
