//! The Wardrobe read against the inventory: which items can be Wardrobe
//! items, the items a card speaks of, and the index of what is computed
//! once per inventory generation and outfit change (HUDs, kits, fit, what
//! Auto-sort can file). docs/WARDROBE.md, sections 6 to 9.

use super::categories::{ASSET_OBJECT, Tag, wardrobe_asset};
use super::fit::{self, Fit, Kits};
use super::huds::{self, Around};
use super::matching::{fold, is_link, names_hud};
use super::{Wardrobe, sort};
use crate::world::inventory::Inventory;
use aurora_net::inventory::InvItem;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// FT_TRASH.
const FOLDER_TRASH: i32 = 14;
/// How deep and how wide a card looks under its folder.
const SCOPE_DEPTH: usize = 3;
const SCOPE_ITEMS: usize = 512;
/// The index is rebuilt at most this often while the inventory changes
/// (a login's folders arriving one after another).
const REBUILD_EVERY: std::time::Duration = std::time::Duration::from_millis(400);

fn descendants(inv: &Inventory, root: Uuid) -> HashSet<Uuid> {
    let mut seen = HashSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if seen.insert(id)
            && let Some(folder) = inv.folders.get(&id)
        {
            stack.extend(folder.children.iter().copied());
        }
    }
    seen
}

/// The items that can be Wardrobe items: the agent's own wearables and
/// objects, outside the Trash and the Library, links left out.
pub fn eligible(inv: &Inventory) -> Vec<&InvItem> {
    let trash = inv.system_folder(FOLDER_TRASH).map(|id| descendants(inv, id)).unwrap_or_default();
    inv.items
        .values()
        .filter(|it| wardrobe_asset(it.asset_type) && !is_link(it))
        .filter(|it| !trash.contains(&it.parent) && inv.folders.get(&it.parent).is_some_and(|f| !f.library))
        .collect()
}

/// The notecards whose name announces a Wardrobe card, outside the Trash
/// and the Library.
pub fn card_notecards(inv: &Inventory) -> Vec<Uuid> {
    let trash = inv.system_folder(FOLDER_TRASH).map(|id| descendants(inv, id)).unwrap_or_default();
    let mut found: Vec<Uuid> = inv
        .items
        .values()
        .filter(|it| it.asset_type == 7 && super::card::named_card(&it.name))
        .filter(|it| !trash.contains(&it.parent) && inv.folders.get(&it.parent).is_some_and(|f| !f.library))
        .map(|it| it.id)
        .collect();
    found.sort();
    found
}

/// The items a card of `folder` speaks of: the folder's own, and those of
/// its subfolders when the whole holds one creator's product.
pub fn card_scope(inv: &Inventory, folder: Uuid) -> Vec<&InvItem> {
    let items_of = |id: &Uuid| {
        inv.folders
            .get(id)
            .into_iter()
            .flat_map(|f| f.items.iter())
            .filter_map(|i| inv.items.get(i))
    };
    let own: Vec<&InvItem> = items_of(&folder).collect();
    let mut below: Vec<&InvItem> = Vec::new();
    let mut level: Vec<Uuid> = inv.folders.get(&folder).map(|f| f.children.clone()).unwrap_or_default();
    for _ in 0..SCOPE_DEPTH {
        let mut next = Vec::new();
        for id in &level {
            below.extend(items_of(id));
            if let Some(f) = inv.folders.get(id) {
                next.extend(f.children.iter().copied());
            }
        }
        level = next;
        if below.len() > SCOPE_ITEMS {
            return own;
        }
    }
    let mut creators = own
        .iter()
        .chain(&below)
        .filter(|it| wardrobe_asset(it.asset_type) && !is_link(it))
        .map(|it| it.creator);
    let one = match creators.next() {
        Some(first) => creators.all(|c| c == first),
        None => true,
    };
    if one { own.into_iter().chain(below).collect() } else { own }
}

/// What is computed from the Wardrobe, the inventory and the worn set.
#[derive(Default)]
pub struct Index {
    key: Option<(u64, u64, u64)>,
    built: Option<std::time::Instant>,
    /// The Wardrobe's revision last seen, and its `index_key` then.
    revision: u64,
    wardrobe_key: u64,
    /// The Wardrobe items that exist in the inventory.
    pub items: Vec<Uuid>,
    /// Item → its HUD items; and every HUD a rule attaches to an item.
    pub huds: HashMap<Uuid, Vec<Uuid>>,
    pub own_huds: HashSet<Uuid>,
    pub kits: Kits,
    pub fit: HashMap<Uuid, Fit>,
    /// What Auto-sort would file now, and what it leaves to the user.
    pub sortable: Vec<(Uuid, Tag)>,
    pub unsorted: Vec<Uuid>,
    /// Bumped at every rebuild: the views built on the index follow it.
    pub generation: u64,
}

