//! The grid of the Wardrobe mode: one card per tile, with its picture, the
//! worn check, the NEW tag, the stack's count, "+HUD" and the store's logo
//! (docs/WARDROBE.md, section 10). Only the rows in sight are drawn.

use super::model::{Browse, Hit, Tile};
use super::*;

const GAP: f32 = 8.0;
const NAME_H: f32 = 16.0;
const SUB_H: f32 = 14.0;
const HEADING_H: f32 = 26.0;

/// What one place of the grid shows: a tile, or one copy of an open stack.
#[derive(Clone, Copy)]
struct Cell {
    tile: usize,
    item: Uuid,
    hit: Hit,
}

enum Row {
    Heading(String, usize),
    Cells(std::ops::Range<usize>),
}

/// The picture of a tile in `rect`: its texture, asked for when it is not
/// loaded yet, else the icon of its main category on a neutral face.
/// `alpha` dims an item that was not created for what is worn.
pub(in super::super) fn paint_picture(
    ui: &egui::Ui,
    p: &Palette,
    rect: Rect,
    tile: &Tile,
    images: &std::collections::HashMap<Uuid, egui::TextureHandle>,
    wanted: &mut HashSet<Uuid>,
    alpha: f32,
) {
    let radius = ui.visuals().window_corner_radius;
    ui.painter().rect_filled(rect, radius, p.raised);
    match tile.picture.and_then(|asset| {
        images.get(&asset).or_else(|| {
            wanted.insert(asset);
            None
        })
    }) {
        Some(texture) => widgets::square_picture(ui, rect, texture),
        None => {
            let icon = category_icon(tile.main().map_or("", |tag| tag.category()));
            widgets::paint_glyph(
                ui.painter(),
                icon,
                rect.center(),
                (rect.width() * 0.42).clamp(14.0, 56.0),
                p.muted_dim,
            );
        }
    }
    if alpha < 1.0 {
        ui.painter().rect_filled(rect, radius, p.panel.gamma_multiply(1.0 - alpha));
    }
}

fn elided(ui: &egui::Ui, text: &str, size: f32, color: egui::Color32, width: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), egui::FontId::proportional(size), color);
    job.wrap.max_width = width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.painter().layout_job(job)
}

/// A small pill over a picture: NEW, "×3", "+HUD".
fn badge(ui: &egui::Ui, text: &str, anchor: egui::Pos2, align: egui::Align2, ink: egui::Color32, fill: egui::Color32) -> Rect {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), egui::FontId::proportional(9.5), ink);
    let size = galley.size() + Vec2::new(8.0, 3.0);
    let rect = align.anchor_size(anchor, size);
    widgets::paint_tag_pill(ui, galley, rect, ink, fill);
    rect
}

