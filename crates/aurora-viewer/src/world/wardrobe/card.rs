//! The Wardrobe card: a plain notecard whose first line is
//! `# Aurora Wardrobe 1`, with `[store]`, `[pack]` and `[items]` blocks
//! (docs/WARDROBE.md, section 5). Parsed against this grammar and nothing
//! else: unknown keys and clauses are ignored, every value is capped, no
//! link is followed. Aurora's own format; the notecard that carries it is
//! decoded by `aurora_net::inventory::notecard` (LLNotecard,
//! indra/llinventory/llnotecard.cpp).

use super::categories::{self, MAX_CATEGORIES, MAX_SUBTYPES, OTHER, Tag};
use std::cmp::Ordering;
use uuid::Uuid;

/// What the first line starts with; the format's number follows.
pub const HEADER: &str = "# Aurora Wardrobe";
pub const FORMAT: u32 = 1;
/// Caps of section 5.
pub const MAX_VALUE: usize = 256;
pub const MAX_NOTE: usize = 24;
pub const MAX_ITEMS: usize = 64;
pub const MAX_NAMES: usize = 16;
/// The notecard limit (LLNotecard: 64 KiB of text).
pub const MAX_BYTES: usize = 65536;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoreBlock {
    pub name: String,
    /// The only id a card may carry: rendered, never printed.
    pub logo: Option<Uuid>,
    pub marketplace: Option<String>,
    pub mainstore: Option<String>,
    pub group: Option<Uuid>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackBlock {
    pub name: Option<String>,
    pub version: Option<String>,
    pub event: Option<String>,
    pub demo: bool,
}

/// What an item was created for.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Fits {
    /// No `fits` clause: nothing is said, which reads as everyone.
    #[default]
    Unsaid,
    Everyone,
    Kits(Vec<String>),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemLine {
    /// 1-based line of the card, for the reports.
    pub line: usize,
    pub name: String,
    /// Known categories, within the caps, the main one first. Empty when
    /// the card names none the list has.
    pub categories: Vec<Tag>,
    pub fits: Fits,
    pub kit: Vec<String>,
    pub picture: Option<String>,
    pub version: Option<String>,
    pub note: Option<String>,
    pub event: Option<String>,
    pub demo: Option<bool>,
}

/// `<hud name> = hud-for: <item name>`.
#[derive(Clone, Debug, PartialEq)]
pub struct HudLine {
    pub line: usize,
    pub name: String,
    pub of: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Warning {
    /// A line of a block that is not `key = value`.
    Malformed(usize),
    /// A category the list does not have, as written.
    UnknownCategory { line: usize, written: String },
    /// A subtype its category does not have, as written: filed as Other.
    UnknownSubtype { line: usize, written: String },
    /// Lines of `[items]` beyond the cap, dropped.
    TooManyItems(usize),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    /// The number of the first line; a later format is read the same way,
    /// its unknown keys ignored.
    pub format: u32,
    pub store: Option<StoreBlock>,
    pub pack: Option<PackBlock>,
    pub items: Vec<ItemLine>,
    pub huds: Vec<HudLine>,
    pub warnings: Vec<Warning>,
}

/// Whether a notecard's name announces a card ("Aurora Wardrobe", "# Aurora
/// Wardrobe — Night market set").
pub fn named_card(name: &str) -> bool {
    let name = name.trim_start_matches(['#', ' ']).to_lowercase();
    name.starts_with("aurora wardrobe")
}

/// Whether a notecard's text is a card: its first line is the header.
#[cfg(test)]
pub fn is_card(text: &str) -> bool {
    header_format(text).is_some()
}

/// A character that stands for an embedded item in a notecard's text
/// (LLViewerTextEditor's FIRST_EMBEDDED_CHAR, U+100000 and up).
fn embedded_marker(c: char) -> bool {
    c as u32 >= 0x10_0000
}

fn header_format(text: &str) -> Option<u32> {
    let first = text.trim_start_matches('\u{feff}').lines().find(|l| !l.trim().is_empty())?.trim();
    let head = first.get(..HEADER.len())?;
    if !head.eq_ignore_ascii_case(HEADER) {
        return None;
    }
    let rest = first[HEADER.len()..].trim();
    match rest.split_whitespace().next() {
        None => Some(FORMAT),
        Some(number) => number.parse().ok().filter(|n| *n >= 1),
    }
}

/// A value without control characters, cut at `max` bytes on a character.
fn cap(value: &str, max: usize) -> String {
    let clean: String = value.trim().chars().filter(|c| !c.is_control()).collect();
    if clean.len() <= max {
        return clean;
    }
    let mut end = max;
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    clean[..end].trim_end().to_owned()
}

fn cap_chars(value: &str, max: usize) -> String {
    cap(value, MAX_VALUE).chars().take(max).collect::<String>().trim_end().to_owned()
}

fn filled(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

fn names(list: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in list.split(',').map(|n| cap(n, MAX_VALUE)).filter(|n| !n.is_empty()) {
        if out.len() < MAX_NAMES && !out.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
    }
    out
}

fn yes(value: &str) -> bool {
    matches!(value.trim().to_lowercase().as_str(), "yes" | "true" | "1" | "on")
}

/// A Marketplace address a card may give: the Marketplace's own, over https.
pub fn valid_marketplace(url: &str) -> bool {
    url.strip_prefix("https://marketplace.secondlife.com/")
        .is_some_and(|rest| !rest.contains(char::is_whitespace))
}

/// A main store address a card may give: an SLurl, nothing else.
pub fn valid_mainstore(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    !url.contains(char::is_whitespace)
        && [
            "secondlife://",
            "https://maps.secondlife.com/secondlife/",
            "http://maps.secondlife.com/secondlife/",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix) && lower.len() > prefix.len())
}

/// The clauses of an `[items]` value: separated by two spaces or more, or a
/// tab.
fn clauses(value: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let gap = bytes[i] == b'\t' || (bytes[i] == b' ' && bytes.get(i + 1).is_some_and(|b| matches!(b, b' ' | b'\t')));
        if gap {
            out.push(value[start..i].trim());
            while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
                i += 1;
            }
            start = i;
        } else {
            i += 1;
        }
    }
    out.push(value[start..].trim());
    out.retain(|c| !c.is_empty());
    out
}

