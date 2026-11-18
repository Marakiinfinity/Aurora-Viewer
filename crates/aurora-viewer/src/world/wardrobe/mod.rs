//! The Wardrobe: a local index of the clothing, bodies and accessories of
//! an account, with what the inventory does not hold (the store, what an
//! item was created for, its category, its HUDs, its pack and event). The
//! data comes from Wardrobe cards (plain notecards) and from the user, and
//! never leaves the PC. Design and rules: docs/WARDROBE.md. Aurora's own
//! feature; the Legacy Viewer has no equivalent.

pub mod card;
pub mod categories;
pub mod demo;
pub mod fit;
pub mod huds;
pub mod matching;
pub mod scan;
pub mod sort;
pub mod store;

use categories::{Part, Tag};
use std::collections::HashMap;
use std::path::PathBuf;
use store::{Account, Binding, CardState, Catalogue, KnownKit, Pack, Product, Removed, Storage, Store};
use uuid::Uuid;

/// Changes that can be undone, the newest last.
const UNDO_DEPTH: usize = 16;

/// Unix seconds.
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// What one change replaced, to put it back.
#[derive(Default)]
struct Undo {
    stores: Vec<(Uuid, Option<Store>)>,
    packs: Vec<(String, Option<Pack>)>,
    products: Vec<(String, Option<Product>)>,
    bindings: Vec<(Uuid, Option<Binding>)>,
    cards: Vec<(Uuid, Option<CardState>)>,
    kits_known: Option<Vec<KnownKit>>,
}

impl Undo {
    fn is_empty(&self) -> bool {
        self.stores.is_empty()
            && self.packs.is_empty()
            && self.products.is_empty()
            && self.bindings.is_empty()
            && self.cards.is_empty()
            && self.kits_known.is_none()
    }
}

#[derive(Default)]
pub struct Wardrobe {
    pub catalogue: Catalogue,
    pub account: Account,
    removed: Removed,
    storage: Option<Storage>,
    /// Both files are read (always true in the demo and the tests).
    catalogue_read: bool,
    account_read: bool,
    /// Bumped by every change: the cached views rebuild.
    pub revision: u64,
    /// Since the login only: an undo never outlives the session.
    undo: Vec<Undo>,
}

/// One change of the Wardrobe: what it touches is remembered so that the
/// whole change can be undone.
pub struct Txn<'a> {
    catalogue: &'a mut Catalogue,
    account: &'a mut Account,
    removed: &'a mut Removed,
    undo: Undo,
    catalogue_changed: bool,
    account_changed: bool,
}

