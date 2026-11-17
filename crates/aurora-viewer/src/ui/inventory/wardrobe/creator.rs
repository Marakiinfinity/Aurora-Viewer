//! The creator tool (docs/WARDROBE.md, section 12): a window that writes
//! the Wardrobe card of a product box. The creator chooses the folder,
//! fills the store and the pack once, then each item's category, kits,
//! HUD and note; the card is saved as a notecard in the folder, as a file,
//! or copied. The card names the items; it embeds none: an embedded item is
//! a copy handed to whoever holds the notecard. The logo field offers the
//! creator's own full-permission textures only, the one id a card may
//! carry (`world::inventory::actions::wardrobe_logo_allowed`).

use super::*;
use crate::world::inventory::actions;
use crate::world::wardrobe::card::{self, Card, Fits, HudLine, ItemLine, PackBlock, StoreBlock};
use crate::world::wardrobe::categories::{OTHER, Tag, wardrobe_asset};
use crate::world::wardrobe::matching::{fold, hud_stem, is_link, names_hud};

struct Row {
    name: String,
    include: bool,
    /// Index of the row this one is a HUD of.
    hud_of: Option<usize>,
    tags: Vec<Tag>,
    fits: String,
    kit: String,
    note: String,
}

#[derive(Default)]
pub struct State {
    pub open: bool,
    folder: Uuid,
    search: String,
    store: StoreBlock,
    store_marketplace: String,
    store_mainstore: String,
    pack: PackBlock,
    pack_name: String,
    pack_version: String,
    pack_event: String,
    rows: Vec<Row>,
    message: String,
}

fn names(list: &str) -> Vec<String> {
    list.split(',')
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .take(card::MAX_NAMES)
        .map(str::to_owned)
        .collect()
}

impl State {
    /// Opens the tool on a folder: its wearables and objects become rows,
    /// with what their type and their name already tell.
    pub fn choose(&mut self, inv: &Inventory, folder: Uuid, agent: Uuid) {
        self.open = true;
        self.folder = folder;
        self.message.clear();
        self.pack_name = inv.folders.get(&folder).map(|f| f.info.name.clone()).unwrap_or_default();
        // the creator's store, as their last card or their link said it
        if let Some(store) = wardrobe::with(|shared| shared.wardrobe.catalogue.stores.get(&agent).cloned()) {
            self.store.name = store.name;
            self.store.logo = store.logo.filter(|logo| {
                inv.items
                    .values()
                    .any(|it| it.asset_id == *logo && actions::wardrobe_logo_allowed(inv, it))
            });
            self.store_marketplace = store.marketplace.unwrap_or_default();
            self.store_mainstore = store.mainstore.unwrap_or_default();
        }
        let mut items: Vec<&aurora_net::inventory::InvItem> = inv
            .folders
            .get(&folder)
            .into_iter()
            .flat_map(|f| f.items.iter())
            .filter_map(|id| inv.items.get(id))
            .filter(|it| wardrobe_asset(it.asset_type) && !is_link(it))
            .collect();
        items.sort_by_key(|it| crate::natural_order::key(&it.name));
        // one line per name: the copies of an item share it
        let mut seen: HashSet<String> = HashSet::new();
        items.retain(|it| seen.insert(fold(&it.name)));
        self.rows = items
            .iter()
            .map(|it| Row {
                name: it.name.clone(),
                include: true,
                hud_of: None,
                tags: crate::world::wardrobe::sort::place(it, false, false).into_iter().collect(),
                fits: String::new(),
                kit: String::new(),
                note: String::new(),
            })
            .collect();
        // a HUD named after an item is that item's
        for n in 0..self.rows.len() {
            if !names_hud(&self.rows[n].name) {
                continue;
            }
            let stem = hud_stem(&self.rows[n].name);
            let items: Vec<usize> = (0..self.rows.len())
                .filter(|m| *m != n && !names_hud(&self.rows[*m].name))
                .collect();
            self.rows[n].hud_of = items
                .iter()
                .copied()
                .find(|m| stem.as_deref() == Some(fold(&self.rows[*m].name).as_str()))
                .or(match items.as_slice() {
                    [only] => Some(*only),
                    _ => None,
                });
        }
    }