fn parse_categories(written: &str, line: usize, warnings: &mut Vec<Warning>) -> Vec<Tag> {
    let mut tags = Vec::new();
    for entry in written.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let (category, subtypes) = entry.split_once('/').unwrap_or((entry, ""));
        let Some(category) = categories::parse_category(category) else {
            warnings.push(Warning::UnknownCategory {
                line,
                written: cap(entry, MAX_VALUE),
            });
            continue;
        };
        let mut keys = Vec::new();
        for subtype in subtypes.split('+').map(str::trim).filter(|s| !s.is_empty()) {
            match categories::parse_subtype(category, subtype) {
                Some(found) => keys.push(found.key.to_owned()),
                None => {
                    warnings.push(Warning::UnknownSubtype {
                        line,
                        written: cap(subtype, MAX_VALUE),
                    });
                    keys.push(OTHER.to_owned());
                }
            }
        }
        tags.push(Tag(category.key.to_owned(), keys));
    }
    categories::normalise(tags)
}

fn parse_item(name: String, value: &str, line: usize, warnings: &mut Vec<Warning>) -> ItemLine {
    let mut item = ItemLine {
        line,
        name,
        ..ItemLine::default()
    };
    for (n, clause) in clauses(value).into_iter().enumerate() {
        let (word, rest) = clause.split_once(char::is_whitespace).unwrap_or((clause, ""));
        let rest = rest.trim();
        match word.to_ascii_lowercase().as_str() {
            "fits" if rest.eq_ignore_ascii_case("everyone") => item.fits = Fits::Everyone,
            "fits" => {
                let kits = names(rest);
                if !kits.is_empty() {
                    item.fits = Fits::Kits(kits);
                }
            }
            "kit" => item.kit = names(rest),
            "picture" => item.picture = filled(cap(rest, MAX_VALUE)),
            "version" => item.version = filled(cap(rest, MAX_VALUE)),
            "note" => item.note = filled(cap_chars(rest, MAX_NOTE)),
            "event" => item.event = filled(cap(rest, MAX_VALUE)),
            "demo" => item.demo = Some(yes(rest)),
            // the first clause is the categories; any other is of a later
            // format and ignored
            _ if n == 0 => item.categories = parse_categories(clause, line, warnings),
            _ => {}
        }
    }
    item
}

