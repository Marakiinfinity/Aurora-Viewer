//! What the Wardrobe mode shows, taken once from the Wardrobe and the
//! inventory when either changes: one tile per item, or per product when
//! its copies stack; then the tiles a filter keeps, in their order
//! (docs/WARDROBE.md, section 10). Pure data: the window draws it.

use crate::world::inventory::Inventory;
use crate::world::wardrobe::Shared;
use crate::world::wardrobe::categories::{OTHER, Part, Tag};
use crate::world::wardrobe::fit::Fit;
use crate::world::wardrobe::store::SortBy;
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

/// "New this week".
const WEEK: i64 = 7 * 24 * 3600;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tile {
    /// The copy the tile acts on: the worn one, else the newest.
    pub item: Uuid,
    /// Every copy of the product in the inventory, `item` included, newest
    /// first; one entry when the tile is not a stack.
    pub copies: Vec<Uuid>,
    /// The untouched original among the copies (its asset is the
    /// product's), when one still exists.
    pub original: Option<Uuid>,
    pub name: String,
    pub lower: String,
    pub asset_type: i32,
    pub tags: Vec<Tag>,
    pub creator: Uuid,
    pub store: Option<String>,
    pub store_by_user: bool,
    pub logo: Option<Uuid>,
    pub marketplace: Option<String>,
    pub mainstore: Option<String>,
    pub pack: Option<String>,
    pub pack_key: Option<String>,
    pub event: Option<String>,
    pub version: Option<String>,
    pub note: Option<String>,
    pub demo: bool,
    pub trusted: bool,
    pub folder: Uuid,
    pub folder_name: String,
    pub desc: String,
    pub worn: bool,
    pub new: bool,
    pub favorite: bool,
    pub huds: Vec<Uuid>,
    /// The texture shown: the chosen picture, else the item's own image.
    pub picture: Option<Uuid>,
    pub added: i64,
    pub last_worn: i64,
    pub fits: Vec<String>,
    pub kit: Vec<String>,
    pub fit: Fit,
    /// No modify, no copy, no transfer for the agent.
    pub rights: [bool; 3],
}

impl Tile {
    pub fn main(&self) -> Option<&Tag> {
        self.tags.first()
    }

    pub fn in_category(&self, category: &str) -> bool {
        self.tags.iter().any(|t| t.category() == category)
    }

    pub fn has_subtype(&self, category: &str, subtype: &str) -> bool {
        self.tags.iter().any(|t| t.category() == category && t.has_subtype(subtype))
    }
}

type GroupsKey = ((u64, Browse, usize), std::rc::Rc<Vec<Group>>);

/// A kit of the "Created for" chip.
#[derive(Clone, Debug, PartialEq)]
pub struct KitRow {
    pub name: String,
    pub part: Part,
    pub worn: bool,
    /// Items created for it.
    pub count: usize,
}

#[derive(Default)]
pub struct Model {
    key: Option<(u64, bool, usize)>,
    pub tiles: Vec<Tile>,
    /// Any copy → its tile.
    pub by_item: HashMap<Uuid, usize>,
    /// Category key → tiles in it, and the categories holding a new item.
    pub counts: HashMap<String, usize>,
    pub new_in: HashSet<String>,
    pub worn: usize,
    pub kits: Vec<KitRow>,
    /// The events the tiles name, and for each category the subtypes in use.
    pub events: Vec<String>,
    pub subtypes: HashMap<String, Vec<String>>,
    /// The creators with their store, for the filters: computed with the
    /// names known when the model was built, and again when more are.
    groups: std::cell::RefCell<Option<GroupsKey>>,
    /// What Auto-sort can file, and what stays for the user.
    pub sortable: usize,
    pub unsorted: Vec<Uuid>,
    pub generation: u64,
}

