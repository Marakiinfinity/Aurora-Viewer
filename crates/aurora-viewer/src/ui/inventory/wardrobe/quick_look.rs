//! Quick Look (docs/WARDROBE.md, section 10): a card over the dimmed
//! window with a large picture, the store, what the item was created for,
//! its HUDs and its event, Try on and Wear; ← and → move to the previous
//! and the next card, Enter wears, Space or Esc closes.

use super::*;

pub(super) fn show(
    ctx: &egui::Context,
    p: &Palette,
    inv: &Inventory,
    st: &mut InventoryUi,
    facts: &Facts,
    images: &std::collections::HashMap<Uuid, egui::TextureHandle>,
    actions: &mut Vec<InvAction>,
) {
    let Some(item) = st.wardrobe.quick_look else { return };
    let Some(tile) = st.wardrobe.model.tile_of(&item).cloned() else {
        st.wardrobe.quick_look = None;
        return;
    };
    // the cards in the order the grid shows them
    let order: Vec<Uuid> = st.wardrobe.visible.iter().map(|(n, _)| st.wardrobe.model.tiles[*n].item).collect();
    let at = order.iter().position(|id| tile.copies.contains(id));
    let step = |by: isize| -> Option<Uuid> {
        let at = at? as isize + by;
        (at >= 0).then(|| order.get(at as usize).copied()).flatten()
    };
    let (mut close, mut go, mut wear_now, mut try_now) = (false, None, false, false);
    ctx.input(|i| {
        if i.key_pressed(egui::Key::ArrowLeft) {
            go = step(-1);
        } else if i.key_pressed(egui::Key::ArrowRight) {
            go = step(1);
        }
        wear_now = i.key_pressed(egui::Key::Enter);
        close = i.key_pressed(egui::Key::Space) && !st.wardrobe.quick_opened_now;
    });
    st.wardrobe.quick_opened_now = false;
    let worn = facts.worn.contains(&item);
    let modal = widgets::Modal::new(egui::Id::unique("wardrobe_quick_look")).show(ctx, |ui| {
        ui.set_width(300.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(256.0).min(Vec2::splat(ui.available_width())), Sense::hover());
        let rect = Rect::from_center_size(egui::pos2(ui.max_rect().center().x, rect.center().y), rect.size());
        grid::paint_picture(
            ui,
            p,
            rect,
            &tile,
            images,
            &mut st.wanted_images,
            1.0,
            worn.then_some(&st.wardrobe.live),
        );
        ui.add_space(8.0);
        tips::lines(ui, p, &tile, item, inv, facts, false);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if widgets::icon_button(
                ui,
                p,
                "chevron-left",
                aurora_i18n::tr!("wardrobe-quick-previous"),
                step(-1).is_some(),
            ) {
                go = step(-1);
            }
            if widgets::icon_button(ui, p, "chevron-right", aurora_i18n::tr!("wardrobe-quick-next"), step(1).is_some()) {
                go = step(1);
            }
            if let Some(url) = &tile.marketplace
                && widgets::icon_button(ui, p, "external-link-filled", aurora_i18n::tr!("wardrobe-store-marketplace"), true)
            {
                crate::ui::context::request(ui.ctx(), crate::ui::web_view::clicked_link_action(url));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let size = Vec2::new(92.0, 28.0);
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
                        wear_now = true;
                    }
                } else {
                    let enabled = can_wear(inv, facts, item);
                    if ui
                        .add_enabled_ui(enabled, |ui| {
                            widgets::primary_button_sized(ui, p, aurora_i18n::tr!("common-wear"), size)
                        })
                        .inner
                        .clicked()
                    {
                        wear_now = true;
                    }
                    if ui
                        .add_enabled(
                            enabled && st.wardrobe.trying.is_none(),
                            egui::Button::new(egui::RichText::new(aurora_i18n::tr!("wardrobe-try-on")).size(12.0)),
                        )
                        .clicked()
                    {
                        try_now = true;
                    }
                }
            });
        });
    });
    if modal.should_close() {
        close = true;
    }
    if try_now {
        try_on(st, inv, facts, item, actions);
        close = true;
    } else if wear_now {
        if worn {
            take_off(inv, facts, item, actions);
        } else {
            wear(st, inv, facts, item, true, actions);
        }
        close = true;
    }
    if let Some(next) = go {
        st.wardrobe.quick_look = Some(next);
        st.wardrobe.selected = Some(next);
    } else if close {
        st.wardrobe.quick_look = None;
    }
}
