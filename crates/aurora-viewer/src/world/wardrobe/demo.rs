//! The offline Wardrobe (`AURORA_DEMO_WARDROBE=cards|browse|huds|fit|creator`,
//! docs/TESTING.md): folders of made-up products with their Wardrobe
//! cards, a few stores, kits and HUDs, to show the Wardrobe mode without a
//! grid. The demo's items are its own and belong to no one.

use super::matching::{self, Groups};
use super::store::{Source, Store};
use super::{Loaded, Shared, card};
use crate::world::inventory::{FetchState, Folder, Inventory};
use aurora_net::inventory::{InvFolder, InvItem};
use uuid::Uuid;

/// The made-up creators: a store with a card, a resident without a store,
/// one whose store the user linked, and a stranger who writes cards.
pub const BLOOM: Uuid = Uuid::from_u128(0xD0_01);
pub const KIKO: Uuid = Uuid::from_u128(0xD0_02);
pub const NORTH: Uuid = Uuid::from_u128(0xD0_03);
pub const STRANGER: Uuid = Uuid::from_u128(0xD0_04);

/// Their names, as the name service would answer.
pub fn names() -> Vec<(Uuid, String, String)> {
    vec![
        (BLOOM, "Static".into(), "Bloom".into()),
        (KIKO, "Kiko".into(), "Makes".into()),
        (NORTH, "North".into(), "Atelier".into()),
        (STRANGER, "Passing".into(), "Reseller".into()),
    ]
}

/// What the scenario placed: the cards to read (notecard item, its text),
/// the items to show as worn, and the folder to open.
#[derive(Default)]
pub struct Seeded {
    pub cards: Vec<(Uuid, String)>,
    pub worn: Vec<Uuid>,
    /// `creator`: the folder of the agent's own product, for the creator
    /// tool.
    pub release: Option<Uuid>,
}

struct Builder<'a> {
    inv: &'a mut Inventory,
    agent: Uuid,
    next: u128,
    root: Uuid,
}

impl Builder<'_> {
    fn id(&mut self) -> Uuid {
        self.next += 1;
        Uuid::from_u128(self.next)
    }

    fn folder(&mut self, parent: Uuid, name: &str) -> Uuid {
        let id = self.id();
        self.inv.folders.insert(
            id,
            Folder {
                info: InvFolder {
                    id,
                    parent,
                    name: name.into(),
                    type_default: -1,
                    version: 1,
                    ..Default::default()
                },
                children: Vec::new(),
                items: Vec::new(),
                state: FetchState::Fetched,
                library: false,
            },
        );
        if let Some(parent) = self.inv.folders.get_mut(&parent) {
            parent.children.push(id);
        }
        id
    }

    /// `kind`: the asset type; `flags`: the wearable type of a system layer.
    fn item(&mut self, folder: Uuid, name: &str, kind: i32, flags: u32, creator: Uuid) -> Uuid {
        let id = self.id();
        let inv_type = match kind {
            0 => 0,
            5 | 13 => 18,
            7 => 7,
            _ => 6,
        };
        // no transfer, like most products: no asset id of them is shown
        let rights = if kind == 7 || kind == 0 {
            0x7fff_ffff
        } else {
            0x7fff_ffff & !(1 << 13)
        };
        self.inv.items.insert(
            id,
            InvItem {
                id,
                parent: folder,
                name: name.into(),
                asset_type: kind,
                inv_type,
                flags,
                asset_id: Uuid::from_u128(id.as_u128() + 0x5_0000),
                creator,
                owner: self.agent,
                base_mask: rights,
                owner_mask: rights,
                next_owner_mask: rights,
                created_at: 1_790_000_000 - (self.next as i64 % 97) * 86_400,
                ..Default::default()
            },
        );
        if let Some(folder) = self.inv.folders.get_mut(&folder) {
            folder.items.push(id);
        }
        id
    }

    /// A product box: its objects, and its card when it has one.
    fn pack(&mut self, name: &str, creator: Uuid, items: &[&str], card: Option<(&str, Uuid)>, out: &mut Seeded) -> (Uuid, Vec<Uuid>) {
        let folder = self.folder(self.root, name);
        let ids: Vec<Uuid> = items.iter().map(|item| self.item(folder, item, 6, 0, creator)).collect();
        if let Some((text, writer)) = card {
            let note = self.item(folder, &format!("# Aurora Wardrobe - {name}"), 7, 0, writer);
            out.cards.push((note, text.to_owned()));
        }
        (folder, ids)
    }
}