/// The grid. Returns nothing: clicks select, wear, open a stack or a menu.
#[allow(clippy::too_many_arguments)]
pub(super) fn show(
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
    if st.wardrobe.visible.is_empty() {
        empty(ui, p, st);
        return;
    }
    // Ctrl + wheel sizes the tiles
    let zoom = ui.input(|i| i.zoom_delta());
    if zoom != 1.0 && ui.rect_contains_pointer(ui.max_rect()) {
        let side = (f32::from(prefs.wardrobe.tile) * zoom)
            .round()
            .clamp(f32::from(TILE_MIN), f32::from(TILE_MAX)) as u16;
        if side != prefs.wardrobe.tile {
            prefs.wardrobe.tile = side;
            st.preferences_dirty = true;
        }
    }
    let wanted = f32::from(prefs.wardrobe.tile).min(if width == Width::Thin { 104.0 } else { f32::from(TILE_MAX) });
    let room = ui.available_width() - 14.0;
    let columns = (((room + GAP) / (wanted + GAP)).floor() as usize).max(1);
    // the tiles glide to their size: they share the row's width
    let side = ui.ctx().animate_value_with_time(
        ui.scope_id().with("wardrobe-tile"),
        ((room - GAP * (columns as f32 - 1.0)) / columns as f32).min(wanted * 1.35),
        0.2 * motion(ui),
    );
    let searching = !model::query(&st.wardrobe.filter.text).words.is_empty();
    let with_sub = matches!(st.wardrobe.filter.rail, model::Rail::All | model::Rail::Wearing) || searching;
    let cell_h = side + 4.0 + NAME_H + if with_sub { SUB_H } else { 0.0 } + 6.0;

    // the cells, an open stack spread into its copies
    let mut cells: Vec<Cell> = Vec::with_capacity(st.wardrobe.visible.len());
    for (n, hit) in &st.wardrobe.visible {
        let tile = &st.wardrobe.model.tiles[*n];
        if tile.copies.len() > 1 && st.wardrobe.open_stack == Some(tile.item) {
            cells.extend(tile.copies.iter().map(|copy| Cell {
                tile: *n,
                item: *copy,
                hit: *hit,
            }));
        } else {
            cells.push(Cell {
                tile: *n,
                item: tile.item,
                hit: *hit,
            });
        }
    }
    // the rows, under a heading per group when the list is grouped
    let group_of = |cell: &Cell| -> Option<String> {
        let tile = &st.wardrobe.model.tiles[cell.tile];
        if searching {
            Some(
                match cell.hit {
                    Hit::Event => aurora_i18n::tr!("wardrobe-group-events"),
                    Hit::Store => aurora_i18n::tr!("wardrobe-group-stores"),
                    Hit::Folder => aurora_i18n::tr!("wardrobe-group-folders"),
                    Hit::Item => aurora_i18n::tr!("wardrobe-group-items"),
                }
                .to_owned(),
            )
        } else if st.wardrobe.filter.browse == Browse::Set {
            Some(match (&tile.pack, &tile.event) {
                (Some(pack), Some(event)) => format!("{pack} · {event}"),
                (Some(pack), None) => pack.clone(),
                _ => aurora_i18n::tr!("wardrobe-group-no-set").to_owned(),
            })
        } else {
            None
        }
    };
    let mut rows: Vec<Row> = Vec::new();
    let mut start = 0;
    while start < cells.len() {
        let group = group_of(&cells[start]);
        let mut end = start;
        while end < cells.len() && group_of(&cells[end]) == group {
            end += 1;
        }
        if let Some(name) = group {
            rows.push(Row::Heading(name, end - start));
        }
        let mut at = start;
        while at < end {
            let next = (at + columns).min(end);
            rows.push(Row::Cells(at..next));
            at = next;
        }
        start = end;
    }
    let heights: Vec<f32> = rows
        .iter()
        .map(|row| match row {
            Row::Heading(..) => HEADING_H,
            Row::Cells(_) => cell_h,
        })
        .collect();
    let total: f32 = heights.iter().sum();

    // the background takes the keyboard on a click between the cards; it is
    // registered before them, so that their own clicks win
    let background = ui.interact(ui.max_rect(), focus_id(st), Sense::click());
    if background.clicked() {
        request_focus(ui, st);
    }
    keyboard(ui, st, inv, facts, &cells, columns, actions);
    let reveal = st.wardrobe.selected.filter(|_| st.wardrobe.moved_by_key);
    let mut clicked: Option<(Cell, bool, bool)> = None;
    let mut toggle_stack: Option<Uuid> = None;
    let mut menu_for: Option<(Uuid, egui::Pos2)> = None;
    let output = egui::ScrollArea::vertical()
        .id_salt("wardrobe_grid")
        .auto_shrink(false)
        .show_viewport(ui, |ui, viewport| {
            let (area, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), total), Sense::hover());
            let mut y = area.top();
            for (row, height) in rows.iter().zip(&heights) {
                let top = y;
                y += height;
                let in_sight = top - area.top() < viewport.bottom() && y - area.top() > viewport.top();
                match row {
                    Row::Heading(name, count) if in_sight => {
                        let pos = egui::pos2(area.left() + 2.0, top + HEADING_H - 8.0);
                        let galley = ui.painter().layout_no_wrap(name.clone(), egui::FontId::proportional(12.5), p.ink);
                        let end = pos.x + galley.size().x;
                        ui.painter().galley(pos - Vec2::new(0.0, galley.size().y), galley, p.ink);
                        ui.painter().text(
                            egui::pos2(end + 8.0, pos.y),
                            egui::Align2::LEFT_BOTTOM,
                            count.to_string(),
                            egui::FontId::proportional(11.0),
                            p.muted,
                        );
                    }
                    Row::Cells(range) => {
                        for (column, index) in range.clone().enumerate() {
                            let cell = cells[index];
                            let rect = Rect::from_min_size(
                                egui::pos2(area.left() + column as f32 * (side + GAP), top),
                                Vec2::new(side, cell_h - 6.0),
                            );
                            if reveal == Some(cell.item) {
                                ui.scroll_to_rect(rect, None);
                            }
                            if !in_sight {
                                continue;
                            }
                            let tile = &st.wardrobe.model.tiles[cell.tile];
                            let response = ui.interact(rect, ui.scope_id().with(("wardrobe-cell", cell.item)), Sense::click());
                            let view = CellView {
                                selected: st.wardrobe.selected,
                                open_stack: st.wardrobe.open_stack,
                                trying: st.wardrobe.trying.as_ref().map(|t| t.item),
                                names: &st.wardrobe.copy_names,
                            };
                            let stack = paint_cell(
                                ui,
                                p,
                                rect,
                                side,
                                tile,
                                cell,
                                &view,
                                &mut st.wanted_images,
                                facts,
                                images,
                                with_sub,
                                &response,
                            );
                            // the item card, after the pointer rests; instant
                            // from one card to the next (egui's tooltip grace)
                            if prefs.wardrobe.tips && response.hovered() && st.wardrobe.menu.is_none() {
                                response.clone().on_hover_ui(|ui| tips::card(ui, p, tile, cell.item, inv, facts));
                            }
                            let on_stack = stack.is_some_and(|badge| {
                                ui.input(|i| i.pointer.interact_pos())
                                    .is_some_and(|pos| badge.expand(3.0).contains(pos))
                            });
                            if response.clicked() && on_stack {
                                toggle_stack = Some(tile.item);
                            } else if response.clicked() || response.double_clicked() {
                                clicked = Some((
                                    cell,
                                    response.double_clicked(),
                                    tile.copies.len() > 1 && st.wardrobe.open_stack != Some(tile.item),
                                ));
                            }
                            if response.secondary_clicked() {
                                menu_for = Some((cell.item, response.interact_pointer_pos().unwrap_or(rect.center())));
                            }
                        }
                    }
                    Row::Heading(..) => {}
                }
            }
        });
    widgets::remember_scroll_bar(ui, ui.scope_id().with("wardrobe_grid_bar"), &output);
    // the right-click menu of a card, owned by the grid: its row may leave
    // the view while the menu is open
    let menu_id = egui::Id::unique("wardrobe_card_menu");
    if let Some((item, pos)) = menu_for {
        st.wardrobe.selected = Some(item);
        st.wardrobe.menu = Some((item, pos));
        egui::Popup::open_id(ui.ctx(), menu_id);
    }
    if let Some((item, pos)) = st.wardrobe.menu {
        match st.wardrobe.model.tile_of(&item).cloned() {
            Some(tile) => {
                menu::context_menu_at(ui, menu_id, pos, p, |ui| tile_menu(ui, p, inv, st, facts, &tile, item, actions));
                if !egui::Popup::is_id_open(ui.ctx(), menu_id) {
                    st.wardrobe.menu = None;
                }
            }
            None => st.wardrobe.menu = None,
        }
    }
    if let Some(stack) = toggle_stack {
        st.wardrobe.open_stack = if st.wardrobe.open_stack == Some(stack) { None } else { Some(stack) };
    }
    if let Some((cell, double, closed_stack)) = clicked {
        st.wardrobe.selected = Some(cell.item);
        request_focus(ui, st);
        // looked at: no longer new
        if st.wardrobe.model.tiles[cell.tile].new {
            wardrobe::with(|shared| {
                shared.wardrobe.change(false, |txn| {
                    if txn.account().bindings.contains_key(&cell.item) {
                        txn.binding(cell.item).new = false;
                    }
                })
            });
        }
        if double {
            if facts.worn.contains(&cell.item) {
                take_off(inv, facts, cell.item, actions);
            } else {
                wear(st, inv, facts, cell.item, true, actions);
            }
        } else if closed_stack {
            // a click on a stack opens its copies in place
            st.wardrobe.open_stack = Some(st.wardrobe.model.tiles[cell.tile].item);
        } else if st.wardrobe.try_on {
            try_on(st, inv, facts, cell.item, actions);
        }
    }
}