impl Model {
    /// Rebuilds the tiles when the index moved. `worn`: the items worn now;
    /// `stacks`: the copies of a product show as one tile.
    pub fn update(&mut self, shared: &Shared, inv: &Inventory, worn: &HashSet<Uuid>, stacks: bool) -> bool {
        let key = (shared.index.generation, stacks, worn.len());
        if self.key == Some(key) {
            return false;
        }
        self.key = Some(key);
        self.generation += 1;
        let wardrobe = &shared.wardrobe;
        let mut tiles: Vec<Tile> = Vec::with_capacity(shared.index.items.len());
        let mut stack_of: HashMap<&str, usize> = HashMap::new();
        for id in &shared.index.items {
            let (Some(item), Some(entry)) = (inv.items.get(id), wardrobe.entry(*id)) else {
                continue;
            };
            let is_worn = worn.contains(id);
            if stacks
                && let Some(product) = entry.product
                && let Some(at) = stack_of.get(product.key.as_str()).copied()
            {
                // another copy of a product already shown: it joins the stack
                let tile = &mut tiles[at];
                tile.copies.push(*id);
                tile.worn |= is_worn;
                tile.new |= entry.binding.new;
                tile.favorite |= entry.binding.favorite;
                tile.last_worn = tile.last_worn.max(entry.binding.last_worn);
                if product.asset_ids.contains(&item.asset_id) {
                    tile.original = Some(*id);
                }
                continue;
            }
            let store = wardrobe.store_of(Some(&entry), item.creator);
            let held = crate::world::inventory::actions::held(inv, item);
            let picture = entry
                .binding
                .picture
                .and_then(|texture| inv.items.get(&texture))
                .map(|texture| texture.asset_id)
                .filter(|asset| !asset.is_nil())
                .or_else(|| (!item.thumbnail.is_nil()).then_some(item.thumbnail));
            use aurora_net::build::perm;
            let tile = Tile {
                item: *id,
                copies: vec![*id],
                original: entry.product.is_some_and(|p| p.asset_ids.contains(&item.asset_id)).then_some(*id),
                name: item.name.clone(),
                lower: item.name.to_lowercase(),
                asset_type: item.asset_type,
                tags: entry.categories().to_vec(),
                creator: item.creator,
                store: store.map(|s| s.name.clone()),
                store_by_user: store.is_some_and(|s| s.source == crate::world::wardrobe::store::Source::User),
                logo: store.and_then(|s| s.logo),
                marketplace: store.and_then(|s| s.marketplace.clone()),
                mainstore: store.and_then(|s| s.mainstore.clone()),
                pack: entry.pack.map(|p| p.name.clone()),
                pack_key: entry.pack.map(|p| p.key.clone()),
                event: entry.event().map(str::to_owned),
                version: entry.version().map(str::to_owned),
                note: entry.note().map(str::to_owned),
                demo: entry.demo(),
                trusted: entry.product.is_some_and(|p| p.trusted),
                folder: item.parent,
                folder_name: inv.folders.get(&item.parent).map(|f| f.info.name.clone()).unwrap_or_default(),
                desc: item.desc.clone(),
                worn: is_worn,
                new: entry.binding.new,
                favorite: entry.binding.favorite,
                huds: shared.index.huds_of(id).to_vec(),
                picture,
                added: entry.binding.added,
                last_worn: entry.binding.last_worn,
                fits: entry.fits().to_vec(),
                kit: entry.kit().to_vec(),
                fit: shared.index.fit_of(id).clone(),
                rights: [held & perm::MODIFY == 0, held & perm::COPY == 0, held & perm::TRANSFER == 0],
            };
            if stacks && let Some(product) = entry.product {
                stack_of.insert(product.key.as_str(), tiles.len());
            }
            tiles.push(tile);
        }
        // the copy a stack acts on: the worn one, else the newest
        for tile in tiles.iter_mut().filter(|t| t.copies.len() > 1) {
            let created = |id: &Uuid| inv.items.get(id).map_or(0, |it| it.created_at);
            tile.copies.sort_by_key(|id| std::cmp::Reverse(created(id)));
            let main = tile
                .copies
                .iter()
                .find(|id| worn.contains(id))
                .or(tile.copies.first())
                .copied()
                .unwrap_or(tile.item);
            if main != tile.item
                && let (Some(item), Some(entry)) = (inv.items.get(&main), wardrobe.entry(main))
            {
                tile.item = main;
                tile.name = item.name.clone();
                tile.lower = item.name.to_lowercase();
                tile.tags = entry.categories().to_vec();
                tile.huds = shared.index.huds_of(&main).to_vec();
                tile.fit = shared.index.fit_of(&main).clone();
                tile.fits = entry.fits().to_vec();
                tile.folder = item.parent;
            }
        }
        self.by_item = tiles
            .iter()
            .enumerate()
            .flat_map(|(n, t)| t.copies.iter().map(move |id| (*id, n)))
            .collect();
        self.counts.clear();
        self.new_in.clear();
        for tile in &tiles {
            for tag in &tile.tags {
                *self.counts.entry(tag.category().to_owned()).or_default() += 1;
                if tile.new {
                    self.new_in.insert(tag.category().to_owned());
                }
            }
        }
        self.worn = tiles.iter().filter(|t| t.worn).count();
        let events: std::collections::BTreeSet<&String> = tiles.iter().filter_map(|t| t.event.as_ref()).collect();
        self.events = events.into_iter().cloned().collect();
        self.subtypes.clear();
        for tile in &tiles {
            for tag in &tile.tags {
                let used = self.subtypes.entry(tag.category().to_owned()).or_default();
                let subtypes: &[String] = &tag.1;
                for subtype in subtypes.iter().map(String::as_str).chain(tag.1.is_empty().then_some(OTHER)) {
                    if !used.iter().any(|s| s == subtype) {
                        used.push(subtype.to_owned());
                    }
                }
            }
        }
        self.groups.replace(None);
        let mut kits: BTreeMap<(Part, String), KitRow> = BTreeMap::new();
        for tile in &tiles {
            for name in &tile.fits {
                let (display, part, is_worn) = match shared.index.kits.find(name) {
                    Some(kit) => (kit.name.clone(), kit.part, kit.worn),
                    None => (name.clone(), Part::Other, false),
                };
                kits.entry((part, display.to_lowercase()))
                    .or_insert(KitRow {
                        name: display,
                        part,
                        worn: is_worn,
                        count: 0,
                    })
                    .count += 1;
            }
        }
        self.kits = kits.into_values().collect();
        self.sortable = shared.index.sortable.len();
        self.unsorted = shared.index.unsorted.clone();
        self.tiles = tiles;
        true
    }

