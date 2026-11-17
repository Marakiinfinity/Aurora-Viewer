//! The review sheet of a Wardrobe card (docs/WARDROBE.md, section 7): who
//! made the card, what it would change, in three groups the user ticks
//! (Store, Items, Pack and event), and what it could not place. A card by
//! the items' creator was applied already and is reviewed after the fact;
//! any other is applied from here, and never claims a store.

use super::*;
use crate::world::wardrobe::card::Warning;
use crate::world::wardrobe::matching::{self, Groups, Plan};
use crate::world::wardrobe::{Loaded, scan, store::CardState};

pub struct Review {
    card: Uuid,
    loaded: Loaded,
    plan: Plan,
    groups: Groups,
    /// The card was applied before the sheet opened (the creator's).
    applied: bool,
}

fn open(inv: &Inventory, card: Uuid) -> Option<Review> {
    let folder = inv.items.get(&card)?.parent;
    let folder_name = inv.folders.get(&folder).map(|f| f.info.name.clone()).unwrap_or_default();
    let scope = scan::card_scope(inv, folder);
    wardrobe::with(|shared| {
        let loaded = shared.cards.get(&card)?.clone();
        let plan = matching::plan(&loaded.card, card, loaded.creator, &folder_name, &scope, &shared.wardrobe.catalogue);
        let applied = matches!(shared.wardrobe.account.cards.get(&card), Some(CardState::Applied { .. }));
        Some(Review {
            card,
            groups: Groups {
                // only the creator says which store is theirs
                store: plan.trusted && loaded.card.store.is_some(),
                items: true,
                pack: true,
            },
            loaded,
            plan,
            applied,
        })
    })
}