/// How much of an animation's time is left by the viewer's "Reduce
/// animations" (the theme sets egui's animation time to zero then).
fn motion(ui: &egui::Ui) -> f32 {
    if ui.style().animation_time > 0.0 { 1.0 } else { 0.0 }
}

/// One card. Returns the rect of its stack badge, which opens and closes
/// the stack.
#[allow(clippy::too_many_arguments)]
fn paint_cell(
    ui: &egui::Ui,
    p: &Palette,
    rect: Rect,
    side: f32,
    tile: &Tile,
    cell: Cell,
    view: &CellView,
    wanted: &mut HashSet<Uuid>,
    facts: &Facts,
    images: &std::collections::HashMap<Uuid, egui::TextureHandle>,
    with_sub: bool,
    response: &egui::Response,
) -> Option<Rect> {
    let selected = view.selected == Some(cell.item);
    let move_time = motion(ui);
    // hover: the tile rises 2 px and its edge brightens; a new selection
    // settles from 97 %
    let lift = ui
        .ctx()
        .animate_bool_with_time(response.id.with("lift"), response.hovered(), 0.16 * move_time)
        * 2.0;
    let settle = ui
        .ctx()
        .animate_bool_with_time(response.id.with("pick"), selected, 0.24 * move_time);
    let scale = if selected { 0.97 + 0.03 * settle } else { 1.0 };
    let picture = Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + side / 2.0 - lift),
        Vec2::splat(side * scale),
    );
    let worn = facts.worn.contains(&cell.item);
    let opened = tile.copies.len() > 1 && view.open_stack == Some(tile.item);
    // a copy of an open stack shows its own picture and state
    let dim = if tile.fit.fits() { 1.0 } else { 0.55 };
    paint_picture(ui, p, picture, tile, images, wanted, dim);
    let radius = ui.visuals().window_corner_radius;
    let edge = if selected {
        egui::Stroke::new(2.0, p.violet)
    } else if response.hovered() {
        egui::Stroke::new(1.0, p.violet_light.gamma_multiply(0.7))
    } else {
        egui::Stroke::NONE
    };
    if edge != egui::Stroke::NONE {
        ui.painter().rect_stroke(picture, radius, edge, egui::StrokeKind::Inside);
    }
    // the worn check, top left
    if worn {
        let mark = Rect::from_min_size(picture.min + Vec2::splat(5.0), Vec2::splat(16.0));
        ui.painter().rect_filled(mark, widgets::inner_radius(ui.visuals()), p.teal);
        widgets::paint_glyph(ui.painter(), "check", mark.center(), 12.0, p.on_accent);
    }
    // top right: NEW, then the stack's count
    let mut right = picture.right() - 5.0;
    let top = picture.top() + 5.0;
    let is_new = wardrobe_new(tile, cell, opened);
    if is_new {
        let pill = badge(
            ui,
            aurora_i18n::tr!("wardrobe-new"),
            egui::pos2(right, top),
            egui::Align2::RIGHT_TOP,
            p.on_accent,
            p.violet,
        );
        right = pill.left() - 3.0;
    }
    let mut stack = None;
    if tile.copies.len() > 1 {
        let (ink, fill) = if opened { (p.on_accent, p.indigo) } else { (p.ink, p.bar) };
        stack = Some(badge(
            ui,
            &format!("×{}", tile.copies.len()),
            egui::pos2(right, top),
            egui::Align2::RIGHT_TOP,
            ink,
            fill,
        ));
    }
    if view.trying == Some(cell.item) {
        badge(
            ui,
            aurora_i18n::tr!("wardrobe-trying-tag"),
            picture.center_top() + Vec2::new(0.0, 5.0),
            egui::Align2::CENTER_TOP,
            p.on_accent,
            p.teal,
        );
    }
    // bottom left: the item comes with HUDs
    let huds = if opened { 0 } else { tile.huds.len() };
    if huds > 0 {
        let text = if huds == 1 {
            aurora_i18n::tr!("wardrobe-plus-hud").to_owned()
        } else {
            aurora_i18n::tr!("wardrobe-plus-huds", count = huds)
        };
        badge(
            ui,
            &text,
            picture.left_bottom() + Vec2::new(5.0, -5.0),
            egui::Align2::LEFT_BOTTOM,
            p.ink,
            p.bar,
        );
    }
    // bottom right: the store's logo, rendered and never printed
    if let Some(logo) = tile.logo {
        match images.get(&logo) {
            Some(texture) => {
                let at = Rect::from_min_size(picture.right_bottom() - Vec2::splat(23.0), Vec2::splat(18.0));
                widgets::square_picture(ui, at, texture);
            }
            None => {
                wanted.insert(logo);
            }
        }
    }
    if tile.favorite && !opened {
        widgets::paint_glyph(
            ui.painter(),
            "heart-filled",
            egui::pos2(picture.left() + 12.0, picture.bottom() - if huds > 0 { 26.0 } else { 12.0 }),
            11.0,
            p.rose,
        );
    }
    // the name, and under it the type and the store
    let name = match view.names.get(&cell.item).filter(|_| opened) {
        Some(name) => name.as_str(),
        None => tile.name.as_str(),
    };
    let ink = if tile.fit.fits() { p.ink } else { p.muted };
    let galley = elided(ui, name, 12.0, ink, side);
    ui.painter().galley(egui::pos2(rect.left(), rect.top() + side + 4.0), galley, ink);
    if with_sub {
        let mut sub = tile.main().map(tag_label).unwrap_or_default();
        if opened {
            sub = if tile.original == Some(cell.item) {
                aurora_i18n::tr!("wardrobe-copy-original").to_owned()
            } else if tile.copies.first() == Some(&cell.item) {
                aurora_i18n::tr!("wardrobe-copy-newest").to_owned()
            } else {
                sub
            };
        } else if let Some(store) = &tile.store {
            sub = if sub.is_empty() {
                store.clone()
            } else {
                format!("{sub} · {store}")
            };
        }
        let galley = elided(ui, &sub, 10.5, p.muted, side);
        ui.painter()
            .galley(egui::pos2(rect.left(), rect.top() + side + 4.0 + NAME_H), galley, p.muted);
    }
    stack
}