/// Reads a card; `None` when the text is not one (no header).
pub fn parse(text: &str) -> Option<Card> {
    #[derive(PartialEq)]
    enum Block {
        None,
        Store,
        Pack,
        Items,
        Unknown,
    }
    let format = header_format(text)?;
    // the markers of the items a notecard embeds are not text
    let plain: String;
    let text = if text.chars().any(embedded_marker) {
        plain = text.chars().filter(|c| !embedded_marker(*c)).collect();
        plain.as_str()
    } else {
        text
    };
    let text = if text.len() > MAX_BYTES {
        let mut end = MAX_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        &text[..end]
    } else {
        text
    };
    let mut card = Card { format, ..Card::default() };
    let mut block = Block::None;
    let mut dropped = 0;
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with('#') {
            continue;
        }
        if raw.starts_with('[') && raw.ends_with(']') {
            block = match raw[1..raw.len() - 1].trim().to_ascii_lowercase().as_str() {
                "store" => {
                    card.store.get_or_insert_with(StoreBlock::default);
                    Block::Store
                }
                "pack" => {
                    card.pack.get_or_insert_with(PackBlock::default);
                    Block::Pack
                }
                "items" => Block::Items,
                _ => Block::Unknown,
            };
            continue;
        }
        if block == Block::Unknown {
            continue;
        }
        let Some((key, value)) = raw.split_once('=') else {
            card.warnings.push(Warning::Malformed(line));
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match block {
            Block::Store => {
                let Some(store) = card.store.as_mut() else { continue };
                match key.to_ascii_lowercase().as_str() {
                    "name" => store.name = cap(value, MAX_VALUE),
                    "logo" => store.logo = Uuid::parse_str(value).ok().filter(|id| !id.is_nil()),
                    "marketplace" => store.marketplace = Some(cap(value, MAX_VALUE)).filter(|url| valid_marketplace(url)),
                    "mainstore" => store.mainstore = Some(cap(value, MAX_VALUE)).filter(|url| valid_mainstore(url)),
                    "group" => store.group = Uuid::parse_str(value).ok().filter(|id| !id.is_nil()),
                    _ => {}
                }
            }
            Block::Pack => {
                let Some(pack) = card.pack.as_mut() else { continue };
                match key.to_ascii_lowercase().as_str() {
                    "name" => pack.name = filled(cap(value, MAX_VALUE)),
                    "version" => pack.version = filled(cap(value, MAX_VALUE)),
                    "event" => pack.event = filled(cap(value, MAX_VALUE)),
                    "demo" => pack.demo = yes(value),
                    _ => {}
                }
            }
            Block::Items => {
                let name = cap(key, MAX_VALUE);
                if name.is_empty() || value.is_empty() {
                    card.warnings.push(Warning::Malformed(line));
                    continue;
                }
                if card.items.len() + card.huds.len() >= MAX_ITEMS {
                    dropped += 1;
                    continue;
                }
                let hud = value.get(..8).filter(|head| head.eq_ignore_ascii_case("hud-for:"));
                if hud.is_some() {
                    let of = cap(&value[8..], MAX_VALUE);
                    if of.is_empty() {
                        card.warnings.push(Warning::Malformed(line));
                    } else {
                        card.huds.push(HudLine { line, name, of });
                    }
                } else {
                    let item = parse_item(name, value, line, &mut card.warnings);
                    card.items.push(item);
                }
            }
            Block::None | Block::Unknown => card.warnings.push(Warning::Malformed(line)),
        }
    }
    if dropped > 0 {
        card.warnings.push(Warning::TooManyItems(dropped));
    }
    Some(card)
}