const NIGHT_MARKET: &str = "# Aurora Wardrobe 1
[store]
name        = Static Bloom
marketplace = https://marketplace.secondlife.com/stores/12345
mainstore   = secondlife://Aurora Plaza/128/128/25

[pack]
name    = Night market set
version = 1.2
event   = Moonlit Market

[items]
Oversized band tee = Tops/T-shirt   fits Aster, Nova   note Rigged, 6 colours
Band tee HUD       = hud-for: Oversized band tee
Pleated skirt      = Bottoms/Skirt   fits Aster, Nova
Market boots       = Shoes/Boots   fits Aster
Market boots HUD   = hud-for: Market boots
Rib tattoo         = Body parts/Tattoo   fits everyone   version 1.3   note Faded ink, BOM
Moon ear cuffs     = Accessories/Earrings   fits Fae ears
";

const ASTER: &str = "# Aurora Wardrobe 1
[store]
name = Static Bloom

[pack]
name    = Aster body
version = 5.3

[items]
Aster Body     = Body parts/Mesh body   kit Aster, Aster Petite
Aster Body HUD = hud-for: Aster Body
Aster hands    = Body parts/Hands   fits Aster
Aster feet     = Body parts/Feet   fits Aster
";

const SORA: &str = "# Aurora Wardrobe 1
[pack]
name    = Sora head
version = 3.1

[items]
Sora head            = Heads/Mesh head   kit Sora
Sora head HUD        = hud-for: Sora head
Sora head HUD Emotes = hud-for: Sora head
Sora brows           = Heads/Brows   fits Sora
Fae ears             = Body parts/Ears   kit Fae ears   fits Sora, Lumen
Fox tail             = Body parts/Tail   kit Fox tail   note Fluffy
";

const NOVA: &str = "# Aurora Wardrobe 1
[pack]
name  = Nova street pack
event = Harbor Weekend

[items]
Nova hoodie    = Tops/Hoodie   fits Nova
Nova joggers   = Bottoms/Trousers   fits Nova
Nova sneakers  = Shoes/Sneakers   fits Nova, Aster
Chain necklace = Accessories/Necklace   fits everyone
Photo radar    = HUDs/Camera and photo+Utility
";

const RESOLD: &str = "# Aurora Wardrobe 1
[store]
name = Not my store

[pack]
name = Vintage finds

[items]
Linen shirt   = Tops/Shirt
Canvas bag    = Accessories/Bag
Missing scarf = Accessories/Scarf
Rain coat     = Outerwear/Coat
";