/// NEW shows on a closed tile when one of its copies is new.
fn wardrobe_new(tile: &Tile, cell: Cell, opened: bool) -> bool {
    if !opened {
        return tile.new;
    }
    wardrobe::with(|shared| shared.wardrobe.account.bindings.get(&cell.item).is_some_and(|b| b.new))
}

/// What a card reads of the window's state.
struct CellView<'a> {
    selected: Option<Uuid>,
    open_stack: Option<Uuid>,
    trying: Option<Uuid>,
    /// The names of the copies of an open stack.
    names: &'a std::collections::HashMap<Uuid, String>,
}

/// The arrows move the selection, Enter wears or takes off, Space opens
/// Quick Look: when the grid has the keyboard and no field does.
fn keyboard(
    ui: &egui::Ui,
    st: &mut InventoryUi,
    inv: &Inventory,
    facts: &Facts,
    cells: &[Cell],
    columns: usize,
    actions: &mut Vec<InvAction>,
) {
    // the names of the copies of an open stack, read once per frame
    st.wardrobe.copy_names.clear();
    for cell in cells {
        if st.wardrobe.model.tiles[cell.tile].copies.len() > 1
            && let Some(item) = inv.items.get(&cell.item)
        {
            st.wardrobe.copy_names.insert(cell.item, item.name.clone());
        }
    }
    // the grid holds the keyboard as the inventory's list does
    // (`clipboard::list_focus`): its keys are taken from the frame's input,
    // so that the arrows do not walk the avatar nor Enter open the chat
    let held = ui.is_enabled()
        && st.wardrobe.quick_look.is_none()
        && ui.memory(|m| m.has_focus(focus_id(st)) && m.allows_interaction(ui.layer_id()))
        && !ui.ctx().text_edit_focused()
        && !egui::Popup::is_any_open(ui.ctx());
    if !held {
        return;
    }
    let none = egui::Modifiers::NONE;
    let (mut step, mut enter, mut space) = (0isize, false, false);
    ui.input_mut(|i| {
        if i.consume_key(none, egui::Key::ArrowRight) {
            step = 1;
        } else if i.consume_key(none, egui::Key::ArrowLeft) {
            step = -1;
        } else if i.consume_key(none, egui::Key::ArrowDown) {
            step = columns as isize;
        } else if i.consume_key(none, egui::Key::ArrowUp) {
            step = -(columns as isize);
        }
        enter = i.consume_key(none, egui::Key::Enter);
        space = i.consume_key(none, egui::Key::Space);
    });
    st.wardrobe.moved_by_key = step != 0;
    let at = st.wardrobe.selected.and_then(|id| cells.iter().position(|c| c.item == id));
    if step != 0 && !cells.is_empty() {
        let next = match at {
            Some(at) => (at as isize + step).clamp(0, cells.len() as isize - 1) as usize,
            None => 0,
        };
        st.wardrobe.selected = Some(cells[next].item);
    }
    let Some(item) = st.wardrobe.selected else { return };
    if enter {
        if facts.worn.contains(&item) {
            take_off(inv, facts, item, actions);
        } else {
            wear(st, inv, facts, item, true, actions);
        }
    }
    if space {
        st.wardrobe.open_quick_look(item);
    }
}