    /// The card as the fields say it.
    fn card(&self) -> Card {
        let filled = |text: &str| Some(text.trim().to_owned()).filter(|t| !t.is_empty());
        let store = (!self.store.name.trim().is_empty()).then(|| StoreBlock {
            name: self.store.name.trim().to_owned(),
            logo: self.store.logo,
            marketplace: filled(&self.store_marketplace).filter(|url| card::valid_marketplace(url)),
            mainstore: filled(&self.store_mainstore).filter(|url| card::valid_mainstore(url)),
            group: None,
        });
        let pack = PackBlock {
            name: filled(&self.pack_name),
            version: filled(&self.pack_version),
            event: filled(&self.pack_event),
            demo: self.pack.demo,
        };
        let mut items = Vec::new();
        let mut huds = Vec::new();
        for row in self.rows.iter().filter(|r| r.include) {
            match row.hud_of.and_then(|n| self.rows.get(n)).filter(|of| of.include) {
                Some(of) => huds.push(HudLine {
                    line: 0,
                    name: row.name.clone(),
                    of: of.name.clone(),
                }),
                None => items.push(ItemLine {
                    line: 0,
                    name: row.name.clone(),
                    categories: crate::world::wardrobe::categories::normalise(row.tags.clone()),
                    fits: match names(&row.fits) {
                        list if list.is_empty() => Fits::Unsaid,
                        list if list.len() == 1 && list[0].eq_ignore_ascii_case("everyone") => Fits::Everyone,
                        list => Fits::Kits(list),
                    },
                    kit: names(&row.kit),
                    note: filled(&row.note).map(|n| n.chars().take(card::MAX_NOTE).collect()),
                    ..ItemLine::default()
                }),
            }
        }
        Card {
            format: card::FORMAT,
            store,
            pack: Some(pack),
            items,
            huds,
            warnings: Vec::new(),
        }
    }
}

/// The category of a row: a menu of the categories and their subtypes.
fn category_menu(ui: &mut egui::Ui, p: &Palette, tags: &mut Vec<Tag>) {
    let label = match tags.first() {
        Some(tag) => {
            let extra = if tags.len() > 1 {
                format!(" +{}", tags.len() - 1)
            } else {
                String::new()
            };
            format!("{} / {}{extra}  ⏷", categories::category_label(tag.category()), tag_label(tag))
        }
        None => format!("{}  ⏷", aurora_i18n::tr!("wardrobe-creator-category")),
    };
    let button = egui::Button::new(egui::RichText::new(label).size(11.5));
    menu::button(ui, p, button, |ui| {
        for category in categories::WARDROBE {
            menu::submenu(ui, p, category_icon(category.key), aurora_i18n::text(category.label), true, |ui| {
                for subtype in category.subtypes {
                    let on = tags.iter().any(|t| t.category() == category.key && t.has_subtype(subtype.key));
                    if menu::check(ui, p, "", aurora_i18n::text(subtype.label), on, true) {
                        // the main category: a click sets it, a second
                        // subtype of the same category joins it
                        match tags.iter_mut().find(|t| t.category() == category.key) {
                            Some(tag) if on => tag.1.retain(|s| s != subtype.key),
                            Some(tag) if subtype.key != OTHER && tag.1.len() < 2 && !tag.1.iter().any(|s| s == OTHER) => {
                                tag.1.push(subtype.key.to_owned())
                            }
                            Some(tag) => tag.1 = vec![subtype.key.to_owned()],
                            None => *tags = vec![Tag::new(category.key, subtype.key)],
                        }
                        tags.retain(|t| !t.1.is_empty());
                    }
                }
            });
        }
    });
}

