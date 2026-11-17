//! The Wardrobe's fixed categories and subtypes, and what tells a category
//! without a card: the wearable type (docs/WARDROBE.md, section 4). Aurora's
//! own feature, no Legacy Viewer source; the wearable types are
//! LLWearableType's (indra/llappearance/llwearabletype.h).

use serde::{Deserialize, Serialize};

/// The subtype every category ends with: what the list does not name, and
/// a category given alone.
pub const OTHER: &str = "other";
/// Categories an item may carry, subtypes per category.
pub const MAX_CATEGORIES: usize = 4;
pub const MAX_SUBTYPES: usize = 2;

pub struct Subtype {
    /// Kebab-case English, the form the files keep.
    pub key: &'static str,
    /// The English name a card is written with.
    pub name: &'static str,
    /// Message key of the interface.
    pub label: &'static str,
}

pub struct Category {
    pub key: &'static str,
    pub name: &'static str,
    pub label: &'static str,
    pub subtypes: &'static [Subtype],
}

include!("categories_table.rs");

/// One category of an item with its subtypes, the first being the main one.
/// Written `["tops", ["t-shirt", "crop-top"]]` in the files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag(pub String, pub Vec<String>);

impl Tag {
    pub fn new(category: &str, subtype: &str) -> Self {
        Tag(category.to_owned(), vec![subtype.to_owned()])
    }

    pub fn category(&self) -> &str {
        &self.0
    }

    /// The main subtype: the first, `other` when none is written.
    pub fn subtype(&self) -> &str {
        self.1.first().map_or(OTHER, String::as_str)
    }

    pub fn has_subtype(&self, key: &str) -> bool {
        self.1.iter().any(|s| s == key) || (self.1.is_empty() && key == OTHER)
    }
}

/// Kebab case of a name as a card writes it: "Animation (AO)" →
/// "animation-ao", "Full body" → "full-body".
pub fn kebab(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

pub fn category(key: &str) -> Option<&'static Category> {
    WARDROBE.iter().find(|c| c.key == key)
}

pub fn subtype(category_key: &str, key: &str) -> Option<&'static Subtype> {
    category(category_key)?.subtypes.iter().find(|s| s.key == key)
}

/// The interface's name of a category, the key itself when the list does
/// not have it (a file of a later version).
pub fn category_label(key: &str) -> &str {
    category(key).map_or(key, |c| aurora_i18n::text(c.label))
}

pub fn subtype_label<'a>(category_key: &str, key: &'a str) -> &'a str {
    subtype(category_key, key).map_or(key, |s| aurora_i18n::text(s.label))
}

/// A category as a card writes it, by its name or its key, whatever the
/// case and the punctuation; a lone plural or singular is accepted ("Top",
/// "HUD", "Accessory").
pub fn parse_category(written: &str) -> Option<&'static Category> {
    let key = kebab(written);
    category(&key).or_else(|| {
        WARDROBE.iter().find(|c| {
            c.key.strip_suffix('s') == Some(key.as_str())
                || key.strip_suffix('s') == Some(c.key)
                || (c.key == "accessories" && key == "accessory")
                || (c.key == "full-body" && key == "fullbody")
                || (c.key == "body-parts" && matches!(key.as_str(), "body" | "bodyparts"))
        })
    })
}

pub fn parse_subtype(category: &'static Category, written: &str) -> Option<&'static Subtype> {
    let key = kebab(written);
    category
        .subtypes
        .iter()
        .find(|s| s.key == key || s.key.strip_suffix('s') == Some(key.as_str()) || key.strip_suffix('s') == Some(s.key))
}

/// What a kit is, to compare like with like (section 9).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Part {
    Body,
    Head,
    Ears,
    Hands,
    Feet,
    Tail,
    Wings,
    AddOn,
    Other,
}

impl Part {
    /// The part of a kit, from the main category and subtype of the item
    /// that declares it.
    pub fn of(tag: Option<&Tag>) -> Part {
        let Some(tag) = tag else {
            return Part::Other;
        };
        match (tag.category(), tag.subtype()) {
            ("body-parts", "mesh-body") => Part::Body,
            ("heads", "mesh-head") => Part::Head,
            ("body-parts", "ears") => Part::Ears,
            ("body-parts", "hands") => Part::Hands,
            ("body-parts", "feet") => Part::Feet,
            ("body-parts", "tail") => Part::Tail,
            ("body-parts", "wings") => Part::Wings,
            ("body-parts", "add-on") => Part::AddOn,
            _ => Part::Other,
        }
    }

    pub fn label(self) -> &'static str {
        aurora_i18n::text(match self {
            Part::Body => "wardrobe-part-body",
            Part::Head => "wardrobe-part-head",
            Part::Ears => "wardrobe-part-ears",
            Part::Hands => "wardrobe-part-hands",
            Part::Feet => "wardrobe-part-feet",
            Part::Tail => "wardrobe-part-tail",
            Part::Wings => "wardrobe-part-wings",
            Part::AddOn => "wardrobe-part-add-on",
            Part::Other => "wardrobe-part-other",
        })
    }
}

/// LLAssetType::AT_CLOTHING, AT_OBJECT, AT_BODYPART.
pub const ASSET_CLOTHING: i32 = 5;
pub const ASSET_OBJECT: i32 = 6;
pub const ASSET_BODYPART: i32 = 13;

/// Whether an inventory item can be a Wardrobe item at all: a wearable or
/// an object, never a link, an animation, a landmark, a notecard or a script.
pub fn wardrobe_asset(asset_type: i32) -> bool {
    matches!(asset_type, ASSET_CLOTHING | ASSET_OBJECT | ASSET_BODYPART)
}