/// The categories of an item as a card writes them: "Tops/Shirt+Jacket,
/// Bottoms/Shorts"; a lone Other is left out.
pub fn write_categories(tags: &[Tag]) -> String {
    let mut out = String::new();
    for tag in tags.iter().take(MAX_CATEGORIES) {
        let Some(category) = categories::category(tag.category()) else {
            continue;
        };
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(category.name);
        let subtypes: Vec<&str> = tag
            .1
            .iter()
            .take(MAX_SUBTYPES)
            .filter_map(|key| category.subtypes.iter().find(|s| s.key == key))
            .map(|s| s.name)
            .collect();
        if subtypes != [categories::subtype(category.key, OTHER).map_or("Other", |s| s.name)] && !subtypes.is_empty() {
            out.push('/');
            out.push_str(&subtypes.join("+"));
        }
    }
    out
}

impl Card {
    /// The card as text, for the creator tool: what `parse` reads back.
    pub fn write(&self) -> String {
        fn pair(out: &mut String, key: &str, width: usize, value: &str) {
            out.push_str(&format!("{key:<width$} = {value}\n"));
        }
        let mut out = format!("{HEADER} {FORMAT}\n");
        if let Some(store) = &self.store {
            out.push_str("[store]\n");
            pair(&mut out, "name", 11, &store.name);
            if let Some(logo) = store.logo {
                pair(&mut out, "logo", 11, &logo.to_string());
            }
            if let Some(url) = &store.marketplace {
                pair(&mut out, "marketplace", 11, url);
            }
            if let Some(url) = &store.mainstore {
                pair(&mut out, "mainstore", 11, url);
            }
            if let Some(group) = store.group {
                pair(&mut out, "group", 11, &group.to_string());
            }
            out.push('\n');
        }
        if let Some(pack) = &self.pack {
            out.push_str("[pack]\n");
            if let Some(name) = &pack.name {
                pair(&mut out, "name", 7, name);
            }
            if let Some(version) = &pack.version {
                pair(&mut out, "version", 7, version);
            }
            if let Some(event) = &pack.event {
                pair(&mut out, "event", 7, event);
            }
            pair(&mut out, "demo", 7, if pack.demo { "yes" } else { "no" });
            out.push('\n');
        }
        out.push_str("[items]\n");
        let width = self
            .items
            .iter()
            .map(|i| i.name.chars().count())
            .chain(self.huds.iter().map(|h| h.name.chars().count()))
            .max()
            .unwrap_or(0)
            .min(40);
        for item in &self.items {
            let mut value = write_categories(&item.categories);
            let mut clause = |word: &str, text: &str| {
                if !value.is_empty() {
                    value.push_str("   ");
                }
                value.push_str(word);
                if !text.is_empty() {
                    value.push(' ');
                    value.push_str(text);
                }
            };
            if !item.kit.is_empty() {
                clause("kit", &item.kit.join(", "));
            }
            match &item.fits {
                Fits::Unsaid => {}
                Fits::Everyone => clause("fits", "everyone"),
                Fits::Kits(kits) => clause("fits", &kits.join(", ")),
            }
            if let Some(picture) = &item.picture {
                clause("picture", picture);
            }
            if let Some(version) = &item.version {
                clause("version", version);
            }
            if let Some(event) = &item.event {
                clause("event", event);
            }
            if let Some(demo) = item.demo {
                clause("demo", if demo { "yes" } else { "no" });
            }
            if let Some(note) = &item.note {
                clause("note", note);
            }
            pair(&mut out, &item.name, width, &value);
        }
        for hud in &self.huds {
            pair(&mut out, &hud.name, width, &format!("hud-for: {}", hud.of));
        }
        out
    }
}