/// The focus target of the grid: stable while its cards scroll out of view.
fn focus_id(st: &InventoryUi) -> egui::Id {
    egui::Id::unique(("wardrobe_grid_focus", st.ui_id))
}

fn request_focus(ui: &egui::Ui, st: &InventoryUi) {
    ui.memory_mut(|m| {
        m.request_focus(focus_id(st));
        m.set_focus_lock_filter(
            focus_id(st),
            egui::EventFilter {
                vertical_arrows: true,
                horizontal_arrows: true,
                ..Default::default()
            },
        );
    });
}

/// Nothing to show: an empty Wardrobe says how to fill it, a filter that
/// keeps nothing says so.
fn empty(ui: &mut egui::Ui, p: &Palette, st: &mut InventoryUi) {
    let nothing = st.wardrobe.model.tiles.is_empty();
    ui.add_space(28.0);
    ui.vertical_centered(|ui| {
        widgets::glyph(ui, "hanger-2-filled", p.muted_dim, 40.0);
        ui.add_space(8.0);
        let title = if nothing {
            aurora_i18n::tr!("wardrobe-empty-title")
        } else {
            aurora_i18n::tr!("wardrobe-empty-filter")
        };
        ui.label(egui::RichText::new(title).size(14.0).strong().color(p.ink));
        if nothing {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(aurora_i18n::tr!("wardrobe-empty-body"))
                    .size(12.0)
                    .color(p.muted),
            );
            ui.add_space(10.0);
            let waiting = st.wardrobe.model.sortable;
            if waiting > 0 {
                let label = aurora_i18n::tr!("wardrobe-auto-sort-button", count = waiting);
                let width = widgets::action_button_fit(ui, "wand", &label);
                if widgets::action_button(ui, p, "wand", &label, widgets::ActionKind::Primary, Vec2::new(width, 28.0)).clicked() {
                    let filed = wardrobe::with(|shared| {
                        let placed = shared.index.sortable.clone();
                        shared.wardrobe.change(true, |txn| sort::apply(txn, &placed))
                    });
                    st.wardrobe
                        .say(aurora_i18n::tr!("wardrobe-auto-sort-filed", count = filed.total, list = ""), true);
                }
            }
        }
    });
}