impl Txn<'_> {
    pub fn catalogue(&self) -> &Catalogue {
        self.catalogue
    }

    pub fn account(&self) -> &Account {
        self.account
    }

    pub fn set_store(&mut self, store: Store) {
        // a store is someone's: never the unknown creator's
        if store.key.is_nil() {
            return;
        }
        let before = self.catalogue.stores.insert(store.key, store.clone());
        self.removed.stores.remove(&store.key);
        if !self.undo.stores.iter().any(|(k, _)| *k == store.key) {
            self.undo.stores.push((store.key, before));
        }
        self.catalogue_changed = true;
    }

    pub fn remove_store(&mut self, key: Uuid) {
        if let Some(before) = self.catalogue.stores.remove(&key) {
            self.removed.stores.insert(key);
            if !self.undo.stores.iter().any(|(k, _)| *k == key) {
                self.undo.stores.push((key, Some(before)));
            }
            self.catalogue_changed = true;
        }
    }

    pub fn set_pack(&mut self, pack: Pack) {
        let before = self.catalogue.packs.insert(pack.key.clone(), pack.clone());
        self.removed.packs.remove(&pack.key);
        if !self.undo.packs.iter().any(|(k, _)| *k == pack.key) {
            self.undo.packs.push((pack.key, before));
        }
        self.catalogue_changed = true;
    }

    pub fn set_product(&mut self, product: Product) {
        let before = self.catalogue.products.insert(product.key.clone(), product.clone());
        self.removed.products.remove(&product.key);
        if !self.undo.products.iter().any(|(k, _)| *k == product.key) {
            self.undo.products.push((product.key, before));
        }
        self.catalogue_changed = true;
    }

    /// Adds a kit to the alias table, or completes the one of that name and
    /// part with new patterns and sizes.
    pub fn learn_kit(&mut self, kit: KnownKit) {
        if self.undo.kits_known.is_none() {
            self.undo.kits_known = Some(self.catalogue.kits_known.clone());
        }
        match self
            .catalogue
            .kits_known
            .iter_mut()
            .find(|k| k.name.eq_ignore_ascii_case(&kit.name) && k.part == kit.part && k.creator == kit.creator)
        {
            Some(known) => {
                for pattern in kit.patterns {
                    if !known.patterns.contains(&pattern) {
                        known.patterns.push(pattern);
                    }
                }
                for size in kit.sizes {
                    if !known.sizes.iter().any(|s| s.eq_ignore_ascii_case(&size)) {
                        known.sizes.push(size);
                    }
                }
            }
            None => self.catalogue.kits_known.push(kit),
        }
        self.catalogue_changed = true;
    }

    /// The binding of an item, created when the item was not in the
    /// Wardrobe.
    pub fn binding(&mut self, item: Uuid) -> &mut Binding {
        if !self.undo.bindings.iter().any(|(k, _)| *k == item) {
            self.undo.bindings.push((item, self.account.bindings.get(&item).cloned()));
        }
        self.account_changed = true;
        self.account.bindings.entry(item).or_default()
    }

    /// Removes an item from the Wardrobe: its binding and the user's values,
    /// never the item.
    pub fn remove_binding(&mut self, item: Uuid) {
        if let Some(before) = self.account.bindings.remove(&item) {
            if !self.undo.bindings.iter().any(|(k, _)| *k == item) {
                self.undo.bindings.push((item, Some(before)));
            }
            self.account_changed = true;
        }
    }

    pub fn set_card(&mut self, card: Uuid, state: CardState) {
        let before = self.account.cards.insert(card, state);
        if !self.undo.cards.iter().any(|(k, _)| *k == card) {
            self.undo.cards.push((card, before));
        }
        self.account_changed = true;
    }

    /// The rest of the account file (HUD links, kits, view): changed in
    /// place, not undone.
    pub fn account_mut(&mut self) -> &mut Account {
        self.account_changed = true;
        self.account
    }
}

/// An item of the Wardrobe with what the catalogue says of it. Every value
/// follows the precedence of section 6: the user's own, then the product
/// record (the creator's card, else a card the user applied), then what
/// Auto-sort found.
#[derive(Clone, Copy)]
pub struct Entry<'a> {
    pub binding: &'a Binding,
    pub product: Option<&'a Product>,
    pub pack: Option<&'a Pack>,
    pub store: Option<&'a Store>,
}

impl<'a> Entry<'a> {
    pub fn categories(&self) -> &'a [Tag] {
        if let Some(own) = &self.binding.categories {
            return own;
        }
        match self.product {
            Some(product) if !product.categories.is_empty() => &product.categories,
            _ => &self.binding.sorted,
        }
    }

    /// The main category: the first.
    pub fn main(&self) -> Option<&'a Tag> {
        self.categories().first()
    }

    /// The kits the item was created for; empty: everyone.
    pub fn fits(&self) -> &'a [String] {
        match (&self.binding.fits, self.product.and_then(|p| p.fits.as_ref())) {
            (Some(own), _) => own,
            (None, Some(card)) => card,
            (None, None) => &[],
        }
    }

    /// The kit names the item provides to others.
    pub fn kit(&self) -> &'a [String] {
        match (&self.binding.kit, self.product) {
            (Some(own), _) => own,
            (None, Some(product)) => &product.kit,
            (None, None) => &[],
        }
    }

    pub fn part(&self) -> Part {
        match self.product.and_then(|p| p.part) {
            Some(part) if self.binding.categories.is_none() => part,
            _ => Part::of(self.main()),
        }
    }

    pub fn note(&self) -> Option<&'a str> {
        self.binding
            .note
            .as_deref()
            .or_else(|| self.product.and_then(|p| p.note.as_deref()))
    }

    pub fn event(&self) -> Option<&'a str> {
        self.product
            .and_then(|p| p.event.as_deref())
            .or_else(|| self.pack.and_then(|p| p.event.as_deref()))
    }

    pub fn version(&self) -> Option<&'a str> {
        self.product
            .and_then(|p| p.version.as_deref())
            .or_else(|| self.pack.and_then(|p| p.version.as_deref()))
    }

    pub fn demo(&self) -> bool {
        self.product
            .and_then(|p| p.demo)
            .or_else(|| self.pack.map(|p| p.demo))
            .unwrap_or(false)
    }
}

