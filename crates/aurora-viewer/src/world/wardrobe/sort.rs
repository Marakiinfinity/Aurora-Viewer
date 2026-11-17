//! Auto-sort: files the items that are not in the Wardrobe yet and that the
//! viewer can place without a card (docs/WARDROBE.md, section 7). It stays
//! conservative: a system layer by its type, an attachment only when its
//! name gives one category without ambiguity, a worn HUD that belongs to no
//! item; everything else stays unsorted for the user.

use super::categories::{self, ASSET_OBJECT, OTHER, Tag, WARDROBE, wardrobe_asset};
use super::matching::{fold, is_link, names_hud};
use super::{Txn, Wardrobe, now};
use aurora_net::inventory::InvItem;
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

/// Words that name a category on their own, beyond the subtypes' names.
const EXTRA: &[(&str, &str, &str)] = &[
    ("top", "tops", OTHER),
    ("tee", "tops", "t-shirt"),
    ("tshirt", "tops", "t-shirt"),
    ("cardigan", "tops", "sweater"),
    ("blazer", "tops", "jacket"),
    ("pants", "bottoms", "trousers"),
    ("panties", "bottoms", "underwear"),
    ("dress", "full-body", "dress"),
    ("gown", "full-body", "dress"),
    ("bikini", "full-body", "swimwear"),
    ("swimsuit", "full-body", "swimwear"),
    ("shoes", "shoes", OTHER),
    ("pumps", "shoes", "heels"),
    ("stilettos", "shoes", "heels"),
    ("socks", "shoes", "socks-and-stockings"),
    ("stockings", "shoes", "socks-and-stockings"),
    ("hair", "hair", OTHER),
    ("hairstyle", "hair", OTHER),
    ("beard", "hair", "facial-hair"),
    ("earring", "accessories", "earrings"),
    ("choker", "accessories", "necklace"),
    ("sunglasses", "accessories", "glasses"),
    ("cap", "accessories", "hat"),
    ("beanie", "accessories", "hat"),
    ("purse", "accessories", "bag"),
    ("backpack", "accessories", "bag"),
];

/// The category a name gives, when its words point at one category only;
/// the subtype when they point at one subtype of it.
pub fn of_name(name: &str) -> Option<Tag> {
    let folded = fold(name);
    let words: HashSet<&str> = folded.split(' ').collect();
    let mut hits: Vec<(&str, &str)> = Vec::new();
    for category in WARDROBE.iter().filter(|c| c.key != "huds") {
        for subtype in category.subtypes.iter().filter(|s| s.key != OTHER) {
            // one-word subtypes only, in the singular or the plural: "short"
            // and "long" say nothing of a hair, "ring" is too common
            let key = subtype.key;
            if key.contains('-')
                || matches!(
                    key,
                    "short" | "long" | "ring" | "suit" | "mouth" | "eyes" | "hands" | "feet" | "ears" | "nails"
                )
            {
                continue;
            }
            let singular = key.strip_suffix('s').unwrap_or(key);
            if words.contains(key) || words.contains(singular) || words.contains(format!("{singular}s").as_str()) {
                hits.push((category.key, key));
            }
        }
    }
    for (word, category, subtype) in EXTRA {
        if words.contains(word) {
            hits.push((category, subtype));
        }
    }
    let category = hits.first()?.0;
    if hits.iter().any(|(c, _)| *c != category) {
        return None;
    }
    let named: Vec<&str> = hits.iter().map(|(_, s)| *s).filter(|s| *s != OTHER).collect();
    let subtype = match named.as_slice() {
        [one, rest @ ..] if rest.iter().all(|s| s == one) => one,
        _ => OTHER,
    };
    Some(Tag::new(category, subtype))
}

/// What Auto-sort would do with one item; `None` leaves it unsorted.
/// `own_hud`: a rule of section 8 attaches this object to an item.
pub fn place(item: &InvItem, worn_hud: bool, own_hud: bool) -> Option<Tag> {
    if !wardrobe_asset(item.asset_type) || is_link(item) {
        return None;
    }
    if item.asset_type != ASSET_OBJECT {
        return categories::of_wearable((item.flags & 0xff) as u8);
    }
    if own_hud {
        return None;
    }
    if worn_hud {
        return Some(Tag::new("huds", OTHER));
    }
    if names_hud(&item.name) {
        return None;
    }
    of_name(&item.name)
}

