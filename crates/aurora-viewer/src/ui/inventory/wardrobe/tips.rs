//! The item card tooltip of the Wardrobe (docs/WARDROBE.md, section 11):
//! the category and the edition, the name with Worn or New, the store, what
//! the item was created for, its HUDs, its rights and its event; with Shift
//! held, the creator, the date, the folder and the permissions. The same
//! lines fill the details pane of the wide window. No asset id is ever
//! written here: the rights say "no copy" or "no transfer".

use super::model::Tile;
use super::*;

fn line(ui: &mut egui::Ui, icon: &str, tint: egui::Color32, text: &str, ink: egui::Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        widgets::glyph(ui, icon, tint, 12.0);
        ui.add(egui::Label::new(egui::RichText::new(text).size(11.5).color(ink)).wrap());
    });
}

/// The lines of an item: the card's, and the details' when `details`.
pub(in super::super) fn lines(ui: &mut egui::Ui, p: &Palette, tile: &Tile, item: Uuid, inv: &Inventory, facts: &Facts, details: bool) {
    ui.spacing_mut().item_spacing.y = 3.0;
    // the kicker: the categories, and the edition
    let mut kicker: Vec<String> = tile.tags.iter().map(tag_label).collect();
    if let Some(version) = &tile.version {
        kicker.push(aurora_i18n::tr!("wardrobe-tip-version", version = version.as_str()));
    }
    if tile.demo {
        kicker.push(aurora_i18n::tr!("wardrobe-tip-demo").to_owned());
    }
    if !kicker.is_empty() {
        ui.label(egui::RichText::new(kicker.join(" · ").to_uppercase()).size(9.5).color(p.muted));
    }
    let name = inv.items.get(&item).map_or(tile.name.as_str(), |it| it.name.as_str());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(egui::RichText::new(name).size(13.5).strong().color(p.ink));
        if facts.worn.contains(&item) {
            ui.label(egui::RichText::new(aurora_i18n::tr!("wardrobe-chip-worn")).size(10.5).color(p.teal));
        } else if tile.new {
            ui.label(
                egui::RichText::new(aurora_i18n::tr!("wardrobe-new"))
                    .size(10.5)
                    .color(p.violet_light),
            );
        }
    });
    if let Some(note) = &tile.note {
        ui.label(egui::RichText::new(note).size(11.5).italics().color(p.muted));
    }
    match &tile.store {
        Some(store) => {
            let text = if tile.store_by_user {
                aurora_i18n::tr!("wardrobe-tip-store-yours", store = store.as_str())
            } else {
                store.clone()
            };
            line(ui, "building-store", p.muted, &text, p.ink);
        }
        None => {
            let creator = facts.names.get(&tile.creator).map_or("…", String::as_str);
            line(
                ui,
                "user-filled",
                p.muted,
                &aurora_i18n::tr!("wardrobe-tip-no-store", creator = creator),
                p.muted,
            );
        }
    }
    let (icon, tint) = if tile.fit.fits() {
        ("check", p.success)
    } else {
        ("info-circle-filled", p.warn)
    };
    line(ui, icon, tint, &fit_text(&tile.fit), p.ink);
    if matches!(tile.fit, crate::world::wardrobe::fit::Fit::Unknown(_)) {
        ui.label(
            egui::RichText::new(aurora_i18n::tr!("wardrobe-tip-unknown-kit"))
                .size(10.5)
                .color(p.muted),
        );
    }
    if !tile.kit.is_empty() {
        line(
            ui,
            "man-filled",
            p.muted,
            &aurora_i18n::tr!("wardrobe-tip-kit", kits = tile.kit.join(", ")),
            p.ink,
        );
    }
    if !tile.huds.is_empty() {
        let worn = tile.huds.iter().filter(|id| facts.worn.contains(id)).count();
        line(
            ui,
            "adjustments-filled",
            p.muted,
            &aurora_i18n::tr!("wardrobe-tip-huds", count = tile.huds.len(), worn = worn),
            p.ink,
        );
    }
    let rights: Vec<&str> = [
        (tile.rights[0], "wardrobe-right-no-modify"),
        (tile.rights[1], "wardrobe-right-no-copy"),
        (tile.rights[2], "wardrobe-right-no-transfer"),
    ]
    .into_iter()
    .filter(|(missing, _)| *missing)
    .map(|(_, key)| aurora_i18n::text(key))
    .collect();
    if !rights.is_empty() {
        line(ui, "shield-lock-filled", p.muted, &rights.join(" · "), p.muted);
    }
    if let Some(event) = &tile.event {
        line(ui, "calendar-event-filled", p.muted, event, p.ink);
    }
    if tile.copies.len() > 1 {
        line(
            ui,
            "stack-2-filled",
            p.muted,
            &aurora_i18n::tr!("wardrobe-tip-copies", count = tile.copies.len()),
            p.muted,
        );
    }
    if !details {
        return;
    }
    widgets::divider(ui, p);
    let creator = facts.names.get(&tile.creator).map_or("…", String::as_str);
    let pair = |ui: &mut egui::Ui, key: &str, value: &str| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(egui::RichText::new(key).size(11.0).color(p.muted));
            ui.label(egui::RichText::new(value).size(11.0).color(p.ink));
        });
    };
    pair(ui, aurora_i18n::tr!("wardrobe-tip-creator"), creator);
    if let Some(pack) = &tile.pack {
        pair(ui, aurora_i18n::tr!("wardrobe-tip-pack"), pack);
    }
    if let Some(it) = inv.items.get(&item) {
        let date = if it.created_at > 0 {
            aurora_net::inventory::created_date(it.created_at)
        } else {
            aurora_i18n::tr!("ui-inventory-unknown-date").into()
        };
        pair(ui, aurora_i18n::tr!("wardrobe-tip-acquired"), &date);
        let folder = inv.folders.get(&it.parent).map_or("", |f| f.info.name.as_str());
        pair(ui, aurora_i18n::tr!("wardrobe-tip-folder"), folder);
    }
    if !tile.trusted && tile.pack.is_some() {
        ui.label(
            egui::RichText::new(aurora_i18n::tr!("wardrobe-tip-untrusted"))
                .size(10.5)
                .color(p.muted),
        );
    }
}

/// The tooltip of a card: its lines, the details while Shift is held.
pub(super) fn card(ui: &mut egui::Ui, p: &Palette, tile: &Tile, item: Uuid, inv: &Inventory, facts: &Facts) {
    ui.set_max_width(260.0);
    let details = ui.input(|i| i.modifiers.shift);
    lines(ui, p, tile, item, inv, facts, details);
    if !details {
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new(aurora_i18n::tr!("wardrobe-tip-shift"))
                .size(10.0)
                .color(p.muted_dim),
        );
    }
}
