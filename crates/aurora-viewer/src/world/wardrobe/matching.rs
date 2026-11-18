//! Which inventory items a Wardrobe card speaks of, whether the card is
//! trusted, and what applying it changes (docs/WARDROBE.md, sections 6 and
//! 7). Pure rules over the items of the card's folder; nothing here reads a
//! file or the network.

use super::card::{Card, Fits, compare_versions};
use super::categories::{self, Part, wardrobe_asset};
use super::store::{CardState, Catalogue, KnownKit, Pack, Product, Source, Store};
use super::{Txn, now};
use aurora_net::inventory::InvItem;
use std::collections::HashMap;
use uuid::Uuid;

/// LLAssetType::AT_TEXTURE, AT_LINK, AT_LINK_FOLDER.
const ASSET_TEXTURE: i32 = 0;
const ASSET_LINK: i32 = 24;
const ASSET_LINK_FOLDER: i32 = 25;

pub fn is_link(item: &InvItem) -> bool {
    matches!(item.asset_type, ASSET_LINK | ASSET_LINK_FOLDER)
}

/// Every word of a name, lower case, its bracketed tags included.
pub fn all_words(text: &str) -> Vec<String> {
    words(text)
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Whether a word is a version or a copy number: "2", "v2", "1.3" (already
/// split at the dot), "v10b".
fn numbered(word: &str) -> bool {
    let digits = word.strip_prefix('v').unwrap_or(word);
    let digits = digits
        .strip_suffix(|c: char| c.is_ascii_alphabetic())
        .filter(|d| !d.is_empty())
        .unwrap_or(digits);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

fn without_brackets(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut depth = 0usize;
    for c in name.chars() {
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => {
                depth = depth.saturating_sub(1);
                out.push(' ');
            }
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn fold_words(mut words: Vec<String>) -> Vec<String> {
    while words.len() > 1 && words.last().is_some_and(|w| numbered(w)) {
        words.pop();
    }
    words
}

/// The name folding that tells two copies of one item: case, punctuation,
/// bracketed tags and trailing version or copy numbers ignored ("Fox Tail
/// (short) v2.1" → "fox tail"). A name that is only a tag keeps its words.
pub fn fold(name: &str) -> String {
    let plain = fold_words(words(&without_brackets(name)));
    let folded = if plain.is_empty() { fold_words(words(name)) } else { plain };
    folded.join(" ")
}

/// Whether a name says the item is a demo: "DEMO" as a word of its own
/// ("Sora head DEMO", "[demo] Band tee"), never part of another word.
pub fn names_demo(name: &str) -> bool {
    words(name).iter().any(|w| w == "demo" || w == "demos")
}

/// Whether a name says the item is a HUD: "HUD" as a word of its own.
pub fn names_hud(name: &str) -> bool {
    words(name).iter().any(|w| w == "hud" || w == "huds")
}

/// The folded name of what a HUD belongs to, from its own name: "Sora head
/// [HUD]", "Sora head HUD", "Sora head – HUD v2" → "sora head". `None` when
/// the name does not say HUD, or says nothing else.
pub fn hud_stem(name: &str) -> Option<String> {
    if !names_hud(name) {
        return None;
    }
    let kept: Vec<String> = words(name).into_iter().filter(|w| w != "hud" && w != "huds").collect();
    let folded = fold(&kept.join(" "));
    (!folded.is_empty()).then_some(folded)
}

/// A key part: kebab case of the folded name.
pub fn slug(name: &str) -> String {
    let slug = categories::kebab(&fold(name));
    if slug.is_empty() { "unnamed".to_owned() } else { slug }
}

/// What a card does to the items of its folder, before anything is applied:
/// the review sheet shows it, `apply` follows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub card_item: Uuid,
    pub card_creator: Uuid,
    /// The creator of the items the card names, when they have one creator.
    pub items_creator: Option<Uuid>,
    /// The card was made by the creator of every item it names.
    pub trusted: bool,
    pub pack_key: String,
    pub pack_name: String,
    /// The catalogue already holds a newer version of the pack: the pack
    /// and its items are left alone.
    pub stale: bool,
    /// For each line of `card.items` (by index), the items it binds.
    pub matched: Vec<(usize, Vec<Uuid>)>,
    /// For each `hud-for` line (by index), the HUD items found and the item
    /// line it names.
    pub huds: Vec<(usize, Vec<Uuid>, Option<usize>)>,
    /// The texture item of each line's `picture`.
    pub pictures: HashMap<usize, Uuid>,
    /// Names of the lines that name no item of the folder.
    pub skipped: Vec<String>,
    /// Items of the folder no line names.
    pub unnamed: usize,
}

impl Plan {
    pub fn item_count(&self) -> usize {
        self.matched.iter().map(|(_, items)| items.len()).sum()
    }
}

/// Reads what `card` would do. `items` are the items of the card's scope
/// (its folder, and the subfolders of a folder that holds one product).
pub fn plan(card: &Card, card_item: Uuid, card_creator: Uuid, folder_name: &str, items: &[&InvItem], catalogue: &Catalogue) -> Plan {
    let wearable: Vec<&InvItem> = items
        .iter()
        .copied()
        .filter(|it| wardrobe_asset(it.asset_type) && !is_link(it))
        .collect();
    // a line binds the items of its exact name, else those of its folded
    // name that no other line names exactly
    let exact: HashMap<String, usize> = card
        .items
        .iter()
        .enumerate()
        .rev()
        .map(|(n, l)| (l.name.trim().to_lowercase(), n))
        .collect();
    let folded: HashMap<String, usize> = card.items.iter().enumerate().rev().map(|(n, l)| (fold(&l.name), n)).collect();
    let hud_exact: HashMap<String, usize> = card
        .huds
        .iter()
        .enumerate()
        .rev()
        .map(|(n, h)| (h.name.trim().to_lowercase(), n))
        .collect();
    let hud_folded: HashMap<String, usize> = card.huds.iter().enumerate().rev().map(|(n, h)| (fold(&h.name), n)).collect();
    let mut matched: Vec<Vec<Uuid>> = vec![Vec::new(); card.items.len()];
    let mut huds: Vec<Vec<Uuid>> = vec![Vec::new(); card.huds.len()];
    let mut creators: Vec<Uuid> = Vec::new();
    let mut unnamed = 0;
    for item in &wearable {
        let lower = item.name.trim().to_lowercase();
        let name = fold(&item.name);
        // a `hud-for` line wins over an item line of the same folded name
        // only by its exact name
        let line = exact
            .get(&lower)
            .map(|n| (false, *n))
            .or_else(|| hud_exact.get(&lower).map(|n| (true, *n)));
        let line = line.or_else(|| {
            folded
                .get(&name)
                .map(|n| (false, *n))
                .or_else(|| hud_folded.get(&name).map(|n| (true, *n)))
        });
        match line {
            Some((false, n)) => matched[n].push(item.id),
            Some((true, n)) => huds[n].push(item.id),
            None => {
                unnamed += 1;
                continue;
            }
        }
        if !creators.contains(&item.creator) {
            creators.push(item.creator);
        }
    }
    let named = !creators.is_empty();
    let trusted = named && !card_creator.is_nil() && creators.iter().all(|c| *c == card_creator);
    let items_creator = match creators.as_slice() {
        [one] if !one.is_nil() => Some(*one),
        _ => None,
    };
    let pack_name = card
        .pack
        .as_ref()
        .and_then(|p| p.name.clone())
        .unwrap_or_else(|| folder_name.trim().to_owned());
    let owner = items_creator.unwrap_or(card_creator);
    let pack_key = format!("{owner}/{}", slug(&pack_name));
    let card_version = card.pack.as_ref().and_then(|p| p.version.as_deref());
    let stale = match (catalogue.packs.get(&pack_key).and_then(|p| p.version.as_deref()), card_version) {
        (Some(have), Some(new)) => compare_versions(new, have).is_lt(),
        _ => false,
    };
    let pictures = card
        .items
        .iter()
        .enumerate()
        .filter_map(|(n, line)| {
            let wanted = line.picture.as_ref()?.trim().to_lowercase();
            let texture = items
                .iter()
                .find(|it| it.asset_type == ASSET_TEXTURE && it.name.trim().to_lowercase() == wanted)
                .or_else(|| {
                    items
                        .iter()
                        .find(|it| it.asset_type == ASSET_TEXTURE && fold(&it.name) == fold(&wanted))
                })?;
            Some((n, texture.id))
        })
        .collect();
    let mut skipped: Vec<String> = card
        .items
        .iter()
        .zip(&matched)
        .filter(|(_, m)| m.is_empty())
        .map(|(l, _)| l.name.clone())
        .collect();
    skipped.extend(
        card.huds
            .iter()
            .zip(&huds)
            .filter(|(_, m)| m.is_empty())
            .map(|(h, _)| h.name.clone()),
    );
    let huds = card
        .huds
        .iter()
        .zip(huds)
        .enumerate()
        .map(|(n, (hud, found))| {
            let of = exact
                .get(&hud.of.trim().to_lowercase())
                .or_else(|| folded.get(&fold(&hud.of)))
                .copied();
            (n, found, of)
        })
        .collect();
    Plan {
        card_item,
        card_creator,
        items_creator,
        trusted,
        pack_key,
        pack_name,
        stale,
        matched: matched.into_iter().enumerate().filter(|(_, m)| !m.is_empty()).collect(),
        huds,
        pictures,
        skipped,
        unnamed,
    }
}

/// The three groups of the review sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Groups {
    pub store: bool,
    pub items: bool,
    pub pack: bool,
}

impl Groups {
    pub const ALL: Groups = Groups {
        store: true,
        items: true,
        pack: true,
    };

    pub fn count(self) -> usize {
        usize::from(self.store) + usize::from(self.items) + usize::from(self.pack)
    }
}

/// What applying a card did, for the toast.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Applied {
    pub store: Option<String>,
    pub pack: String,
    /// Items that joined the Wardrobe, and those already there that the
    /// card updated.
    pub added: usize,
    pub updated: usize,
    /// A store the user had linked by hand, replaced by the creator's.
    pub replaced_user_store: bool,
}

/// Applies the chosen groups of a card. The user's own values (the
/// bindings' overrides) are never touched; a record written by the items'
/// creator is never replaced by anyone else's card; a stale pack is left
/// alone. `embedded` are the items a card carries (name, asset id): the
/// creator's own records, kept to bind a renamed copy.
pub fn apply(txn: &mut Txn, card: &Card, plan: &Plan, groups: Groups, embedded: &[(String, Uuid)]) -> Applied {
    let mut done = Applied {
        pack: plan.pack_name.clone(),
        ..Applied::default()
    };
    let owner = plan.items_creator.unwrap_or(plan.card_creator);
    let source = if plan.trusted { Source::Card } else { Source::User };
    let version = card.pack.as_ref().and_then(|p| p.version.clone());
    // only the creator says which store is theirs
    if groups.store
        && plan.trusted
        && let Some(block) = card.store.as_ref().filter(|s| !s.name.is_empty())
    {
        done.replaced_user_store = txn
            .catalogue()
            .stores
            .get(&plan.card_creator)
            .is_some_and(|s| s.source == Source::User);
        txn.set_store(Store {
            key: plan.card_creator,
            name: block.name.clone(),
            logo: block.logo,
            logo_file: None,
            marketplace: block.marketplace.clone(),
            mainstore: block.mainstore.clone(),
            group: block.group,
            source: Source::Card,
            card_version: version.clone(),
        });
        done.store = Some(block.name.clone());
    }
    if plan.stale {
        txn.set_card(plan.card_item, CardState::Applied { version });
        return done;
    }
    let existing = txn.catalogue().packs.get(&plan.pack_key).cloned();
    let protected = |trusted_record: bool| trusted_record && !plan.trusted;
    if groups.pack && !protected(existing.as_ref().is_some_and(|p| p.source == Source::Card)) {
        let block = card.pack.clone().unwrap_or_default();
        txn.set_pack(Pack {
            key: plan.pack_key.clone(),
            store: owner,
            name: plan.pack_name.clone(),
            version: block.version,
            event: block.event,
            demo: block.demo,
            source,
        });
    } else if groups.items && existing.is_none() {
        // the items need their pack: its name alone
        txn.set_pack(Pack {
            key: plan.pack_key.clone(),
            store: owner,
            name: plan.pack_name.clone(),
            source,
            ..Pack::default()
        });
    }
    if groups.items {
        let time = now();
        for (n, items) in &plan.matched {
            let Some(line) = card.items.get(*n) else { continue };
            let key = format!("{}/{}", plan.pack_key, slug(&line.name));
            let before = txn.catalogue().products.get(&key).cloned();
            if !protected(before.as_ref().is_some_and(|p| p.trusted)) {
                let mut product = before.unwrap_or_default();
                product.key = key.clone();
                product.pack = plan.pack_key.clone();
                product.name = line.name.clone();
                if !line.categories.is_empty() {
                    product.categories = line.categories.clone();
                }
                product.fits = match &line.fits {
                    Fits::Unsaid => None,
                    Fits::Everyone => Some(Vec::new()),
                    Fits::Kits(kits) => Some(kits.clone()),
                };
                product.kit = line.kit.clone();
                product.part = (!line.kit.is_empty()).then(|| Part::of(product.categories.first()));
                product.huds = plan
                    .huds
                    .iter()
                    .filter(|(_, _, of)| *of == Some(*n))
                    .filter_map(|(h, _, _)| card.huds.get(*h).map(|hud| hud.name.clone()))
                    .collect();
                product.picture = line.picture.clone();
                product.note = line.note.clone();
                product.version = line.version.clone();
                product.event = line.event.clone();
                product.demo = line.demo;
                product.trusted = plan.trusted;
                if plan.trusted {
                    let name = fold(&line.name);
                    for (_, asset) in embedded
                        .iter()
                        .filter(|(embedded, asset)| fold(embedded) == name && !asset.is_nil())
                    {
                        if !product.asset_ids.contains(asset) {
                            product.asset_ids.push(*asset);
                        }
                    }
                }
                if !product.kit.is_empty() {
                    txn.learn_kit(KnownKit {
                        name: product.kit[0].clone(),
                        part: product.part,
                        creator: plan.items_creator,
                        patterns: vec![name_pattern(&line.name)],
                        sizes: product.kit[1..].to_vec(),
                    });
                }
                txn.set_product(product);
            }
            for item in items {
                let fresh = !txn.account().bindings.contains_key(item);
                let binding = txn.binding(*item);
                binding.product = Some(key.clone());
                if fresh {
                    binding.added = time;
                    binding.new = true;
                    done.added += 1;
                } else {
                    done.updated += 1;
                }
                if !binding.picture_by_user {
                    binding.picture = plan.pictures.get(n).copied().or(binding.picture);
                }
            }
        }
    }
    txn.set_card(plan.card_item, CardState::Applied { version });
    done
}

fn name_pattern(name: &str) -> String {
    fold(name)
}

/// Binds the unbound copies whose asset is one a creator's card recorded
/// (a renamed or redelivered copy that nobody edited). Returns how many.
pub fn rebind_by_asset(txn: &mut Txn, items: &[&InvItem]) -> usize {
    let by_asset: HashMap<Uuid, String> = txn
        .catalogue()
        .products
        .values()
        .flat_map(|p| p.asset_ids.iter().map(|a| (*a, p.key.clone())))
        .collect();
    if by_asset.is_empty() {
        return 0;
    }
    let time = now();
    let mut bound = 0;
    for item in items
        .iter()
        .filter(|it| wardrobe_asset(it.asset_type) && !is_link(it) && !it.asset_id.is_nil())
    {
        let Some(key) = by_asset.get(&item.asset_id) else { continue };
        if txn.account().bindings.get(&item.id).is_some_and(|b| b.product.is_some()) {
            continue;
        }
        let fresh = !txn.account().bindings.contains_key(&item.id);
        let binding = txn.binding(item.id);
        binding.product = Some(key.clone());
        if fresh {
            binding.added = time;
            binding.new = true;
        }
        bound += 1;
    }
    bound
}

/// "Same product as…": `item` joins the product of `other`, its own values
/// kept. Returns false when `other` has no product record to share.
pub fn same_product(txn: &mut Txn, item: &InvItem, other: &InvItem) -> bool {
    // a copy of a product has its creator: two creators are two products,
    // and joining them would hand one store's name to the other's items
    if item.id == other.id || item.creator != other.creator || !txn.account().bindings.contains_key(&other.id) {
        return false;
    }
    let key = match txn.account().bindings.get(&other.id).and_then(|b| b.product.clone()) {
        Some(key) => key,
        None => {
            // no card describes `other`: a record of the user's own, so
            // that the two (and later copies) stand as one product
            let key = format!("{}/user/{}", other.creator, slug(&other.name));
            if !txn.catalogue().products.contains_key(&key) {
                let categories = txn
                    .account()
                    .bindings
                    .get(&other.id)
                    .map(|b| b.categories.clone().unwrap_or_else(|| b.sorted.clone()))
                    .unwrap_or_default();
                txn.set_product(Product {
                    key: key.clone(),
                    name: other.name.clone(),
                    categories,
                    ..Product::default()
                });
            }
            txn.binding(other.id).product = Some(key.clone());
            key
        }
    };
    let fresh = !txn.account().bindings.contains_key(&item.id);
    let binding = txn.binding(item.id);
    binding.product = Some(key);
    if fresh {
        binding.added = now();
        binding.new = true;
    }
    true
}

/// "Not the same product": `item` leaves the product it was joined to and
/// keeps the categories it showed.
pub fn separate(txn: &mut Txn, item: Uuid) {
    let Some(key) = txn.account().bindings.get(&item).and_then(|b| b.product.clone()) else {
        return;
    };
    let shown = txn.catalogue().products.get(&key).map(|p| p.categories.clone()).unwrap_or_default();
    let binding = txn.binding(item);
    binding.product = None;
    if binding.categories.is_none() && binding.sorted.is_empty() {
        binding.sorted = shown;
    }
}