/// Seeds the inventory for `scenario` under a "Wardrobe demo" folder.
pub fn seed(inv: &mut Inventory, agent: Uuid, scenario: &str) -> Seeded {
    let mut out = Seeded::default();
    let root_parent = inv.root;
    let mut b = Builder {
        inv,
        agent,
        next: 0xD1_0000,
        root: Uuid::nil(),
    };
    b.root = b.folder(root_parent, "Wardrobe demo");
    // the kits: a body, a head, ears and a tail
    let (_, aster) = b.pack(
        "Aster body 5.3",
        BLOOM,
        &["Aster Body", "Aster Body HUD", "Aster hands", "Aster feet"],
        Some((ASTER, BLOOM)),
        &mut out,
    );
    let (sora_folder, sora) = b.pack(
        "Sora head 3.1",
        KIKO,
        &[
            "Sora head",
            "Sora head HUD",
            "Sora head HUD Emotes",
            "Sora brows",
            "Fae ears",
            "Fox tail",
        ],
        Some((SORA, KIKO)),
        &mut out,
    );
    // edited copies of the tail: one product, three copies
    b.item(sora_folder, "Fox tail 2", 6, 0, KIKO);
    b.item(sora_folder, "Fox tail (short)", 6, 0, KIKO);
    let (_, market) = b.pack(
        "Night market set",
        BLOOM,
        &[
            "Oversized band tee",
            "Band tee HUD",
            "Pleated skirt",
            "Market boots",
            "Market boots HUD",
            "Rib tattoo",
            "Moon ear cuffs",
        ],
        Some((NIGHT_MARKET, BLOOM)),
        &mut out,
    );
    b.pack(
        "Nova street pack",
        NORTH,
        &["Nova hoodie", "Nova joggers", "Nova sneakers", "Chain necklace", "Photo radar"],
        Some((NOVA, NORTH)),
        &mut out,
    );
    // a card by someone who is not the items' creator, with a line that
    // names nothing and a category the list does not have
    b.pack(
        "Vintage finds",
        KIKO,
        &["Linen shirt", "Canvas bag", "Rain coat"],
        Some((RESOLD, STRANGER)),
        &mut out,
    );
    // no card at all: what Auto-sort can place, and what it cannot
    let loose = b.folder(b.root, "Old favourites");
    let layers = [
        ("Plain white shirt", 5, 4),
        ("Blue jeans layer", 5, 5),
        ("Freckles", 5, 14),
        ("My shape", 13, 0),
        ("Green eyes", 13, 3),
    ];
    let mut worn_layers = Vec::new();
    for (name, kind, flags) in layers {
        worn_layers.push(b.item(loose, name, kind, flags, agent));
    }
    for name in [
        "Summer dress - red",
        "Leather jacket",
        "Wide brim hat",
        "Round glasses",
        "Mystery object",
        "Gift from a friend",
    ] {
        b.item(loose, name, 6, 0, KIKO);
    }
    out.worn = vec![aster[0], sora[0], market[0], market[3], worn_layers[3], worn_layers[4]];
    if scenario == "creator" {
        // a product the agent made, with its HUD and a logo of their own
        let release = b.folder(b.root, "Lumen release");
        for name in ["Lumen dress", "Lumen dress HUD", "Lumen heels", "Lumen clutch"] {
            b.item(release, name, 6, 0, agent);
        }
        b.item(release, "Lumen logo", 0, 0, agent);
        out.release = Some(release);
    }
    if scenario == "fit" {
        // a second body, not worn: what was made for it shows as not for
        // what is worn
        out.worn.retain(|id| *id != market[3]);
    }
    if scenario == "huds" {
        out.worn.push(sora[1]);
    }
    out
}

