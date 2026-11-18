//! The Wardrobe's two files (docs/WARDROBE.md, section 3): the catalogue of
//! stores, packs and products, shared by every account of the PC
//! (`wardrobe/catalogue.json` in the settings folder), and the account's
//! own file (`wardrobe-account.json`): which inventory item is which
//! product, the user's own values, the HUD links, the cards seen. Both are
//! pretty JSON with a `format` and a `version`, like the outfit backups
//! (`world/outfit_backup.rs`), read off the frame thread and written by a
//! thread of their own like the favourites' order (`world/favorites.rs`);
//! the demo keeps them in memory. Aurora's own feature, no Legacy Viewer
//! source.

use super::categories::{Part, Tag};
use crossbeam_channel::{self as mpsc, Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::thread::JoinHandle;
use uuid::Uuid;

pub const CATALOGUE_FOLDER: &str = "wardrobe";
pub const CATALOGUE_FILE: &str = "catalogue.json";
pub const ACCOUNT_FILE: &str = "wardrobe-account.json";
const CATALOGUE_FORMAT: &str = "aurora-wardrobe-catalogue";
const ACCOUNT_FORMAT: &str = "aurora-wardrobe-account";
const VERSION: u32 = 1;
/// A file larger than this is not a Wardrobe file.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Read from the creator's card.
    #[default]
    Card,
    /// Set by the user on this computer.
    User,
}

/// A shop. A store read from a creator's card has the creator's id as its
/// key; one the user made has a key of its own, and creators or single
/// items are linked to it (`Catalogue::creator_stores`, `Binding::store`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Store {
    pub key: Uuid,
    pub name: String,
    /// Rendered, never printed.
    #[serde(default)]
    pub logo: Option<Uuid>,
    /// A logo the user gave from a file or the clipboard: the name of its
    /// file in the shared `wardrobe/logos` folder. It wins over `logo`.
    #[serde(default)]
    pub logo_file: Option<String>,
    #[serde(default)]
    pub marketplace: Option<String>,
    #[serde(default)]
    pub mainstore: Option<String>,
    #[serde(default)]
    pub group: Option<Uuid>,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub card_version: Option<String>,
}

/// A product box.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pack {
    pub key: String,
    /// The creator the pack belongs to (a store's key when one is known).
    pub store: Uuid,
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub demo: bool,
    #[serde(default)]
    pub source: Source,
}

/// One product of a pack: what a card line says of an item. Every copy of
/// the item in an inventory binds to it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Product {
    pub key: String,
    pub pack: String,
    pub name: String,
    #[serde(default)]
    pub categories: Vec<Tag>,
    /// `None`: the card says nothing; an empty list: "fits everyone".
    #[serde(default)]
    pub fits: Option<Vec<String>>,
    /// The kit names this product provides to others.
    #[serde(default)]
    pub kit: Vec<String>,
    #[serde(default)]
    pub part: Option<Part>,
    /// Names of the HUD items of the pack declared for this product.
    #[serde(default)]
    pub huds: Vec<String>,
    /// Name of the texture item of the pack shown as the picture.
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default)]
    pub demo: Option<bool>,
    /// From a card that embeds the items; used to bind a renamed copy,
    /// never shown.
    #[serde(default)]
    pub asset_ids: Vec<Uuid>,
    /// Written by the items' creator: a card by anyone else never replaces it.
    #[serde(default)]
    pub trusted: bool,
}

/// A kit the viewer can recognise among the worn attachments.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KnownKit {
    pub name: String,
    pub part: Option<Part>,
    #[serde(default)]
    pub creator: Option<Uuid>,
    /// Folded item names (`matching::fold`) that provide the kit.
    #[serde(default)]
    pub patterns: Vec<String>,
    /// Other names of the same kit, its sizes included.
    #[serde(default)]
    pub sizes: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalogue {
    pub stores: BTreeMap<Uuid, Store>,
    pub packs: BTreeMap<String, Pack>,
    pub products: BTreeMap<String, Product>,
    pub kits_known: Vec<KnownKit>,
    /// The user's links: every item of this creator belongs to that store.
    pub creator_stores: BTreeMap<Uuid, Uuid>,
}