    pub fn tile_of(&self, item: &Uuid) -> Option<&Tile> {
        self.by_item.get(item).map(|n| &self.tiles[*n])
    }

    /// [`groups`], kept until the tiles, the way of browsing or the known
    /// names change: the rail and the filters ask for it every frame.
    pub fn groups(&self, browse: Browse, names: &HashMap<Uuid, String>) -> std::rc::Rc<Vec<Group>> {
        let key = (self.generation, browse, names.len());
        if let Some((kept, list)) = self.groups.borrow().as_ref()
            && *kept == key
        {
            return list.clone();
        }
        let list = std::rc::Rc::new(groups(self, browse, names));
        self.groups.replace(Some((key, list.clone())));
        list
    }
}

/// What the rail shows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Rail {
    #[default]
    All,
    Wearing,
    Category(String, Option<String>),
}

/// What "Created for" keeps.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum Created {
    #[default]
    Any,
    /// What fits the kits worn now.
    Mine,
    Everyone,
    Kit(String),
}

/// Where the search looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Scope {
    #[default]
    Everything,
    Names,
    Stores,
    Events,
    Folders,
    Descriptions,
}

impl Scope {
    pub const ALL: [Scope; 6] = [
        Scope::Everything,
        Scope::Names,
        Scope::Stores,
        Scope::Events,
        Scope::Folders,
        Scope::Descriptions,
    ];
}

/// How the sidebar groups the Wardrobe (Medium and Wide).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Browse {
    #[default]
    Category,
    Creator,
    Set,
    Event,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Filter {
    pub rail: Rail,
    pub browse: Browse,
    /// The creator, set or event chosen in the sidebar.
    pub pick: Option<String>,
    pub worn: bool,
    pub not_worn: bool,
    pub new: bool,
    pub favorites: bool,
    pub has_hud: bool,
    pub this_week: bool,
    pub created: Created,
    pub event: Option<String>,
    pub store: Option<String>,
    pub text: String,
    pub scope: Scope,
}

/// A search text read: its shortcuts (`creator:`, `event:`, `body:`,
/// `folder:`, `worn`, `new`) and the words left.
#[derive(Debug, Default, PartialEq)]
pub struct Query {
    pub words: Vec<String>,
    pub creator: Option<String>,
    pub event: Option<String>,
    pub body: Option<String>,
    pub folder: Option<String>,
    pub worn: bool,
    pub new: bool,
}

