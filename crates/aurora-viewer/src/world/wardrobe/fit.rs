//! Kits and fit (docs/WARDROBE.md, section 9). A kit is anything other
//! items are created for: a body, a head, ears, hands, a tail base, an
//! add-on, and a size of any of them. An item names every kit it was
//! created for and fits when one of them is worn; each part is judged on
//! its own, so changing heads never greys a shirt. A hint, never a refusal.

use super::Wardrobe;
use super::categories::{ASSET_OBJECT, Part};
use super::matching::{all_words, is_link, names_hud};
use super::store::KnownKit;
use aurora_net::inventory::InvItem;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// The mainstream bodies, heads and add-ons the viewer can name without a
/// card. Each is recognised among the attachments by the words its folded
/// name must all hold; no creator id is kept here, so a kit named by a
/// card or by the user always takes precedence.
const SEED: &[(&str, Part, &[&str], &[&str])] = &[
    (
        "Lara",
        Part::Body,
        &["maitreya", "body"],
        &["Maitreya", "Maitreya Lara", "Lara X", "LaraX", "Lara Petite", "Petite"],
    ),
    (
        "Legacy",
        Part::Body,
        &["legacy", "meshbody", "f"],
        &[
            "Legacy F",
            "Legacy Female",
            "Legacy Perky",
            "Perky",
            "Legacy Pinup",
            "Legacy Bombshell",
        ],
    ),
    (
        "Legacy Male",
        Part::Body,
        &["legacy", "meshbody", "m"],
        &["Legacy M", "Legacy Athletic"],
    ),
    (
        "Reborn",
        Part::Body,
        &["ebody", "reborn"],
        &["eBODY Reborn", "Reborn Juicy", "Reborn Waifu", "Waifu"],
    ),
    ("Kupra", Part::Body, &["inithium", "kupra"], &["Kupra Kups"]),
    ("Freya", Part::Body, &["belleza", "freya"], &["Belleza Freya"]),
    ("Jake", Part::Body, &["belleza", "jake"], &["Belleza Jake"]),
    ("Gianni", Part::Body, &["signature", "gianni"], &["Signature Gianni"]),
    ("Davis", Part::Body, &["signature", "davis"], &["Signature Davis"]),
    ("Anatomy", Part::Body, &["anatomy", "body"], &[]),
    (
        "Evo X",
        Part::Head,
        &["lelutka", "head"],
        &["LeLUTKA", "Lelutka Evo X", "EvoX", "Evolution X", "LeLUTKA EvoX"],
    ),
    ("Genus", Part::Head, &["genus", "head"], &["GENUS Project"]),
    ("Catwa", Part::Head, &["catwa", "head"], &["Catwa HDPRO", "HDPRO"]),
    ("Akeruka", Part::Head, &["akeruka", "head"], &["AK ADVX", "ADVX"]),
];

/// The preset bodies and heads, for the menu that says "this item is…".
pub fn presets() -> Vec<(&'static str, Part)> {
    SEED.iter().map(|(name, part, _, _)| (*name, *part)).collect()
}

/// The preset an item's name tells, when it is one: "Maitreya Mesh Body -
/// Lara V5.3" is Lara, a dress made for it is not.
pub fn preset_of(item: &InvItem) -> Option<(&'static str, Part)> {
    if item.asset_type != ASSET_OBJECT || is_link(item) {
        return None;
    }
    if names_hud(&item.name) {
        return None;
    }
    let words = name_words(item);
    seed()
        .iter()
        .position(|kit| recognises_words(kit, item, &words))
        .map(|at| (SEED[at].0, SEED[at].1))
}

/// The seed table as known kits (patterns are the required words, joined),
/// built once.
pub fn seed() -> &'static [KnownKit] {
    static KITS: std::sync::LazyLock<Vec<KnownKit>> = std::sync::LazyLock::new(seed_kits);
    &KITS
}