/// The category of a system wearable, from its LLWearableType (the low byte
/// of the item's flags); `None` for physics, which is not a Wardrobe item.
pub fn of_wearable(kind: u8) -> Option<Tag> {
    let (category, subtype) = match kind {
        // shape, skin, hair, eyes
        0 => ("body-parts", "body-shape"),
        1 => ("body-parts", "body-skin"),
        2 => ("hair", OTHER),
        3 => ("heads", "eyes"),
        // shirt, pants, shoes, socks, jacket, gloves
        4 | 8 | 10 => ("tops", OTHER),
        5 | 11 => ("bottoms", OTHER),
        6 | 7 => ("shoes", OTHER),
        9 => ("accessories", "gloves"),
        12 => ("bottoms", "skirt"),
        13 => ("body-parts", "alpha-mask"),
        // tattoo, universal
        14 | 16 => ("body-parts", "tattoo"),
        _ => return None,
    };
    Some(Tag::new(category, subtype))
}

/// Caps a list of categories to the limits and merges a repeated category
/// into its first entry, keeping the order.
pub fn normalise(tags: Vec<Tag>) -> Vec<Tag> {
    let mut out: Vec<Tag> = Vec::new();
    for Tag(category, subtypes) in tags {
        let at = match out.iter().position(|t| t.0 == category) {
            Some(at) => at,
            None if out.len() < MAX_CATEGORIES => {
                out.push(Tag(category, Vec::new()));
                out.len() - 1
            }
            None => continue,
        };
        for subtype in subtypes {
            if out[at].1.len() < MAX_SUBTYPES && !out[at].1.contains(&subtype) {
                out[at].1.push(subtype);
            }
        }
    }
    for tag in &mut out {
        if tag.1.is_empty() {
            tag.1.push(OTHER.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nine_categories_each_ending_in_other() {
        assert_eq!(WARDROBE.len(), 9);
        for category in WARDROBE {
            assert_eq!(category.subtypes.last().map(|s| s.key), Some(OTHER), "{}", category.key);
            assert_eq!(kebab(category.name), category.key);
            for subtype in category.subtypes {
                assert_eq!(kebab(subtype.name), subtype.key);
            }
        }
    }

    #[test]
    fn every_name_has_its_three_translations() {
        for language in ["en", "fr", "ja"] {
            for category in WARDROBE {
                assert_ne!(aurora_i18n::text_in(language, category.label), category.label);
                for subtype in category.subtypes {
                    assert_ne!(aurora_i18n::text_in(language, subtype.label), subtype.label, "{language}");
                }
            }
        }
    }

    #[test]
    fn a_card_names_a_category_loosely() {
        assert_eq!(parse_category("Full body").map(|c| c.key), Some("full-body"));
        assert_eq!(parse_category(" body PARTS ").map(|c| c.key), Some("body-parts"));
        assert_eq!(parse_category("HUD").map(|c| c.key), Some("huds"));
        assert_eq!(parse_category("Top").map(|c| c.key), Some("tops"));
        assert_eq!(parse_category("Furniture").map(|c| c.key), None);
        let huds = parse_category("HUDs").expect("category");
        assert_eq!(parse_subtype(huds, "Animation (AO)").map(|s| s.key), Some("animation-ao"));
        let tops = parse_category("tops").expect("category");
        assert_eq!(parse_subtype(tops, "t-shirts").map(|s| s.key), Some("t-shirt"));
        assert_eq!(parse_subtype(tops, "Poncho").map(|s| s.key), None);
    }

    #[test]
    fn wearable_types_give_a_category() {
        assert_eq!(of_wearable(4), Some(Tag::new("tops", OTHER)));
        assert_eq!(of_wearable(12), Some(Tag::new("bottoms", "skirt")));
        assert_eq!(of_wearable(1), Some(Tag::new("body-parts", "body-skin")));
        assert_eq!(of_wearable(3), Some(Tag::new("heads", "eyes")));
        assert_eq!(of_wearable(16), Some(Tag::new("body-parts", "tattoo")));
        assert_eq!(of_wearable(9), Some(Tag::new("accessories", "gloves")));
        assert_eq!(of_wearable(15), None);
    }

    #[test]
    fn categories_merge_and_stay_within_the_caps() {
        let tags = normalise(vec![
            Tag("tops".into(), vec!["shirt".into()]),
            Tag("bottoms".into(), vec![]),
            Tag("tops".into(), vec!["jacket".into(), "coat".into()]),
            Tag("shoes".into(), vec![]),
            Tag("hair".into(), vec![]),
            Tag("heads".into(), vec![]),
        ]);
        assert_eq!(tags.len(), 4);
        assert_eq!(tags[0], Tag("tops".into(), vec!["shirt".into(), "jacket".into()]));
        assert_eq!(tags[1], Tag::new("bottoms", OTHER));
        assert!(tags[1].has_subtype(OTHER));
    }

    #[test]
    fn a_kit_part_follows_the_main_category() {
        assert_eq!(Part::of(Some(&Tag::new("body-parts", "mesh-body"))), Part::Body);
        assert_eq!(Part::of(Some(&Tag::new("heads", "mesh-head"))), Part::Head);
        assert_eq!(Part::of(Some(&Tag::new("body-parts", "ears"))), Part::Ears);
        assert_eq!(Part::of(Some(&Tag::new("tops", "shirt"))), Part::Other);
        assert_eq!(Part::of(None), Part::Other);
    }
}