/// Reads a search text. A shortcut's value may be quoted to hold spaces.
pub fn query(text: &str) -> Query {
    let mut q = Query::default();
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            c => current.extend(c.to_lowercase()),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    for part in parts {
        let field = part.split_once(':').filter(|(_, value)| !value.is_empty());
        match (part.as_str(), field) {
            (_, Some(("creator" | "store", value))) => q.creator = Some(value.to_owned()),
            (_, Some(("event", value))) => q.event = Some(value.to_owned()),
            (_, Some(("body" | "kit" | "fits", value))) => q.body = Some(value.to_owned()),
            (_, Some(("folder" | "set", value))) => q.folder = Some(value.to_owned()),
            ("worn", _) => q.worn = true,
            ("new", _) => q.new = true,
            _ => q.words.push(part),
        }
    }
    q
}

fn holds(value: &str, needle: &str) -> bool {
    value.to_lowercase().contains(needle)
}

fn holds_some(value: Option<&String>, needle: &str) -> bool {
    value.is_some_and(|v| holds(v, needle))
}

/// What a word of the search matched, for the grouped results.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Hit {
    Event,
    Store,
    Folder,
    Item,
}

impl Tile {
    /// Where every word of the search is found in this tile, by `scope`;
    /// `None` when one word is found nowhere.
    pub fn hit(&self, words: &[String], scope: Scope, names: &HashMap<Uuid, String>) -> Option<Hit> {
        let creator = names.get(&self.creator);
        let mut best = Hit::Item;
        for word in words {
            let name = self.lower.contains(word.as_str());
            let store = holds_some(self.store.as_ref(), word) || holds_some(creator, word);
            let event = holds_some(self.event.as_ref(), word);
            let folder = holds(&self.folder_name, word) || holds_some(self.pack.as_ref(), word);
            let desc = holds(&self.desc, word) || holds_some(self.note.as_ref(), word);
            let hit = match scope {
                Scope::Names => name.then_some(Hit::Item),
                Scope::Stores => store.then_some(Hit::Store),
                Scope::Events => event.then_some(Hit::Event),
                Scope::Folders => folder.then_some(Hit::Folder),
                Scope::Descriptions => desc.then_some(Hit::Item),
                Scope::Everything => {
                    if name || desc {
                        Some(Hit::Item)
                    } else if event {
                        Some(Hit::Event)
                    } else if store {
                        Some(Hit::Store)
                    } else if folder {
                        Some(Hit::Folder)
                    } else {
                        None
                    }
                }
            }?;
            best = best.min(hit);
        }
        Some(best)
    }
}