#[derive(Serialize, Deserialize)]
struct CatalogueFile {
    format: String,
    version: u32,
    #[serde(default)]
    stores: Vec<Store>,
    #[serde(default)]
    packs: Vec<Pack>,
    #[serde(default)]
    items: Vec<Product>,
    #[serde(default)]
    kits_known: Vec<KnownKit>,
    #[serde(default)]
    creator_stores: BTreeMap<Uuid, Uuid>,
}

impl Catalogue {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let file: CatalogueFile =
            serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes)).map_err(|e| e.to_string())?;
        if file.format != CATALOGUE_FORMAT || file.version != VERSION {
            return Err(format!("not a Wardrobe catalogue of version {VERSION}"));
        }
        Ok(Catalogue {
            stores: file.stores.into_iter().filter(|s| !s.key.is_nil()).map(|s| (s.key, s)).collect(),
            packs: file.packs.into_iter().map(|p| (p.key.clone(), p)).collect(),
            products: file.items.into_iter().map(|p| (p.key.clone(), p)).collect(),
            kits_known: file.kits_known,
            creator_stores: file
                .creator_stores
                .into_iter()
                .filter(|(creator, store)| !creator.is_nil() && !store.is_nil())
                .collect(),
        })
    }

    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        let file = CatalogueFile {
            format: CATALOGUE_FORMAT.into(),
            version: VERSION,
            stores: self.stores.values().cloned().collect(),
            packs: self.packs.values().cloned().collect(),
            items: self.products.values().cloned().collect(),
            kits_known: self.kits_known.clone(),
            creator_stores: self.creator_stores.clone(),
        };
        let mut bytes = serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Adds what `other` has and this one lacks: the records another
    /// account of the PC wrote since this one read the file. `removed`
    /// names what this session deleted, which must not come back.
    pub fn merge_missing(&mut self, other: Catalogue, removed: &Removed) {
        for (key, store) in other.stores {
            if !removed.stores.contains(&key) {
                self.stores.entry(key).or_insert(store);
            }
        }
        for (key, pack) in other.packs {
            if !removed.packs.contains(&key) {
                self.packs.entry(key).or_insert(pack);
            }
        }
        for (key, product) in other.products {
            if !removed.products.contains(&key) {
                self.products.entry(key).or_insert(product);
            }
        }
        for kit in other.kits_known {
            if !self
                .kits_known
                .iter()
                .any(|k| k.name.eq_ignore_ascii_case(&kit.name) && k.part == kit.part)
            {
                self.kits_known.push(kit);
            }
        }
        for (creator, store) in other.creator_stores {
            if !removed.links.contains(&creator) {
                self.creator_stores.entry(creator).or_insert(store);
            }
        }
    }
}

/// Catalogue records deleted this session (an undo, a removed store link).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Removed {
    pub stores: BTreeSet<Uuid>,
    /// Creators whose link to a store was taken away.
    pub links: BTreeSet<Uuid>,
    pub packs: BTreeSet<String>,
    pub products: BTreeSet<String>,
}