impl Wardrobe {
    /// The files of a logged-in account; the demo and the tests keep
    /// everything in memory (`Default`).
    pub fn load(catalogue: PathBuf, account: PathBuf) -> Self {
        let storage = Storage::open(catalogue, account);
        let unread = storage.is_some();
        Wardrobe {
            storage,
            catalogue_read: !unread,
            account_read: !unread,
            ..Wardrobe::default()
        }
    }

    /// In memory, as the demo: nothing is read or written.
    pub fn in_memory() -> Self {
        Wardrobe {
            catalogue_read: true,
            account_read: true,
            ..Wardrobe::default()
        }
    }

    /// Takes the files once they are read. What changed before the read
    /// stays ahead of it.
    pub fn poll(&mut self) {
        let Some(storage) = &self.storage else { return };
        if !self.catalogue_read
            && let Some(read) = storage.take_catalogue()
        {
            self.catalogue.merge_missing(read, &self.removed);
            self.catalogue_read = true;
            self.revision += 1;
        }
        if !self.account_read
            && let Some(read) = storage.take_account()
        {
            let early = std::mem::replace(&mut self.account, read);
            self.account.bindings.extend(early.bindings);
            self.account.cards.extend(early.cards);
            self.account.hud_links.extend(early.hud_links);
            self.account.hud_excluded.extend(early.hud_excluded);
            self.account.worn_huds.extend(early.worn_huds);
            self.account.my_kits.extend(early.my_kits);
            self.account_read = true;
            self.revision += 1;
        }
    }

    /// Both files are read: before that, the Wardrobe shows as loading
    /// rather than empty.
    pub fn ready(&self) -> bool {
        self.catalogue_read && self.account_read
    }

    /// Runs one change and saves what it touched. `undoable`: the change
    /// joins the undo stack (a card, Auto-sort, a removal), where a small
    /// edit (a favourite, a seen tag) does not.
    pub fn change<R>(&mut self, undoable: bool, f: impl FnOnce(&mut Txn) -> R) -> R {
        let mut txn = Txn {
            catalogue: &mut self.catalogue,
            account: &mut self.account,
            removed: &mut self.removed,
            undo: Undo::default(),
            catalogue_changed: false,
            account_changed: false,
        };
        let result = f(&mut txn);
        let (undo, catalogue_changed, account_changed) = (txn.undo, txn.catalogue_changed, txn.account_changed);
        if undoable && !undo.is_empty() {
            self.undo.push(undo);
            if self.undo.len() > UNDO_DEPTH {
                self.undo.remove(0);
            }
        }
        self.saved(catalogue_changed, account_changed);
        result
    }