/// The tiles `filter` keeps, in `sort`'s order. With a search text, each
/// comes with what it matched, and the list is grouped by it.
pub fn visible(
    model: &Model,
    filter: &Filter,
    sort: SortBy,
    reversed: bool,
    names: &HashMap<Uuid, String>,
    kits_same: impl Fn(&str, &str) -> bool,
    now: i64,
) -> Vec<(usize, Hit)> {
    let q = query(&filter.text);
    let mut out: Vec<(usize, Hit)> = model
        .tiles
        .iter()
        .enumerate()
        .filter(|(_, t)| match &filter.rail {
            Rail::All => true,
            Rail::Wearing => t.worn,
            Rail::Category(category, None) => t.in_category(category),
            Rail::Category(category, Some(subtype)) => t.has_subtype(category, subtype),
        })
        .filter(|(_, t)| match (filter.browse, &filter.pick) {
            (_, None) | (Browse::Category, _) => true,
            (Browse::Creator, Some(pick)) => t.creator.to_string() == *pick,
            (Browse::Set, Some(pick)) => t.pack_key.as_deref().unwrap_or("") == pick,
            (Browse::Event, Some(pick)) => t.event.as_deref().unwrap_or("") == pick,
        })
        .filter(|(_, t)| {
            (!(filter.worn || q.worn) || t.worn)
                && (!filter.not_worn || !t.worn)
                && (!(filter.new || q.new) || t.new)
                && (!filter.favorites || t.favorite)
                && (!filter.has_hud || !t.huds.is_empty())
                && (!filter.this_week || now - t.added <= WEEK)
        })
        .filter(|(_, t)| match &filter.created {
            Created::Any => true,
            Created::Mine => t.fit.fits(),
            Created::Everyone => t.fits.is_empty(),
            Created::Kit(kit) => t.fits.iter().any(|name| kits_same(name, kit)),
        })
        .filter(|(_, t)| filter.event.as_ref().is_none_or(|e| t.event.as_ref() == Some(e)))
        .filter(|(_, t)| filter.store.as_ref().is_none_or(|s| t.creator.to_string() == *s))
        .filter(|(_, t)| {
            q.creator
                .as_ref()
                .is_none_or(|c| holds_some(t.store.as_ref(), c) || holds_some(names.get(&t.creator), c))
                && q.event.as_ref().is_none_or(|e| holds_some(t.event.as_ref(), e))
                && q.body.as_ref().is_none_or(|b| t.fits.iter().any(|f| holds(f, b)))
                && q.folder
                    .as_ref()
                    .is_none_or(|f| holds(&t.folder_name, f) || holds_some(t.pack.as_ref(), f))
        })
        .filter_map(|(n, t)| t.hit(&q.words, filter.scope, names).map(|hit| (n, hit)))
        .collect();
    let tile = |n: usize| &model.tiles[n];
    out.sort_by(|(a, _), (b, _)| {
        let (a, b) = (tile(*a), tile(*b));
        let by_name = || crate::natural_order::key(&a.name).cmp(&crate::natural_order::key(&b.name));
        let order = match sort {
            SortBy::Added => b.added.cmp(&a.added).then_with(by_name),
            SortBy::LastWorn => b.last_worn.cmp(&a.last_worn).then_with(by_name),
            SortBy::Name => by_name(),
            SortBy::Store => {
                // the items without a store come last
                let key = |t: &Tile| (t.store.is_none(), t.store.as_deref().map(str::to_lowercase));
                key(a).cmp(&key(b)).then_with(by_name)
            }
            SortBy::Category => {
                let rank = |t: &Tile| {
                    t.main()
                        .and_then(|tag| {
                            crate::world::wardrobe::categories::WARDROBE
                                .iter()
                                .position(|c| c.key == tag.category())
                        })
                        .unwrap_or(usize::MAX)
                };
                let sub = |t: &Tile| t.main().map_or(OTHER, |tag| tag.subtype()).to_owned();
                rank(a).cmp(&rank(b)).then_with(|| sub(a).cmp(&sub(b))).then_with(by_name)
            }
        };
        if reversed { order.reverse() } else { order }
    });
    if !q.words.is_empty() {
        // grouped: events, stores, folders, then items (a stable sort)
        out.sort_by_key(|(_, hit)| *hit);
    }
    out
}

/// One entry of the sidebar's Browse by list.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// What `Filter::pick` takes.
    pub key: String,
    pub label: String,
    pub sub: Option<String>,
    pub creator: Option<Uuid>,
    pub logo: Option<Uuid>,
    pub count: usize,
}