/// One inventory item of the account in the Wardrobe.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    /// The product record this copy belongs to, when a card or the user
    /// said so.
    #[serde(default)]
    pub product: Option<String>,
    /// What Auto-sort found for an item without a card (the lowest
    /// precedence).
    #[serde(default)]
    pub sorted: Vec<Tag>,
    /// The user's own values: they win over any card.
    #[serde(default)]
    pub categories: Option<Vec<Tag>>,
    #[serde(default)]
    pub fits: Option<Vec<String>>,
    #[serde(default)]
    pub kit: Option<Vec<String>>,
    #[serde(default)]
    pub note: Option<String>,
    /// The texture item of the inventory shown as the picture, and whether
    /// the user chose it (a card never replaces the user's).
    #[serde(default)]
    pub picture: Option<Uuid>,
    #[serde(default)]
    pub picture_by_user: bool,
    /// A picture the user chose from a file or the clipboard: the name of
    /// its file in the account's picture folder. It wins over `picture`.
    #[serde(default)]
    pub picture_file: Option<String>,
    #[serde(default)]
    pub favorite: bool,
    /// The store the user gave this item alone; it wins over the creator's.
    #[serde(default)]
    pub store: Option<Uuid>,
    /// The user's word on whether the item is a demo; unsaid, the card and
    /// the item's name tell.
    #[serde(default)]
    pub demo: Option<bool>,
    /// The part of the kit the user registered this item as.
    #[serde(default)]
    pub part: Option<Part>,
    /// Unix seconds: when the item joined the Wardrobe, last time worn.
    #[serde(default)]
    pub added: i64,
    #[serde(default)]
    pub last_worn: i64,
    /// Not looked at yet (the NEW tag).
    #[serde(default)]
    pub new: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "state")]
pub enum CardState {
    Applied {
        #[serde(default)]
        version: Option<String>,
    },
    /// Not by the items' creator: waits for the review sheet.
    Waiting,
    Ignored,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SortBy {
    #[default]
    Added,
    LastWorn,
    Name,
    Store,
    Category,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct View {
    #[serde(default)]
    pub sort: SortBy,
    /// The other way round: oldest first for the dates, Z to A.
    #[serde(default)]
    pub reversed: bool,
    /// The kit of the "Created for" chip.
    #[serde(default)]
    pub kit: Option<String>,
    /// Categories whose new dot already pulsed.
    #[serde(default)]
    pub seen_categories: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub bindings: HashMap<Uuid, Binding>,
    /// HUD items the user linked to an item, and those said not to be its.
    pub hud_links: HashMap<Uuid, Vec<Uuid>>,
    pub hud_excluded: HashMap<Uuid, Vec<Uuid>>,
    /// Objects the user already wore on a HUD attachment point.
    pub worn_huds: BTreeSet<Uuid>,
    /// The Wardrobe cards seen, by notecard item.
    pub cards: HashMap<Uuid, CardState>,
    /// Kits the user added or corrected by hand.
    pub my_kits: Vec<KnownKit>,
    pub view: View,
}

#[derive(Serialize, Deserialize)]
struct AccountFile {
    format: String,
    version: u32,
    #[serde(default)]
    bindings: BTreeMap<Uuid, Binding>,
    #[serde(default)]
    hud_links: BTreeMap<Uuid, Vec<Uuid>>,
    #[serde(default)]
    hud_excluded: BTreeMap<Uuid, Vec<Uuid>>,
    #[serde(default)]
    worn_huds: BTreeSet<Uuid>,
    #[serde(default)]
    cards: BTreeMap<Uuid, CardState>,
    #[serde(default)]
    my_kits: Vec<KnownKit>,
    #[serde(default)]
    view: View,
}

impl Account {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let file: AccountFile =
            serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes)).map_err(|e| e.to_string())?;
        if file.format != ACCOUNT_FORMAT || file.version != VERSION {
            return Err(format!("not a Wardrobe account file of version {VERSION}"));
        }
        Ok(Account {
            bindings: file.bindings.into_iter().collect(),
            hud_links: file.hud_links.into_iter().collect(),
            hud_excluded: file.hud_excluded.into_iter().collect(),
            worn_huds: file.worn_huds,
            cards: file.cards.into_iter().collect(),
            my_kits: file.my_kits,
            view: file.view,
        })
    }

    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        let file = AccountFile {
            format: ACCOUNT_FORMAT.into(),
            version: VERSION,
            bindings: self.bindings.iter().map(|(k, v)| (*k, v.clone())).collect(),
            hud_links: self.hud_links.iter().map(|(k, v)| (*k, v.clone())).collect(),
            hud_excluded: self.hud_excluded.iter().map(|(k, v)| (*k, v.clone())).collect(),
            worn_huds: self.worn_huds.clone(),
            cards: self.cards.iter().map(|(k, v)| (*k, v.clone())).collect(),
            my_kits: self.my_kits.clone(),
            view: self.view.clone(),
        };
        let mut bytes = serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