/// The outcome, for the note "Filed 5 new items · 2 Tops, 1 Shoes…".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filed {
    pub total: usize,
    /// Category key → count.
    pub by_category: BTreeMap<String, usize>,
}

/// The items Auto-sort can file now, with their place.
pub fn candidates(wardrobe: &Wardrobe, items: &[&InvItem], own_huds: &HashSet<Uuid>) -> Vec<(Uuid, Tag)> {
    items
        .iter()
        .filter(|it| !wardrobe.account.bindings.contains_key(&it.id))
        .filter_map(|it| place(it, wardrobe.account.worn_huds.contains(&it.id), own_huds.contains(&it.id)).map(|tag| (it.id, tag)))
        .collect()
}

pub fn apply(txn: &mut Txn, placed: &[(Uuid, Tag)]) -> Filed {
    let time = now();
    let mut filed = Filed::default();
    for (item, tag) in placed {
        if txn.account().bindings.contains_key(item) {
            continue;
        }
        let binding = txn.binding(*item);
        binding.sorted = vec![tag.clone()];
        binding.added = time;
        binding.new = true;
        filed.total += 1;
        *filed.by_category.entry(tag.category().to_owned()).or_default() += 1;
    }
    filed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::wardrobe::categories::{ASSET_BODYPART, ASSET_CLOTHING};

    fn item(name: &str, asset_type: i32, flags: u32) -> InvItem {
        InvItem {
            id: Uuid::new_v4(),
            name: name.into(),
            asset_type,
            flags,
            ..InvItem::default()
        }
    }

    #[test]
    fn a_name_gives_a_category_only_without_ambiguity() {
        assert_eq!(of_name("Cosy Hoodie (Lara) v2"), Some(Tag::new("tops", "hoodie")));
        assert_eq!(of_name("Leather boots"), Some(Tag::new("shoes", "boots")));
        assert_eq!(of_name("Summer Dress - red"), Some(Tag::new("full-body", "dress")));
        assert_eq!(of_name("Crop Top"), Some(Tag::new("tops", OTHER)));
        assert_eq!(of_name("Jacket and hoodie"), Some(Tag::new("tops", OTHER)));
        // two categories: left to the user
        assert_eq!(of_name("Skirt and boots set"), None);
        assert_eq!(of_name("Object"), None);
        assert_eq!(of_name("Long story"), None);
    }

    #[test]
    fn system_layers_follow_their_type_and_huds_their_rules() {
        assert_eq!(
            place(&item("Whatever", ASSET_CLOTHING, 5), false, false),
            Some(Tag::new("bottoms", OTHER))
        );
        assert_eq!(
            place(&item("Me", ASSET_BODYPART, 0), false, false),
            Some(Tag::new("body-parts", "body-shape"))
        );
        assert_eq!(place(&item("Bounce", ASSET_CLOTHING, 15), false, false), None);
        assert_eq!(place(&item("Dance", 20, 0), false, false), None);
        assert_eq!(place(&item("Radar", ASSET_OBJECT, 0), true, false), Some(Tag::new("huds", OTHER)));
        assert_eq!(place(&item("Sora head HUD", ASSET_OBJECT, 0), true, true), None);
        assert_eq!(place(&item("Boots HUD", ASSET_OBJECT, 0), false, false), None);
        assert_eq!(place(&item("Link", 24, 0), false, false), None);
    }

    #[test]
    fn auto_sort_files_once_and_counts_by_category() {
        let mut wardrobe = Wardrobe::default();
        let a = item("Cosy hoodie", ASSET_OBJECT, 0);
        let b = item("Old shirt", ASSET_CLOTHING, 4);
        let c = item("Mystery", ASSET_OBJECT, 0);
        let all = [&a, &b, &c];
        let placed = candidates(&wardrobe, &all, &HashSet::new());
        assert_eq!(placed.len(), 2);
        let filed = wardrobe.change(true, |txn| apply(txn, &placed));
        assert_eq!(filed.total, 2);
        assert_eq!(filed.by_category.get("tops"), Some(&2));
        assert!(wardrobe.account.bindings[&a.id].new);
        assert!(candidates(&wardrobe, &all, &HashSet::new()).is_empty());
        assert!(wardrobe.undo());
        assert!(wardrobe.account.bindings.is_empty());
    }
}