    fn saved(&mut self, catalogue: bool, account: bool) {
        if !catalogue && !account {
            return;
        }
        self.revision += 1;
        if let Some(storage) = &self.storage {
            if catalogue {
                storage.save_catalogue(&self.catalogue, &self.removed);
            }
            if account {
                storage.save_account(&self.account);
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Puts back what the last undoable change replaced. What the user
    /// changed since on other records stays.
    pub fn undo(&mut self) -> bool {
        let Some(undo) = self.undo.pop() else { return false };
        let catalogue = !undo.stores.is_empty() || !undo.packs.is_empty() || !undo.products.is_empty() || undo.kits_known.is_some();
        let account = !undo.bindings.is_empty() || !undo.cards.is_empty();
        for (key, before) in undo.stores {
            match before {
                Some(store) => {
                    self.removed.stores.remove(&key);
                    self.catalogue.stores.insert(key, store);
                }
                None => {
                    self.catalogue.stores.remove(&key);
                    self.removed.stores.insert(key);
                }
            }
        }
        for (key, before) in undo.packs {
            match before {
                Some(pack) => {
                    self.removed.packs.remove(&key);
                    self.catalogue.packs.insert(key, pack);
                }
                None => {
                    self.catalogue.packs.remove(&key);
                    self.removed.packs.insert(key);
                }
            }
        }
        for (key, before) in undo.products {
            match before {
                Some(product) => {
                    self.removed.products.remove(&key);
                    self.catalogue.products.insert(key, product);
                }
                None => {
                    self.catalogue.products.remove(&key);
                    self.removed.products.insert(key);
                }
            }
        }
        if let Some(kits) = undo.kits_known {
            self.catalogue.kits_known = kits;
        }
        for (key, before) in undo.bindings {
            match before {
                Some(binding) => self.account.bindings.insert(key, binding),
                None => self.account.bindings.remove(&key),
            };
        }
        for (key, before) in undo.cards {
            match before {
                Some(state) => self.account.cards.insert(key, state),
                None => self.account.cards.remove(&key),
            };
        }
        self.saved(catalogue, account);
        true
    }

    pub fn contains(&self, item: &Uuid) -> bool {
        self.account.bindings.contains_key(item)
    }

    /// An item of the Wardrobe with its product, pack and store.
    pub fn entry(&self, item: Uuid) -> Option<Entry<'_>> {
        let binding = self.account.bindings.get(&item)?;
        let product = binding.product.as_ref().and_then(|key| self.catalogue.products.get(key));
        let pack = product.and_then(|p| self.catalogue.packs.get(&p.pack));
        let store = pack.and_then(|p| self.catalogue.stores.get(&p.store));
        Some(Entry {
            binding,
            product,
            pack,
            store,
        })
    }

    /// The store of an item: its pack's, else the one known for its
    /// creator (a card of another pack, or the user's link).
    pub fn store_of<'a>(&'a self, entry: Option<&Entry<'a>>, creator: Uuid) -> Option<&'a Store> {
        // an item whose creator is not known yet has no store of its own:
        // no record may stand for "nobody"
        entry
            .and_then(|e| e.store)
            .or_else(|| self.catalogue.stores.get(&creator).filter(|_| !creator.is_nil()))
    }

    /// The copies of each product in this account: product key → items.
    #[cfg(test)]
    pub fn copies(&self) -> HashMap<&str, Vec<Uuid>> {
        let mut copies: HashMap<&str, Vec<Uuid>> = HashMap::new();
        for (item, binding) in &self.account.bindings {
            if let Some(key) = &binding.product {
                copies.entry(key.as_str()).or_default().push(*item);
            }
        }
        copies
    }
}

/// The Wardrobe of the logged-in account and its index, shared by every
/// window that shows it, like the saved smart searches
/// (`ui/inventory/smart_store.rs`).
#[derive(Default)]
pub struct Shared {
    pub wardrobe: Wardrobe,
    pub index: scan::Index,
    /// The Wardrobe cards read this session, by notecard item.
    pub cards: HashMap<Uuid, Loaded>,
    /// Where the account keeps the pictures the user chose; `None` in the
    /// demo, which keeps them in memory.
    pub pictures: Option<PathBuf>,
}

/// The account's folder of chosen pictures.
pub const PICTURES_FOLDER: &str = "wardrobe-pictures";

/// A Wardrobe card read from its notecard.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub card: card::Card,
    /// The notecard's creator: who wrote the card.
    pub creator: Uuid,
    /// The items the notecard embeds (name, asset id).
    pub embedded: Vec<(String, Uuid)>,
}

static SHARED: std::sync::Mutex<Option<Shared>> = std::sync::Mutex::new(None);

/// Runs `f` on the account's Wardrobe. Never call it from inside itself.
pub fn with<R>(f: impl FnOnce(&mut Shared) -> R) -> R {
    let mut guard = SHARED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    f(guard.get_or_insert_with(|| Shared {
        wardrobe: Wardrobe::in_memory(),
        ..Shared::default()
    }))
}

/// Opens the files of the account that logs in; replaces the Wardrobe of a
/// previous account. `None`: in memory (the demo).
pub fn open(files: Option<(PathBuf, PathBuf)>) {
    // the pictures the user chose, next to the account's file
    let pictures = files
        .as_ref()
        .and_then(|(_, account)| account.parent())
        .map(|folder| folder.join(PICTURES_FOLDER));
    let wardrobe = match files {
        Some((catalogue, account)) => Wardrobe::load(catalogue, account),
        None => Wardrobe::in_memory(),
    };
    let mut guard = SHARED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // the previous account's storage writes what it still owes, then goes
    *guard = Some(Shared {
        wardrobe,
        pictures,
        ..Shared::default()
    });
}

#[cfg(test)]
mod tests;