fn read<T>(path: &std::path::Path, what: &str, parse: impl FnOnce(&[u8]) -> Result<T, String>) -> Option<T> {
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > MAX_BYTES => {
            log::warn!("Cannot read the Wardrobe {what}: the file is too large");
            return None;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        _ => {}
    }
    let bytes = std::fs::read(path)
        .map_err(|error| log::warn!("Cannot read the Wardrobe {what}: {error}"))
        .ok()?;
    parse(&bytes)
        .map_err(|error| log::warn!("Cannot read the Wardrobe {what}: {error}"))
        .ok()
}

fn write(path: &std::path::Path, bytes: Result<Vec<u8>, String>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::atomic_file::write(path, &bytes.map_err(std::io::Error::other)?)
}

/// The two files of a logged-in account, each read and written by a thread
/// of its own.
pub struct Storage {
    loaded_catalogue: Receiver<Catalogue>,
    loaded_account: Receiver<Account>,
    catalogue: Option<Sender<(Catalogue, Removed)>>,
    account: Option<Sender<Account>>,
    workers: Vec<JoinHandle<()>>,
}

impl Drop for Storage {
    fn drop(&mut self) {
        self.catalogue.take();
        self.account.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl Storage {
    pub fn open(catalogue_path: PathBuf, account_path: PathBuf) -> Option<Self> {
        let (catalogue_tx, loaded_catalogue) = mpsc::unbounded();
        let (account_tx, loaded_account) = mpsc::unbounded();
        let (catalogue, catalogue_writes) = mpsc::unbounded::<(Catalogue, Removed)>();
        let (account, account_writes) = mpsc::unbounded::<Account>();
        let first = std::thread::Builder::new().name("aurora-wardrobe-catalogue".into()).spawn(move || {
            let _ = catalogue_tx.send(read(&catalogue_path, "catalogue", Catalogue::parse).unwrap_or_default());
            crate::save_retry::write_latest("Wardrobe catalogue", &catalogue_writes, |(catalogue, removed)| {
                // another account of the PC may have written since: keep
                // what it added
                let mut merged = catalogue.clone();
                if let Some(on_disk) = read(&catalogue_path, "catalogue", Catalogue::parse) {
                    merged.merge_missing(on_disk, removed);
                }
                write(&catalogue_path, merged.bytes())
            });
        });
        let second = std::thread::Builder::new().name("aurora-wardrobe-account".into()).spawn(move || {
            let _ = account_tx.send(read(&account_path, "account file", Account::parse).unwrap_or_default());
            crate::save_retry::write_latest("Wardrobe account file", &account_writes, |account| {
                write(&account_path, account.bytes())
            });
        });
        match (first, second) {
            (Ok(first), Ok(second)) => Some(Storage {
                loaded_catalogue,
                loaded_account,
                catalogue: Some(catalogue),
                account: Some(account),
                workers: vec![first, second],
            }),
            (first, second) => {
                for error in [first.err(), second.err()].into_iter().flatten() {
                    log::warn!("Cannot start the Wardrobe's storage: {error}");
                }
                None
            }
        }
    }

    pub fn take_catalogue(&self) -> Option<Catalogue> {
        self.loaded_catalogue.try_recv().ok()
    }

    pub fn take_account(&self) -> Option<Account> {
        self.loaded_account.try_recv().ok()
    }

    pub fn save_catalogue(&self, catalogue: &Catalogue, removed: &Removed) {
        if let Some(writer) = &self.catalogue {
            let _ = writer.send((catalogue.clone(), removed.clone()));
        }
    }

    pub fn save_account(&self, account: &Account) {
        if let Some(writer) = &self.account {
            let _ = writer.send(account.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalogue() -> Catalogue {
        let creator = Uuid::from_u128(1);
        let mut c = Catalogue::default();
        c.stores.insert(
            creator,
            Store {
                key: creator,
                name: "Static Bloom".into(),
                mainstore: Some("secondlife://Somewhere/1/2/3".into()),
                ..Store::default()
            },
        );
        c.packs.insert(
            "p".into(),
            Pack {
                key: "p".into(),
                store: creator,
                name: "Night market set".into(),
                version: Some("1.2".into()),
                ..Pack::default()
            },
        );
        c.products.insert(
            "p/tee".into(),
            Product {
                key: "p/tee".into(),
                pack: "p".into(),
                name: "Oversized band tee".into(),
                categories: vec![Tag("tops".into(), vec!["t-shirt".into(), "crop-top".into()])],
                fits: Some(vec!["Aster".into()]),
                trusted: true,
                ..Product::default()
            },
        );
        c.kits_known.push(KnownKit {
            name: "Aster".into(),
            part: Some(Part::Body),
            ..KnownKit::default()
        });
        c
    }

    #[test]
    fn the_catalogue_reads_back_what_it_wrote() {
        let c = catalogue();
        let bytes = c.bytes().expect("bytes");
        let text = String::from_utf8(bytes.clone()).expect("utf-8");
        assert!(text.contains("\"format\": \"aurora-wardrobe-catalogue\""));
        assert!(text.contains("\"t-shirt\""));
        assert_eq!(Catalogue::parse(&bytes), Ok(c));
        assert!(Catalogue::parse(b"{\"format\":\"other\",\"version\":1}").is_err());
        assert!(Catalogue::parse(b"{\"format\":\"aurora-wardrobe-catalogue\",\"version\":2}").is_err());
        assert!(Catalogue::parse(b"[]").is_err());
    }

    #[test]
    fn the_account_file_reads_back_what_it_wrote() {
        let mut a = Account::default();
        a.bindings.insert(
            Uuid::from_u128(5),
            Binding {
                product: Some("p/tee".into()),
                categories: Some(vec![Tag::new("tops", "shirt")]),
                favorite: true,
                added: 1_700_000_000,
                new: true,
                ..Binding::default()
            },
        );
        a.cards.insert(
            Uuid::from_u128(6),
            CardState::Applied {
                version: Some("1.2".into()),
            },
        );
        a.cards.insert(Uuid::from_u128(7), CardState::Ignored);
        a.view.sort = SortBy::LastWorn;
        a.view.kit = Some("Aster".into());
        let bytes = a.bytes().expect("bytes");
        assert_eq!(Account::parse(&bytes), Ok(a));
        assert!(Account::parse(b"{\"format\":\"aurora-wardrobe-catalogue\",\"version\":1}").is_err());
    }

    #[test]
    fn another_accounts_records_are_kept_and_deleted_ones_stay_deleted() {
        let mut ours = catalogue();
        let mut theirs = catalogue();
        theirs.products.get_mut("p/tee").expect("tee").name = "Theirs".into();
        theirs.products.insert(
            "p/new".into(),
            Product {
                key: "p/new".into(),
                ..Product::default()
            },
        );
        theirs.products.insert(
            "p/gone".into(),
            Product {
                key: "p/gone".into(),
                ..Product::default()
            },
        );
        let removed = Removed {
            products: ["p/gone".to_owned()].into(),
            ..Removed::default()
        };
        ours.merge_missing(theirs, &removed);
        assert_eq!(ours.products["p/tee"].name, "Oversized band tee");
        assert!(ours.products.contains_key("p/new"));
        assert!(!ours.products.contains_key("p/gone"));
        assert_eq!(ours.kits_known.len(), 1);
    }
}