fn seed_kits() -> Vec<KnownKit> {
    SEED.iter()
        .map(|(name, part, words, sizes)| KnownKit {
            name: (*name).to_owned(),
            part: Some(*part),
            creator: None,
            patterns: vec![words.join(" ")],
            sizes: sizes.iter().map(|s| (*s).to_owned()).collect(),
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Kit {
    pub name: String,
    pub part: Part,
    /// The kit is worn now; owned: an item that provides it is in the
    /// inventory.
    pub worn: bool,
    pub owned: bool,
}

/// The kits of an account: what the items name, what the user owns and
/// what they wear. Rebuilt once per inventory generation and outfit change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kits {
    pub list: Vec<Kit>,
    /// Lower-case name, size or alias → index in `list`.
    names: HashMap<String, usize>,
}

fn key(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

impl Kits {
    fn add(&mut self, name: &str, part: Part, aliases: &[String]) -> usize {
        let at = match self.names.get(&key(name)) {
            Some(at) => *at,
            None => {
                self.list.push(Kit {
                    name: name.trim().to_owned(),
                    part,
                    worn: false,
                    owned: false,
                });
                self.names.insert(key(name), self.list.len() - 1);
                self.list.len() - 1
            }
        };
        for alias in aliases {
            self.names.entry(key(alias)).or_insert(at);
        }
        at
    }

    pub fn find(&self, name: &str) -> Option<&Kit> {
        self.names.get(&key(name)).map(|at| &self.list[*at])
    }

    pub fn any_worn(&self, part: Part) -> bool {
        self.list.iter().any(|k| k.worn && k.part == part)
    }

    pub fn worn(&self) -> impl Iterator<Item = &Kit> {
        self.list.iter().filter(|k| k.worn)
    }

    /// Whether two names are the same kit (a kit and one of its sizes).
    pub fn same(&self, a: &str, b: &str) -> bool {
        match (self.names.get(&key(a)), self.names.get(&key(b))) {
            (Some(a), Some(b)) => a == b,
            _ => key(a) == key(b),
        }
    }
}

/// Whether a known kit's pattern recognises an item: every word of the
/// pattern is a word of the item's name, the creator matches when the kit
/// names one, and the item is not a HUD.
#[cfg(test)]
fn recognises(kit: &KnownKit, item: &InvItem) -> bool {
    !names_hud(&item.name) && recognises_words(kit, item, &name_words(item))
}

/// Every word of the name, its bracketed tags included: "[LEGACY]
/// Meshbody (f)" is told by its tags.
fn name_words(item: &InvItem) -> HashSet<String> {
    all_words(&item.name).into_iter().collect()
}

/// `words`: the item's `name_words`, split once for every kit; a HUD is
/// left out by the caller.
fn recognises_words(kit: &KnownKit, item: &InvItem, words: &HashSet<String>) -> bool {
    if kit.creator.is_some_and(|c| c != item.creator) {
        return false;
    }
    kit.patterns
        .iter()
        .any(|pattern| !pattern.is_empty() && pattern.split(' ').all(|w| words.contains(w)))
}

/// The account's kits. `worn` are the inventory items worn now (the
/// attachments and the system layers); `items` every item of the inventory
/// that may provide a kit.
pub fn kits(wardrobe: &Wardrobe, items: &[&InvItem], worn: &HashSet<Uuid>) -> Kits {
    let mut kits = Kits::default();
    // 1. items that declare a kit: a card's `kit` clause, or the user
    let mut declared: HashSet<Uuid> = HashSet::new();
    for item in items {
        let Some(entry) = wardrobe.entry(item.id) else { continue };
        let names = entry.kit();
        let Some(first) = names.first() else { continue };
        let at = kits.add(first, entry.part(), &names[1..]);
        kits.list[at].owned = true;
        kits.list[at].worn |= worn.contains(&item.id);
        declared.insert(item.id);
    }
    // 2. the alias table: the user's kits, the cards', then the seed
    let known = wardrobe.account.my_kits.iter().chain(&wardrobe.catalogue.kits_known);
    // each name split once, for every kit
    let named: Vec<(&InvItem, HashSet<String>)> = items
        .iter()
        .filter(|it| it.asset_type == ASSET_OBJECT && !is_link(it) && !declared.contains(&it.id) && !names_hud(&it.name))
        .map(|it| (*it, name_words(it)))
        .collect();
    for kit in known.chain(seed()) {
        let at = kits.add(&kit.name, kit.part.unwrap_or(Part::Other), &kit.sizes);
        for (item, words) in &named {
            if recognises_words(kit, item, words) {
                kits.list[at].owned = true;
                kits.list[at].worn |= worn.contains(&item.id);
            }
        }
    }
    kits
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Fit {
    /// One of the item's kits is worn: its name.
    Fits(String),
    #[default]
    Everyone,
    /// The item names kits of `part` and none is worn. `nothing_worn`: no
    /// kit of that part is worn at all ("Needs Fae ears" rather than "Not
    /// for Aster").
    NotFor { kit: String, part: Part, nothing_worn: bool },
    /// No kit of the catalogue or the alias table has this name.
    Unknown(String),
}

impl Fit {
    pub fn fits(&self) -> bool {
        matches!(self, Fit::Fits(_) | Fit::Everyone)
    }
}

/// The fit of an item from the kits it names (`fits`; empty: everyone).
pub fn fit(fits: &[String], kits: &Kits) -> Fit {
    if fits.is_empty() {
        return Fit::Everyone;
    }
    let mut unknown: Option<&String> = None;
    let mut by_part: Vec<(Part, Vec<&Kit>)> = Vec::new();
    for name in fits {
        match kits.find(name) {
            Some(kit) => match by_part.iter_mut().find(|(part, _)| *part == kit.part) {
                Some((_, list)) => list.push(kit),
                None => by_part.push((kit.part, vec![kit])),
            },
            None => unknown = unknown.or(Some(name)),
        }
    }
    if by_part.is_empty() {
        return Fit::Unknown(unknown.cloned().unwrap_or_default());
    }
    by_part.sort_by_key(|(part, _)| *part);
    let mut worn_name = None;
    for (part, named) in &by_part {
        match named.iter().find(|k| k.worn) {
            Some(kit) => worn_name = worn_name.or(Some(kit.name.clone())),
            None => {
                return Fit::NotFor {
                    kit: named[0].name.clone(),
                    part: *part,
                    nothing_worn: !kits.any_worn(*part),
                };
            }
        }
    }
    Fit::Fits(worn_name.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kits(list: &[(&str, Part, bool, &[&str])]) -> Kits {
        let mut kits = Kits::default();
        for (name, part, worn, sizes) in list {
            let sizes: Vec<String> = sizes.iter().map(|s| (*s).to_owned()).collect();
            let at = kits.add(name, *part, &sizes);
            kits.list[at].worn = *worn;
        }
        kits
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn an_item_fits_when_one_of_its_kits_is_worn() {
        let k = kits(&[
            ("Aster", Part::Body, true, &["Aster Petite"]),
            ("Nova", Part::Body, false, &[]),
            ("Sora", Part::Head, true, &[]),
        ]);
        assert_eq!(fit(&names(&["Nova", "Aster"]), &k), Fit::Fits("Aster".into()));
        // a size of the worn kit
        assert_eq!(fit(&names(&["aster  petite"]), &k), Fit::Fits("Aster".into()));
        assert_eq!(fit(&[], &k), Fit::Everyone);
        assert_eq!(
            fit(&names(&["Nova"]), &k),
            Fit::NotFor {
                kit: "Nova".into(),
                part: Part::Body,
                nothing_worn: false
            }
        );
        assert_eq!(fit(&names(&["Mystery"]), &k), Fit::Unknown("Mystery".into()));
        // an unknown name next to a known one does not hide the known one
        assert_eq!(fit(&names(&["Mystery", "Aster"]), &k), Fit::Fits("Aster".into()));
    }

    #[test]
    fn each_part_is_judged_on_its_own() {
        // changing heads never greys a shirt
        let k = kits(&[
            ("Aster", Part::Body, true, &[]),
            ("Sora", Part::Head, false, &[]),
            ("Lumen", Part::Head, true, &[]),
            ("Fae ears", Part::Ears, false, &[]),
        ]);
        assert_eq!(fit(&names(&["Aster"]), &k), Fit::Fits("Aster".into()));
        // ears created for two heads: one of them is worn
        assert_eq!(fit(&names(&["Sora", "Lumen"]), &k), Fit::Fits("Lumen".into()));
        // cuffs created for ears nobody wears
        assert_eq!(
            fit(&names(&["Fae ears"]), &k),
            Fit::NotFor {
                kit: "Fae ears".into(),
                part: Part::Ears,
                nothing_worn: true
            }
        );
        // several parts on one item: the first that fails shows
        assert_eq!(
            fit(&names(&["Aster", "Sora"]), &k),
            Fit::NotFor {
                kit: "Sora".into(),
                part: Part::Head,
                nothing_worn: false
            }
        );
        assert!(k.same("aster", "Aster") && !k.same("Sora", "Lumen"));
    }

    #[test]
    fn a_preset_is_told_from_an_objects_name() {
        let object = |name: &str| InvItem {
            name: name.into(),
            asset_type: ASSET_OBJECT,
            ..InvItem::default()
        };
        assert_eq!(preset_of(&object("Maitreya Mesh Body - Lara V5.3")), Some(("Lara", Part::Body)));
        assert_eq!(preset_of(&object("LeLUTKA.Head.Avalon.3.1")), Some(("Evo X", Part::Head)));
        assert_eq!(preset_of(&object("Night dress (Maitreya Lara)")), None);
        assert_eq!(preset_of(&object("Maitreya Mesh Body HUD")), None);
        assert!(presets().iter().any(|(name, part)| *name == "Reborn" && *part == Part::Body));
    }

    #[test]
    fn the_seed_recognises_a_body_and_not_what_was_made_for_it() {
        let lara = seed().iter().find(|k| k.name == "Lara").expect("seed");
        let body = InvItem {
            name: "Maitreya Mesh Body - Lara V5.3".into(),
            ..InvItem::default()
        };
        let dress = InvItem {
            name: "Night dress (Maitreya Lara)".into(),
            ..InvItem::default()
        };
        assert!(recognises(lara, &body));
        assert!(!recognises(lara, &dress));
        let mine = KnownKit {
            creator: Some(Uuid::from_u128(9)),
            ..lara.clone()
        };
        assert!(!recognises(&mine, &body));
    }
}