/// The sidebar's entries for a way of browsing, the largest first for the
/// creators, by name otherwise.
pub fn groups(model: &Model, browse: Browse, names: &HashMap<Uuid, String>) -> Vec<Group> {
    let mut found: BTreeMap<String, Group> = BTreeMap::new();
    for tile in &model.tiles {
        let entry = match browse {
            Browse::Category => continue,
            Browse::Creator => Some((
                tile.creator.to_string(),
                tile.store.clone().or_else(|| names.get(&tile.creator).cloned()).unwrap_or_default(),
                tile.store.as_ref().and(names.get(&tile.creator)).cloned(),
                Some(tile.creator),
                tile.logo,
            )),
            Browse::Set => tile
                .pack_key
                .clone()
                .zip(tile.pack.clone())
                .map(|(key, name)| (key, name, tile.event.clone(), None, None)),
            Browse::Event => tile.event.clone().map(|event| (event.clone(), event, None, None, None)),
        };
        let Some((key, label, sub, creator, logo)) = entry else { continue };
        let group = found.entry(key.clone()).or_insert(Group {
            key,
            label,
            sub,
            creator,
            logo,
            count: 0,
        });
        group.count += 1;
        if group.label.is_empty() {
            group.label = names.get(&tile.creator).cloned().unwrap_or_default();
        }
    }
    let mut list: Vec<Group> = found.into_values().collect();
    match browse {
        Browse::Creator => list.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
        }),
        _ => list.sort_by_key(|g| g.label.to_lowercase()),
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(name: &str, category: &str, subtype: &str) -> Tile {
        Tile {
            item: Uuid::new_v4(),
            name: name.into(),
            lower: name.to_lowercase(),
            tags: vec![Tag::new(category, subtype)],
            fit: Fit::Everyone,
            ..Tile::default()
        }
    }

    fn model(tiles: Vec<Tile>) -> Model {
        Model { tiles, ..Model::default() }
    }

    fn names_of(list: &[(usize, Hit)], m: &Model) -> Vec<String> {
        list.iter().map(|(n, _)| m.tiles[*n].name.clone()).collect()
    }

    fn show(m: &Model, f: &Filter, sort: SortBy) -> Vec<String> {
        names_of(
            &visible(m, f, sort, false, &HashMap::new(), |a, b| a.eq_ignore_ascii_case(b), 1000),
            m,
        )
    }

    #[test]
    fn the_search_text_reads_its_shortcuts() {
        let q = query("Band tee creator:bloom event:\"moonlit market\" body:Aster worn NEW folder:set");
        assert_eq!(q.words, vec!["band".to_owned(), "tee".to_owned()]);
        assert_eq!(q.creator.as_deref(), Some("bloom"));
        assert_eq!(q.event.as_deref(), Some("moonlit market"));
        assert_eq!(q.body.as_deref(), Some("aster"));
        assert_eq!(q.folder.as_deref(), Some("set"));
        assert!(q.worn && q.new);
        // a colon with nothing after it is a plain word
        assert_eq!(query("creator:").words, vec!["creator:".to_owned()]);
    }

    #[test]
    fn the_rail_and_the_chips_filter() {
        let mut tee = tile("Tee", "tops", "t-shirt");
        tee.worn = true;
        tee.tags.push(Tag::new("full-body", "costume"));
        let mut boots = tile("Boots", "shoes", "boots");
        boots.favorite = true;
        boots.huds = vec![Uuid::new_v4()];
        boots.fits = vec!["Aster".into()];
        boots.fit = Fit::NotFor {
            kit: "Aster".into(),
            part: Part::Body,
            nothing_worn: false,
        };
        let m = model(vec![tee, boots, tile("Shirt", "tops", "shirt")]);
        let mut f = Filter::default();
        assert_eq!(show(&m, &f, SortBy::Name), ["Boots", "Shirt", "Tee"]);
        f.rail = Rail::Category("tops".into(), None);
        assert_eq!(show(&m, &f, SortBy::Name), ["Shirt", "Tee"]);
        f.rail = Rail::Category("tops".into(), Some("shirt".into()));
        assert_eq!(show(&m, &f, SortBy::Name), ["Shirt"]);
        // an item shows under each of its categories
        f.rail = Rail::Category("full-body".into(), None);
        assert_eq!(show(&m, &f, SortBy::Name), ["Tee"]);
        f.rail = Rail::Wearing;
        assert_eq!(show(&m, &f, SortBy::Name), ["Tee"]);
        f = Filter {
            favorites: true,
            has_hud: true,
            ..Filter::default()
        };
        assert_eq!(show(&m, &f, SortBy::Name), ["Boots"]);
        f = Filter {
            created: Created::Mine,
            ..Filter::default()
        };
        assert_eq!(show(&m, &f, SortBy::Name), ["Shirt", "Tee"]);
        f.created = Created::Kit("aster".into());
        assert_eq!(show(&m, &f, SortBy::Name), ["Boots"]);
        f.created = Created::Everyone;
        assert_eq!(show(&m, &f, SortBy::Name), ["Shirt", "Tee"]);
        f = Filter {
            not_worn: true,
            ..Filter::default()
        };
        assert_eq!(show(&m, &f, SortBy::Name), ["Boots", "Shirt"]);
    }

    #[test]
    fn the_orders_sort_and_reverse() {
        let mut a = tile("Alpha 10", "shoes", "boots");
        a.added = 5;
        a.store = Some("Zed".into());
        let mut b = tile("Alpha 2", "tops", "shirt");
        b.added = 9;
        b.last_worn = 3;
        let mut c = tile("Beta", "tops", "hoodie");
        c.added = 1;
        c.store = Some("Able".into());
        let m = model(vec![a, b, c]);
        let f = Filter::default();
        assert_eq!(show(&m, &f, SortBy::Added), ["Alpha 2", "Alpha 10", "Beta"]);
        assert_eq!(show(&m, &f, SortBy::Name), ["Alpha 2", "Alpha 10", "Beta"]);
        assert_eq!(show(&m, &f, SortBy::LastWorn), ["Alpha 2", "Alpha 10", "Beta"]);
        assert_eq!(show(&m, &f, SortBy::Store), ["Beta", "Alpha 10", "Alpha 2"]);
        assert_eq!(show(&m, &f, SortBy::Category), ["Beta", "Alpha 2", "Alpha 10"]);
        let reversed = visible(&m, &f, SortBy::Name, true, &HashMap::new(), |_, _| false, 0);
        assert_eq!(names_of(&reversed, &m), ["Beta", "Alpha 10", "Alpha 2"]);
    }

    #[test]
    fn the_search_follows_its_scope_and_groups_what_it_finds() {
        let mut tee = tile("Band tee", "tops", "t-shirt");
        tee.store = Some("Static Bloom".into());
        tee.event = Some("Moonlit Market".into());
        tee.folder_name = "Night market set".into();
        let mut cap = tile("Market cap", "accessories", "hat");
        cap.desc = "Worn at the market".into();
        let mut ring = tile("Ring", "accessories", "ring");
        ring.creator = Uuid::from_u128(7);
        let names: HashMap<Uuid, String> = [(Uuid::from_u128(7), "Market Maker".to_owned())].into();
        let m = model(vec![tee, cap, ring]);
        let find = |text: &str, scope: Scope| {
            let f = Filter {
                text: text.into(),
                scope,
                ..Filter::default()
            };
            visible(&m, &f, SortBy::Name, false, &names, |_, _| false, 0)
                .into_iter()
                .map(|(n, hit)| (m.tiles[n].name.clone(), hit))
                .collect::<Vec<_>>()
        };
        // grouped: the event first, then the creator, then the item
        assert_eq!(
            find("market", Scope::Everything),
            vec![
                ("Band tee".to_owned(), Hit::Event),
                ("Ring".to_owned(), Hit::Store),
                ("Market cap".to_owned(), Hit::Item)
            ]
        );
        assert_eq!(find("market", Scope::Names), vec![("Market cap".to_owned(), Hit::Item)]);
        assert_eq!(find("market", Scope::Folders), vec![("Band tee".to_owned(), Hit::Folder)]);
        assert_eq!(find("bloom tee", Scope::Everything).len(), 1);
        assert_eq!(find("event:moonlit", Scope::Everything).len(), 1);
        assert_eq!(find("creator:maker", Scope::Everything), vec![("Ring".to_owned(), Hit::Item)]);
        assert!(find("nothing", Scope::Everything).is_empty());
    }

    #[test]
    fn browse_by_lists_creators_sets_and_events_with_their_counts() {
        let creator = Uuid::from_u128(1);
        let mut a = tile("A", "tops", OTHER);
        a.creator = creator;
        a.store = Some("Static Bloom".into());
        a.pack = Some("Night market set".into());
        a.pack_key = Some("k/night".into());
        a.event = Some("Moonlit Market".into());
        let mut b = a.clone();
        b.name = "B".into();
        let mut c = tile("C", "shoes", OTHER);
        c.creator = Uuid::from_u128(2);
        let names: HashMap<Uuid, String> = [(Uuid::from_u128(2), "Kiko Makes".to_owned())].into();
        let m = model(vec![a, b, c]);
        let creators = groups(&m, Browse::Creator, &names);
        assert_eq!(
            creators.iter().map(|g| (g.label.as_str(), g.count)).collect::<Vec<_>>(),
            [("Static Bloom", 2), ("Kiko Makes", 1)]
        );
        let sets = groups(&m, Browse::Set, &names);
        assert_eq!((sets.len(), sets[0].count, sets[0].sub.as_deref()), (1, 2, Some("Moonlit Market")));
        assert_eq!(groups(&m, Browse::Event, &names)[0].label, "Moonlit Market");
        let f = Filter {
            browse: Browse::Creator,
            pick: Some(creator.to_string()),
            ..Filter::default()
        };
        assert_eq!(show(&m, &f, SortBy::Name), ["A", "B"]);
    }
}