/// Orders two versions: as dotted integers when both read as such ("1.10"
/// after "1.9", a leading "v" ignored), otherwise as text.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    fn dotted(v: &str) -> Option<Vec<u64>> {
        let v = v.trim();
        let v = v.strip_prefix(['v', 'V']).unwrap_or(v);
        (!v.is_empty()).then(|| v.split('.').map(|part| part.parse().ok()).collect::<Option<Vec<u64>>>())?
    }
    match (dotted(a), dotted(b)) {
        (Some(a), Some(b)) => {
            let len = a.len().max(b.len());
            let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
            (0..len)
                .map(|i| at(&a, i).cmp(&at(&b, i)))
                .find(|o| o.is_ne())
                .unwrap_or(Ordering::Equal)
        }
        _ => a.trim().to_lowercase().cmp(&b.trim().to_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Aurora Wardrobe 1
[store]
name        = Static Bloom
logo        = 11111111-2222-3333-4444-555555555555
marketplace = https://marketplace.secondlife.com/stores/12345
mainstore   = secondlife://Somewhere/128/128/25
group       = not-an-id
colour      = purple

[pack]
name    = Night market set
version = 1.2
event   = Moonlit Market
demo    = no

[items]
# a comment
Oversized band tee = Tops/T-shirt   fits Aster, Nova   picture band-tee-icon
Band tee HUD       = hud-for: Oversized band tee
Rib tattoo         = Body parts/Tattoo   fits everyone   version 1.3   note Faded ink, BOM
Fae ears           = Body parts/Ears   kit Fae ears   fits Sora, Lumen
Moon ear cuffs     = Accessories/Earrings   fits Fae ears
";

    #[test]
    fn the_sample_of_the_design_page_reads() {
        let card = parse(SAMPLE).expect("a card");
        assert_eq!(card.format, 1);
        let store = card.store.expect("store");
        assert_eq!(store.name, "Static Bloom");
        assert!(store.logo.is_some());
        assert_eq!(store.group, None);
        assert_eq!(store.mainstore.as_deref(), Some("secondlife://Somewhere/128/128/25"));
        let pack = card.pack.expect("pack");
        assert_eq!(pack.name.as_deref(), Some("Night market set"));
        assert_eq!(pack.version.as_deref(), Some("1.2"));
        assert!(!pack.demo);
        assert_eq!(card.items.len(), 4);
        assert_eq!(card.items[0].categories, vec![Tag::new("tops", "t-shirt")]);
        assert_eq!(card.items[0].fits, Fits::Kits(vec!["Aster".into(), "Nova".into()]));
        assert_eq!(card.items[0].picture.as_deref(), Some("band-tee-icon"));
        assert_eq!(card.items[1].fits, Fits::Everyone);
        assert_eq!(card.items[1].version.as_deref(), Some("1.3"));
        assert_eq!(card.items[1].note.as_deref(), Some("Faded ink, BOM"));
        assert_eq!(card.items[2].kit, vec!["Fae ears".to_owned()]);
        assert_eq!(card.items[3].fits, Fits::Kits(vec!["Fae ears".into()]));
        assert_eq!(
            card.huds,
            vec![HudLine {
                line: 19,
                name: "Band tee HUD".into(),
                of: "Oversized band tee".into()
            }]
        );
        assert!(card.warnings.is_empty(), "{:?}", card.warnings);
    }

    #[test]
    fn a_text_without_the_header_is_not_a_card() {
        assert!(parse("[items]\nTee = Tops").is_none());
        assert!(parse("").is_none());
        assert!(is_card("\n  # aurora wardrobe 1\n"));
        assert!(is_card("# Aurora Wardrobe"));
        assert!(!is_card("# Aurora Wardrobe zero"));
        assert!(named_card("# Aurora Wardrobe - Night market"));
        assert!(named_card("aurora wardrobe"));
        assert!(!named_card("Read me"));
    }

    #[test]
    fn the_markers_of_embedded_items_are_not_text() {
        let card = parse("# Aurora Wardrobe 1\n[items]\nTee = Tops\n\u{100000}\n\u{100001} \n").expect("card");
        assert_eq!(card.items.len(), 1);
        assert!(card.warnings.is_empty(), "{:?}", card.warnings);
    }

    #[test]
    fn several_categories_and_two_subtypes() {
        let card = parse("# Aurora Wardrobe 1\n[items]\nRomper = Tops/Shirt+Jacket+Coat, Bottoms/Shorts, tops/vest   note x\nAO = HUDs/RLVa+Animation (AO)\n")
            .expect("card");
        assert_eq!(
            card.items[0].categories,
            vec![
                Tag("tops".into(), vec!["shirt".into(), "jacket".into()]),
                Tag::new("bottoms", "shorts")
            ]
        );
        assert_eq!(
            card.items[1].categories,
            vec![Tag("huds".into(), vec!["rlva".into(), "animation-ao".into()])]
        );
    }

    #[test]
    fn unknown_names_are_reported_and_kept_apart() {
        let card =
            parse("# Aurora Wardrobe 1\n[items]\nSofa = Furniture/Seat\nPoncho = Tops/Poncho\nbroken line\n = Tops\n").expect("card");
        assert!(card.items[0].categories.is_empty());
        assert_eq!(card.items[1].categories, vec![Tag::new("tops", OTHER)]);
        assert_eq!(
            card.warnings,
            vec![
                Warning::UnknownCategory {
                    line: 3,
                    written: "Furniture/Seat".into()
                },
                Warning::UnknownSubtype {
                    line: 4,
                    written: "Poncho".into()
                },
                Warning::Malformed(5),
                Warning::Malformed(6),
            ]
        );
    }

    #[test]
    fn the_caps_hold() {
        let mut text = String::from("# Aurora Wardrobe 1\n[items]\n");
        for n in 0..70 {
            text.push_str(&format!("Item {n} = Tops\n"));
        }
        let card = parse(&text).expect("card");
        assert_eq!(card.items.len(), MAX_ITEMS);
        assert_eq!(card.warnings, vec![Warning::TooManyItems(6)]);

        let long = "é".repeat(400);
        let kits: Vec<String> = (0..20).map(|n| format!("Kit {n}")).collect();
        let card = parse(&format!(
            "# Aurora Wardrobe 1\n[pack]\nname = {long}\n[items]\nTee = Tops   note A note that is far too long to keep   fits {}\n",
            kits.join(", ")
        ))
        .expect("card");
        let name = card.pack.expect("pack").name.expect("name");
        assert!(name.len() <= MAX_VALUE && name.chars().all(|c| c == 'é'));
        assert_eq!(card.items[0].note.as_deref(), Some("A note that is far too l"));
        assert_eq!(card.items[0].note.as_ref().map(|n| n.chars().count()), Some(MAX_NOTE));
        let Fits::Kits(kits) = &card.items[0].fits else { panic!("kits") };
        assert_eq!(kits.len(), MAX_NAMES);
    }

    #[test]
    fn links_outside_the_rules_are_ignored() {
        let card = parse(
            "# Aurora Wardrobe 1\n[store]\nname = X\nmarketplace = https://example.com/stores/1\nmainstore = https://example.com/\n[items]\nA = Tops\n",
        )
        .expect("card");
        let store = card.store.expect("store");
        assert_eq!(store.marketplace, None);
        assert_eq!(store.mainstore, None);
        assert!(valid_mainstore("https://maps.secondlife.com/secondlife/Somewhere/1/2/3"));
        assert!(!valid_mainstore("secondlife://"));
        assert!(!valid_marketplace("http://marketplace.secondlife.com/stores/1"));
    }

    #[test]
    fn a_written_card_reads_back() {
        let mut card = parse(SAMPLE).expect("card");
        card.items[0].categories = vec![
            Tag("tops".into(), vec!["shirt".into(), "jacket".into()]),
            Tag::new("bottoms", OTHER),
        ];
        card.items[0].demo = Some(true);
        let text = card.write();
        let again = parse(&text).expect("card");
        assert_eq!(again.store, card.store);
        assert_eq!(again.pack, card.pack);
        assert_eq!(again.huds.len(), 1);
        assert!(again.warnings.is_empty(), "{:?}\n{text}", again.warnings);
        for (a, b) in again.items.iter().zip(&card.items) {
            assert_eq!(ItemLine { line: 0, ..a.clone() }, ItemLine { line: 0, ..b.clone() }, "{text}");
        }
        assert!(text.contains("= Tops/Shirt+Jacket, Bottoms   fits Aster, Nova"), "{text}");
    }

    #[test]
    fn versions_compare_as_numbers_then_as_text() {
        assert_eq!(compare_versions("1.10", "1.9"), Ordering::Greater);
        assert_eq!(compare_versions("v2", "2.0"), Ordering::Equal);
        assert_eq!(compare_versions("1.2", "1.2.1"), Ordering::Less);
        assert_eq!(compare_versions("beta", "Alpha"), Ordering::Greater);
        assert_eq!(compare_versions("1.2b", "1.2a"), Ordering::Greater);
    }
}