pub(super) fn show(
    ctx: &egui::Context,
    p: &Palette,
    inv: &Inventory,
    st: &mut InventoryUi,
    facts: &Facts,
    images: &std::collections::HashMap<Uuid, egui::TextureHandle>,
) {
    if let Some(card) = st.wardrobe.pending_review.take() {
        match open(inv, card) {
            Some(review) => {
                st.wardrobe.wanted_names.insert(review.plan.card_creator);
                st.wardrobe.wanted_names.extend(review.plan.items_creator);
                st.wardrobe.review = Some(review);
            }
            None => st.wardrobe.say(aurora_i18n::tr!("wardrobe-review-unread").to_owned(), false),
        }
    }
    let Some(review) = &mut st.wardrobe.review else { return };
    let name = |id: &Uuid| facts.names.get(id).map_or("…", String::as_str).to_owned();
    let mut apply = false;
    let mut close = false;
    let mut wanted_logo = None;
    let modal = widgets::Modal::new(egui::Id::unique("wardrobe_review")).show(ctx, |ui| {
        ui.set_width(420.0);
        let card = &review.loaded.card;
        let title = card
            .store
            .as_ref()
            .map(|s| s.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| review.plan.pack_name.clone());
        ui.horizontal(|ui| {
            if let Some(logo) = card.store.as_ref().and_then(|s| s.logo).filter(|_| review.plan.trusted) {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
                match images.get(&logo) {
                    Some(texture) => widgets::square_picture(ui, rect, texture),
                    None => wanted_logo = Some(logo),
                }
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.label(egui::RichText::new(title).size(15.0).strong().color(p.ink));
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!(
                        "wardrobe-review-sub",
                        pack = review.plan.pack_name.as_str(),
                        count = review.plan.item_count()
                    ))
                    .size(11.5)
                    .color(p.muted),
                );
            });
        });
        ui.add_space(6.0);
        // the trust line
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            if review.plan.trusted {
                widgets::glyph(ui, "circle-check-filled", p.success, 14.0);
                ui.label(
                    egui::RichText::new(aurora_i18n::tr!("wardrobe-review-trusted", name = name(&review.plan.card_creator)))
                        .size(12.0)
                        .color(p.ink),
                );
            } else {
                widgets::glyph(ui, "info-circle-filled", p.warn, 14.0);
                let text = match review.plan.items_creator {
                    Some(creator) => aurora_i18n::tr!(
                        "wardrobe-review-untrusted",
                        name = name(&review.plan.card_creator),
                        creator = name(&creator)
                    ),
                    None => aurora_i18n::tr!("wardrobe-review-untrusted-several", name = name(&review.plan.card_creator)),
                };
                ui.label(egui::RichText::new(text).size(12.0).color(p.ink));
            }
        });
        ui.add_space(8.0);
        // the three groups
        let store_possible = review.plan.trusted && card.store.as_ref().is_some_and(|s| !s.name.is_empty());
        ui.add_enabled_ui(store_possible, |ui| {
            widgets::checkbox(ui, p, &mut review.groups.store, aurora_i18n::tr!("wardrobe-review-store"));
        });
        match (&card.store, review.plan.trusted) {
            (Some(store), true) => {
                let mut parts = vec![store.name.clone()];
                if store.marketplace.is_some() {
                    parts.push(aurora_i18n::tr!("wardrobe-store-marketplace").to_owned());
                }
                if store.mainstore.is_some() {
                    parts.push(aurora_i18n::tr!("wardrobe-review-mainstore").to_owned());
                }
                detail(ui, p, &parts.join(" · "));
            }
            (Some(_), false) => detail(ui, p, aurora_i18n::tr!("wardrobe-review-store-ignored")),
            (None, _) => detail(ui, p, aurora_i18n::tr!("wardrobe-review-store-none")),
        }
        ui.add_space(4.0);
        widgets::checkbox(
            ui,
            p,
            &mut review.groups.items,
            aurora_i18n::tr!("wardrobe-review-items", count = review.plan.matched.len()),
        );
        egui::ScrollArea::vertical()
            .id_salt("wardrobe_review_items")
            .max_height(180.0)
            .show(ui, |ui| {
                for (n, items) in &review.plan.matched {
                    let Some(line) = card.items.get(*n) else { continue };
                    let mut parts: Vec<String> = Vec::new();
                    let categories = line.categories.iter().map(tag_label).collect::<Vec<_>>().join(", ");
                    parts.push(if categories.is_empty() {
                        aurora_i18n::tr!("wardrobe-review-no-category").to_owned()
                    } else {
                        categories
                    });
                    match &line.fits {
                        crate::world::wardrobe::card::Fits::Kits(kits) => parts.push(kits.join(", ")),
                        crate::world::wardrobe::card::Fits::Everyone => parts.push(aurora_i18n::tr!("wardrobe-fit-everyone").to_owned()),
                        crate::world::wardrobe::card::Fits::Unsaid => {}
                    }
                    let huds = review
                        .plan
                        .huds
                        .iter()
                        .filter(|(_, found, of)| *of == Some(*n) && !found.is_empty())
                        .count();
                    if huds > 0 {
                        parts.push(aurora_i18n::tr!("wardrobe-plus-huds", count = huds));
                    }
                    if review.plan.pictures.contains_key(n) {
                        parts.push(aurora_i18n::tr!("wardrobe-review-picture").to_owned());
                    }
                    if items.len() > 1 {
                        parts.push(format!("×{}", items.len()));
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(22.0);
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.add(egui::Label::new(egui::RichText::new(&line.name).size(12.0).color(p.ink)).truncate());
                        ui.add(egui::Label::new(egui::RichText::new(parts.join(" · ")).size(11.0).color(p.muted)).truncate());
                    });
                }
            });
        ui.add_space(4.0);
        widgets::checkbox(ui, p, &mut review.groups.pack, aurora_i18n::tr!("wardrobe-review-pack"));
        let pack = card.pack.clone().unwrap_or_default();
        let mut parts = vec![review.plan.pack_name.clone()];
        parts.extend(
            pack.version
                .iter()
                .map(|v| aurora_i18n::tr!("wardrobe-tip-version", version = v.as_str())),
        );
        parts.extend(pack.event.iter().cloned());
        if pack.demo {
            parts.push(aurora_i18n::tr!("wardrobe-tip-demo").to_owned());
        }
        detail(ui, p, &parts.join(" · "));
        // what the card could not place
        let mut warnings: Vec<String> = Vec::new();
        if review.plan.stale {
            warnings.push(aurora_i18n::tr!("wardrobe-review-stale").to_owned());
        }
        if !review.plan.skipped.is_empty() {
            warnings.push(aurora_i18n::tr!("wardrobe-review-skipped", count = review.plan.skipped.len()));
        }
        for warning in &card.warnings {
            warnings.push(match warning {
                Warning::Malformed(line) => aurora_i18n::tr!("wardrobe-review-malformed", line = *line),
                Warning::UnknownCategory { written, .. } => {
                    aurora_i18n::tr!("wardrobe-review-unknown-category", written = written.as_str())
                }
                Warning::UnknownSubtype { written, .. } => aurora_i18n::tr!("wardrobe-review-unknown-subtype", written = written.as_str()),
                Warning::TooManyItems(count) => aurora_i18n::tr!("wardrobe-review-too-many", count = *count),
            });
        }
        if !warnings.is_empty() {
            ui.add_space(6.0);
            for warning in warnings.iter().take(6) {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 5.0;
                    widgets::glyph(ui, "alert-triangle-filled", p.warn, 12.0);
                    ui.label(egui::RichText::new(warning).size(11.5).color(p.muted));
                });
            }
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(aurora_i18n::tr!("wardrobe-review-footnote"))
                .size(11.0)
                .color(p.muted),
        );
        ui.add_space(8.0);
        let count = review.groups.count();
        let label = if review.applied {
            aurora_i18n::tr!("wardrobe-review-apply-again", count = count)
        } else {
            aurora_i18n::tr!("wardrobe-review-apply", count = count)
        };
        let buttons = [
            widgets::DialogButton {
                icon: "",
                label: aurora_i18n::tr!("common-cancel"),
                kind: widgets::ActionKind::Flat,
                enabled: true,
            },
            widgets::DialogButton {
                icon: "",
                label: &label,
                kind: widgets::ActionKind::Primary,
                enabled: count > 0 && !review.plan.stale && review.plan.item_count() > 0,
            },
        ];
        match widgets::dialog_buttons(ui, p, &buttons, 28.0) {
            Some(0) => close = true,
            Some(_) => apply = true,
            None => {}
        }
    });
    if let Some(logo) = wanted_logo {
        st.wanted_images.insert(logo);
    }
    if modal.should_close() {
        close = true;
    }
    if apply && let Some(review) = st.wardrobe.review.take() {
        let done = wardrobe::with(|shared| {
            shared.wardrobe.change(true, |txn| {
                matching::apply(txn, &review.loaded.card, &review.plan, review.groups, &review.loaded.embedded)
            })
        });
        let text = if done.added > 0 {
            aurora_i18n::tr!("wardrobe-card-applied", count = done.added)
        } else {
            aurora_i18n::tr!("wardrobe-card-updated", count = done.updated)
        };
        st.wardrobe.say(text, true);
        let _ = review.card;
    } else if close {
        st.wardrobe.review = None;
    }
}

fn detail(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(22.0);
        ui.add(egui::Label::new(egui::RichText::new(text).size(11.5).color(p.muted)).wrap());
    });
}