/// Applies the scenario's cards the way a login would have found them
/// (`browse`, `huds`, `fit`), and links a store by hand. The `cards`
/// scenario leaves them to the app, which reads them one by one with
/// their toasts.
pub fn apply(shared: &mut Shared, inv: &Inventory, seeded: &Seeded) {
    for (note, text) in &seeded.cards {
        let (Some(item), Some(parsed)) = (inv.items.get(note), card::parse(text)) else {
            continue;
        };
        let folder_name = inv.folders.get(&item.parent).map(|f| f.info.name.clone()).unwrap_or_default();
        let scope = super::scan::card_scope(inv, item.parent);
        let plan = matching::plan(&parsed, *note, item.creator, &folder_name, &scope, &shared.wardrobe.catalogue);
        shared.cards.insert(
            *note,
            Loaded {
                card: parsed.clone(),
                creator: item.creator,
                embedded: Vec::new(),
            },
        );
        if plan.trusted {
            shared
                .wardrobe
                .change(false, |txn| matching::apply(txn, &parsed, &plan, Groups::ALL, &[]));
        } else {
            shared
                .wardrobe
                .change(false, |txn| txn.set_card(*note, super::store::CardState::Waiting));
        }
    }
    shared.wardrobe.change(false, |txn| {
        txn.set_store(Store {
            key: NORTH,
            name: "North Atelier".into(),
            marketplace: Some("https://marketplace.secondlife.com/stores/67890".into()),
            source: Source::User,
            ..Store::default()
        });
        // a few are favourites, and not everything is new
        let ids: Vec<Uuid> = txn.account().bindings.keys().copied().collect();
        for (n, id) in ids.iter().enumerate() {
            let binding = txn.binding(*id);
            binding.new = n % 4 == 0;
            binding.favorite = n % 5 == 0;
            binding.last_worn = 1_790_000_000 - n as i64 * 3600;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::wardrobe::fit::Fit;
    use std::collections::HashSet;

    /// The whole chain on the demo's inventory: cards, copies, HUDs, kits
    /// and fit, as the Wardrobe mode shows them.
    #[test]
    fn the_demo_fills_a_wardrobe() {
        let agent = Uuid::from_u128(1);
        let mut inv = Inventory {
            owner: agent,
            root: Uuid::from_u128(2),
            ..Inventory::default()
        };
        inv.folders.insert(
            inv.root,
            Folder {
                info: InvFolder {
                    id: inv.root,
                    ..Default::default()
                },
                children: Vec::new(),
                items: Vec::new(),
                state: FetchState::Fetched,
                library: false,
            },
        );
        let seeded = seed(&mut inv, agent, "browse");
        assert_eq!(seeded.cards.len(), 5);
        let mut shared = Shared {
            wardrobe: super::super::Wardrobe::in_memory(),
            ..Shared::default()
        };
        apply(&mut shared, &inv, &seeded);
        let worn: HashSet<Uuid> = seeded.worn.iter().copied().collect();
        assert!(shared.index.update(&shared.wardrobe, &inv, &worn));
        let named = |name: &str| inv.items.values().find(|it| it.name == name).map(|it| it.id).expect(name);
        // the stranger's card applied nothing; the creators' did
        assert!(!shared.wardrobe.contains(&named("Linen shirt")));
        assert!(shared.wardrobe.contains(&named("Oversized band tee")));
        // HUDs belong to their items and are no cards of their own
        assert_eq!(shared.index.huds_of(&named("Sora head")).len(), 2);
        assert!(!shared.index.items.contains(&named("Band tee HUD")));
        assert!(shared.index.own_huds.contains(&named("Market boots HUD")));
        // the three tails are one product
        let tails = ["Fox tail", "Fox tail 2", "Fox tail (short)"]
            .map(|name| shared.wardrobe.entry(named(name)).and_then(|e| e.product.map(|p| p.key.clone())));
        assert!(tails[0].is_some() && tails[0] == tails[1] && tails[1] == tails[2]);
        // fit, per part: the body and the head are worn, the ears are not
        assert_eq!(shared.index.fit_of(&named("Pleated skirt")), &Fit::Fits("Aster".into()));
        assert_eq!(shared.index.fit_of(&named("Fae ears")), &Fit::Fits("Sora".into()));
        assert!(matches!(
            shared.index.fit_of(&named("Moon ear cuffs")),
            Fit::NotFor { nothing_worn: true, .. }
        ));
        assert!(matches!(shared.index.fit_of(&named("Nova hoodie")), Fit::Unknown(_)));
        assert_eq!(shared.index.fit_of(&named("Nova sneakers")), &Fit::Fits("Aster".into()));
        // Auto-sort places the layers and the clear names, not the rest
        let sortable: HashSet<Uuid> = shared.index.sortable.iter().map(|(id, _)| *id).collect();
        assert!(sortable.contains(&named("Plain white shirt")) && sortable.contains(&named("Summer dress - red")));
        assert!(shared.index.unsorted.contains(&named("Mystery object")));
    }
}
