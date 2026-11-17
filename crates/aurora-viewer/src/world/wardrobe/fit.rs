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

/// The seed table as known kits (patterns are the required words, joined).
pub fn seed() -> Vec<KnownKit> {
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
fn recognises(kit: &KnownKit, item: &InvItem) -> bool {
    if kit.creator.is_some_and(|c| c != item.creator) || names_hud(&item.name) {
        return false;
    }
    // every word of the name, its bracketed tags included: "[LEGACY]
    // Meshbody (f)" is told by its tags
    let words: HashSet<String> = all_words(&item.name).into_iter().collect();
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
    let seeded = seed();
    for kit in known.chain(&seeded) {
        let at = kits.add(&kit.name, kit.part.unwrap_or(Part::Other), &kit.sizes);
        for item in items
            .iter()
            .filter(|it| it.asset_type == ASSET_OBJECT && !is_link(it) && !declared.contains(&it.id))
        {
            if recognises(kit, item) {
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
    fn the_seed_recognises_a_body_and_not_what_was_made_for_it() {
        let lara = seed().into_iter().find(|k| k.name == "Lara").expect("seed");
        let body = InvItem {
            name: "Maitreya Mesh Body - Lara V5.3".into(),
            ..InvItem::default()
        };
        let dress = InvItem {
            name: "Night dress (Maitreya Lara)".into(),
            ..InvItem::default()
        };
        assert!(recognises(&lara, &body));
        assert!(!recognises(&lara, &dress));
        let mine = KnownKit {
            creator: Some(Uuid::from_u128(9)),
            ..lara
        };
        assert!(!recognises(&mine, &body));
    }
}