pub(in super::super) fn show(
    ctx: &egui::Context,
    p: &Palette,
    inv: &Inventory,
    st: &mut InventoryUi,
    facts: &Facts,
    actions: &mut Vec<InvAction>,
) {
    if !st.wardrobe.creator.open {
        return;
    }
    let mut open = true;
    let screen = ctx.content_rect();
    let mut choose: Option<Uuid> = None;
    let mut save: Option<(String, String)> = None;
    let state = &mut st.wardrobe.creator;
    widgets::Floater::new(
        "wardrobe_creator",
        aurora_i18n::tr!("wardrobe-creator-title"),
        egui::pos2(screen.center().x - 330.0, 80.0),
        Vec2::new(660.0, 560.0),
    )
    .show(ctx, p, &mut open, |ui| {
        if state.folder.is_nil() || !inv.folders.contains_key(&state.folder) {
            // the folder of the product box
            ui.label(
                egui::RichText::new(aurora_i18n::tr!("wardrobe-creator-pick"))
                    .size(12.5)
                    .color(p.ink),
            );
            ui.add_space(6.0);
            widgets::clearable_search_box(
                ui,
                p,
                &mut state.search,
                aurora_i18n::tr!("wardrobe-creator-folder-hint"),
                ui.available_width(),
            );
            ui.add_space(6.0);
            let needle = state.search.to_lowercase();
            egui::ScrollArea::vertical()
                .id_salt("wardrobe_creator_folders")
                .auto_shrink(false)
                .show(ui, |ui| {
                    let mut folders: Vec<_> = inv
                        .folders
                        .values()
                        .filter(|f| !f.library && f.info.type_default < 0)
                        .filter(|f| {
                            f.items
                                .iter()
                                .filter_map(|id| inv.items.get(id))
                                .any(|it| wardrobe_asset(it.asset_type) && it.creator == facts.agent)
                        })
                        .filter(|f| needle.is_empty() || f.info.name.to_lowercase().contains(&needle))
                        .collect();
                    folders.sort_by_key(|f| crate::natural_order::key(&f.info.name));
                    if folders.is_empty() {
                        ui.label(
                            egui::RichText::new(aurora_i18n::tr!("wardrobe-creator-no-folder"))
                                .size(12.0)
                                .color(p.muted),
                        );
                    }
                    for folder in folders.into_iter().take(300) {
                        if widgets::rail_row(
                            ui,
                            p,
                            "folder-filled",
                            p.muted,
                            &folder.info.name,
                            Some(folder.items.len()),
                            false,
                            0,
                        )
                        .clicked()
                        {
                            choose = Some(folder.info.id);
                        }
                    }
                });
            return;
        }
        let text = state.card().write();
        ui.horizontal(|ui| {
            widgets::glyph(ui, "folder-filled", p.muted, 14.0);
            let name = inv.folders.get(&state.folder).map_or("", |f| f.info.name.as_str());
            ui.label(egui::RichText::new(name).size(13.0).strong().color(p.ink));
            if widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-creator-change")).clicked() {
                state.folder = Uuid::nil();
            }
        });
        ui.add_space(6.0);
        let room = ui.available_height() - 40.0;
        egui::ScrollArea::vertical()
            .id_salt("wardrobe_creator_form")
            .max_height(room)
            .auto_shrink(false)
            .show(ui, |ui| {
                let field = |ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str, width: f32| {
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.label(egui::RichText::new(label).size(11.0).color(p.muted));
                        ui.add(
                            egui::TextEdit::singleline(text)
                                .desired_width(width)
                                .hint_text(hint)
                                .char_limit(card::MAX_VALUE),
                        );
                    });
                };
                widgets::section_header(ui, p, "building-store", aurora_i18n::tr!("wardrobe-review-store"));
                ui.horizontal_wrapped(|ui| {
                    field(ui, aurora_i18n::tr!("wardrobe-store-name"), &mut state.store.name, "", 180.0);
                    field(
                        ui,
                        aurora_i18n::tr!("wardrobe-store-marketplace"),
                        &mut state.store_marketplace,
                        "https://marketplace.secondlife.com/stores/…",
                        220.0,
                    );
                    field(
                        ui,
                        aurora_i18n::tr!("wardrobe-review-mainstore"),
                        &mut state.store_mainstore,
                        "secondlife://Region/128/128/25",
                        190.0,
                    );
                });
                ui.horizontal(|ui| {
                    // the logo: one of the creator's own textures, held with
                    // full permissions
                    let label = match state.store.logo.and_then(|logo| inv.items.values().find(|it| it.asset_id == logo)) {
                        Some(texture) => aurora_i18n::tr!("wardrobe-creator-logo-set", name = texture.name.as_str()),
                        None => aurora_i18n::tr!("wardrobe-creator-logo").to_owned(),
                    };
                    let button = egui::Button::new(egui::RichText::new(format!("{label}  ⏷")).size(11.5));
                    menu::button(ui, p, button, |ui| {
                        if menu::item(ui, p, "x", aurora_i18n::tr!("wardrobe-creator-logo-none")) {
                            state.store.logo = None;
                        }
                        let mut textures: Vec<_> = inv.items.values().filter(|it| actions::wardrobe_logo_allowed(inv, it)).collect();
                        textures.sort_by_key(|it| crate::natural_order::key(&it.name));
                        for texture in textures.into_iter().take(60) {
                            if menu::item(ui, p, "photo-filled", &texture.name) {
                                state.store.logo = Some(texture.asset_id);
                            }
                        }
                    });
                });
                ui.add_space(6.0);
                widgets::section_header(ui, p, "box", aurora_i18n::tr!("wardrobe-review-pack"));
                ui.horizontal_wrapped(|ui| {
                    field(ui, aurora_i18n::tr!("wardrobe-creator-pack-name"), &mut state.pack_name, "", 220.0);
                    field(
                        ui,
                        aurora_i18n::tr!("wardrobe-creator-version"),
                        &mut state.pack_version,
                        "1.0",
                        70.0,
                    );
                    field(ui, aurora_i18n::tr!("wardrobe-browse-event"), &mut state.pack_event, "", 180.0);
                    ui.vertical(|ui| {
                        ui.add_space(14.0);
                        widgets::checkbox(ui, p, &mut state.pack.demo, aurora_i18n::tr!("wardrobe-tip-demo"));
                    });
                });
                ui.add_space(6.0);
                widgets::section_header(ui, p, "shirt-filled", aurora_i18n::tr!("wardrobe-creator-items"));
                if state.rows.is_empty() {
                    ui.label(
                        egui::RichText::new(aurora_i18n::tr!("wardrobe-creator-no-items"))
                            .size(12.0)
                            .color(p.muted),
                    );
                }
                let item_names: Vec<(usize, String)> = state
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.hud_of.is_none())
                    .map(|(n, r)| (n, r.name.clone()))
                    .collect();
                for (n, row) in state.rows.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        widgets::checkbox(ui, p, &mut row.include, "");
                        ui.add_sized(
                            Vec2::new(170.0, 18.0),
                            egui::Label::new(
                                egui::RichText::new(&row.name)
                                    .size(12.0)
                                    .color(if row.include { p.ink } else { p.muted }),
                            )
                            .truncate(),
                        );
                        if !row.include {
                            return;
                        }
                        // an item, or the HUD of one
                        let hud_label = match row.hud_of.and_then(|of| item_names.iter().find(|(m, _)| *m == of)) {
                            Some((_, name)) => aurora_i18n::tr!("wardrobe-creator-hud-of", name = name.as_str()),
                            None => aurora_i18n::tr!("wardrobe-creator-is-item").to_owned(),
                        };
                        let button = egui::Button::new(egui::RichText::new(format!("{hud_label}  ⏷")).size(11.5));
                        menu::button(ui, p, button, |ui| {
                            if menu::check(ui, p, "", aurora_i18n::tr!("wardrobe-creator-is-item"), row.hud_of.is_none(), true) {
                                row.hud_of = None;
                            }
                            menu::separator(ui, p);
                            for (m, name) in item_names.iter().filter(|(m, _)| *m != n) {
                                if menu::check(
                                    ui,
                                    p,
                                    "adjustments-filled",
                                    &aurora_i18n::tr!("wardrobe-creator-hud-of", name = name.as_str()),
                                    row.hud_of == Some(*m),
                                    true,
                                ) {
                                    row.hud_of = Some(*m);
                                }
                            }
                        });
                        if row.hud_of.is_some() {
                            return;
                        }
                        category_menu(ui, p, &mut row.tags);
                    });
                    if row.include && row.hud_of.is_none() {
                        ui.horizontal(|ui| {
                            ui.add_space(26.0);
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.add(
                                egui::TextEdit::singleline(&mut row.fits)
                                    .desired_width(190.0)
                                    .hint_text(aurora_i18n::tr!("wardrobe-fits-hint")),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut row.kit)
                                    .desired_width(150.0)
                                    .hint_text(aurora_i18n::tr!("wardrobe-kit-hint")),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut row.note)
                                    .desired_width(150.0)
                                    .hint_text(aurora_i18n::tr!("wardrobe-creator-note-hint"))
                                    .char_limit(card::MAX_NOTE),
                            );
                        });
                    }
                    ui.add_space(2.0);
                }
                ui.add_space(6.0);
                widgets::section_header(ui, p, "file-text-filled", aurora_i18n::tr!("wardrobe-creator-preview"));
                let mut preview = text.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut preview)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(8)
                        .interactive(false),
                );
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(egui::RichText::new(&state.message).size(11.5).color(p.muted)).truncate());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let size = Vec2::new(150.0, 28.0);
                let any = state.rows.iter().any(|r| r.include);
                let too_long = text.len() > card::MAX_BYTES;
                if ui
                    .add_enabled_ui(any && !too_long, |ui| {
                        widgets::primary_button_sized(ui, p, aurora_i18n::tr!("wardrobe-creator-save"), size)
                    })
                    .inner
                    .clicked()
                {
                    let name = format!("Aurora Wardrobe - {}", state.pack_name.trim());
                    save = Some((name, text.clone()));
                }
                if widgets::flat_button(ui, p, aurora_i18n::tr!("wardrobe-creator-file")).clicked() {
                    let text = text.clone();
                    // the dialog blocks: it runs off the frame thread
                    let spawned = std::thread::Builder::new().name("aurora-wardrobe-card".into()).spawn(move || {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_file_name("Aurora Wardrobe.txt")
                            .add_filter("Text", &["txt"])
                            .save_file()
                            && let Err(error) = crate::atomic_file::write(&path, text.as_bytes())
                        {
                            log::warn!("Cannot save the Wardrobe card: {error}");
                        }
                    });
                    if let Err(error) = spawned {
                        log::warn!("Cannot open the save dialog: {error}");
                    }
                }
                if widgets::flat_button(ui, p, aurora_i18n::tr!("common-copy")).clicked() {
                    ui.ctx().copy_text(text.clone());
                    state.message = aurora_i18n::tr!("wardrobe-creator-copied").to_owned();
                }
                if too_long {
                    ui.label(
                        egui::RichText::new(aurora_i18n::tr!("wardrobe-creator-too-long"))
                            .size(11.5)
                            .color(p.warn),
                    );
                }
            });
        });
    });
    if let Some(folder) = choose {
        state.choose(inv, folder, facts.agent);
    }
    if let Some((name, text)) = save {
        match aurora_net::inventory::notecard::Notecard::default().encode(&text) {
            Ok(data) => {
                actions.push(InvAction::WardrobeCard {
                    parent: state.folder,
                    name,
                    data,
                });
                state.message = aurora_i18n::tr!("wardrobe-creator-saved").to_owned();
            }
            Err(reason) => state.message = reason,
        }
    }
    if !open {
        state.open = false;
    }
}