fn worn_key(worn: &HashSet<Uuid>) -> u64 {
    worn.iter().fold(worn.len() as u128, |acc, id| acc ^ id.as_u128().rotate_left(17)) as u64
}

impl Index {
    /// Rebuilds the index when the inventory, the Wardrobe or the worn set
    /// changed. Returns whether it did.
    pub fn update(&mut self, wardrobe: &Wardrobe, inv: &Inventory, worn: &HashSet<Uuid>) -> bool {
        // a revision whose change the index does not read (a favourite, a
        // card seen) is taken as it is, without a rebuild
        if self.revision != wardrobe.revision {
            self.revision = wardrobe.revision;
            self.wardrobe_key = wardrobe.index_key();
        }
        let key = (inv.generation, self.wardrobe_key, worn_key(worn));
        // an edit of the Wardrobe or a change of outfit shows at once; the
        // inventory arriving folder by folder is followed a few times a
        // second
        let loading = self.key.is_some_and(|(_, revision, worn)| revision == key.1 && worn == key.2);
        if self.key == Some(key) || (loading && self.built.is_some_and(|at| at.elapsed() < REBUILD_EVERY)) {
            return false;
        }
        self.key = Some(key);
        self.built = Some(std::time::Instant::now());
        self.generation += 1;
        let all = eligible(inv);
        let mut by_folder: HashMap<Uuid, Vec<&InvItem>> = HashMap::new();
        for item in &all {
            by_folder.entry(item.parent).or_default().push(item);
        }
        self.items.clear();
        self.huds.clear();
        self.own_huds.clear();
        // the HUDs of every object of the Wardrobe, folder by folder
        for (folder, siblings) in &by_folder {
            let bound: Vec<&&InvItem> = siblings.iter().filter(|it| wardrobe.contains(&it.id)).collect();
            if bound.is_empty() {
                continue;
            }
            let hud_folder: Vec<&InvItem> = inv
                .folders
                .get(folder)
                .into_iter()
                .flat_map(|f| f.children.iter())
                .filter(|child| {
                    inv.folders
                        .get(child)
                        .is_some_and(|f| matches!(f.info.name.trim().to_lowercase().as_str(), "hud" | "huds"))
                })
                .flat_map(|child| by_folder.get(child).into_iter().flatten().copied())
                .collect();
            // one product: one creator, and one item once its copies and
            // its HUDs are set aside
            let creators: HashSet<Uuid> = siblings.iter().map(|it| it.creator).collect();
            let products: HashSet<String> = siblings.iter().filter(|it| !names_hud(&it.name)).map(|it| fold(&it.name)).collect();
            let one_product = creators.len() == 1 && products.len() == 1;
            for item in bound.iter().filter(|it| it.asset_type == ASSET_OBJECT) {
                let declared = wardrobe
                    .entry(item.id)
                    .and_then(|e| e.product.map(|p| p.huds.as_slice()))
                    .unwrap_or_default();
                let around = Around {
                    siblings,
                    hud_folder: &hud_folder,
                    one_product,
                    declared,
                };
                let (found, _) = huds::huds_of(item, &around, &wardrobe.account);
                if !found.is_empty() {
                    self.own_huds.extend(found.iter().copied());
                    self.huds.insert(item.id, found);
                }
            }
        }
        self.kits = fit::kits(wardrobe, &all, worn);
        self.fit.clear();
        for item in &all {
            // a HUD of an item is not a card of its own
            if self.own_huds.contains(&item.id) {
                continue;
            }
            if let Some(entry) = wardrobe.entry(item.id) {
                self.items.push(item.id);
                self.fit.insert(item.id, fit::fit(entry.fits(), &self.kits));
            }
        }
        self.sortable = sort::candidates(wardrobe, &all, &self.own_huds);
        let sortable: HashSet<Uuid> = self.sortable.iter().map(|(id, _)| *id).collect();
        self.unsorted = all
            .iter()
            .filter(|it| !wardrobe.contains(&it.id) && !sortable.contains(&it.id) && !self.own_huds.contains(&it.id))
            .map(|it| it.id)
            .collect();
        true
    }

    pub fn huds_of(&self, item: &Uuid) -> &[Uuid] {
        self.huds.get(item).map(Vec::as_slice).unwrap_or_default()
    }

    pub fn fit_of(&self, item: &Uuid) -> &Fit {
        self.fit.get(item).unwrap_or(&Fit::Everyone)
    }
}
