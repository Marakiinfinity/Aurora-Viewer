//! Cards applied to a folder: matching, trust, precedence, copies, undo
//! (docs/WARDROBE.md, sections 6 and 7).

use super::card::{self, Card};
use super::categories::{ASSET_OBJECT, OTHER, Part, Tag};
use super::matching::{self, Groups, Plan, fold, hud_stem, slug};
use super::store::{CardState, Source, Store};
use super::*;
use aurora_net::inventory::InvItem;

const CREATOR: Uuid = Uuid::from_u128(0xC0);
const STRANGER: Uuid = Uuid::from_u128(0x57);
const CARD: Uuid = Uuid::from_u128(0xCA);

const TEXT: &str = "# Aurora Wardrobe 1
[store]
name = Static Bloom
marketplace = https://marketplace.secondlife.com/stores/1

[pack]
name = Night market set
version = 1.2
event = Moonlit Market

[items]
Oversized band tee = Tops/T-shirt   fits Aster, Nova   picture tee-icon
Band tee HUD = hud-for: Oversized band tee
Fox tail = Body parts/Tail   kit Fox tail   note Fluffy
Ghost = Tops
";

fn object(id: u128, name: &str, creator: Uuid) -> InvItem {
    InvItem {
        id: Uuid::from_u128(id),
        name: name.into(),
        asset_type: ASSET_OBJECT,
        creator,
        asset_id: Uuid::from_u128(id + 0x1000),
        ..InvItem::default()
    }
}

fn folder(creator: Uuid) -> Vec<InvItem> {
    vec![
        object(1, "Oversized Band Tee v1.2", creator),
        object(2, "Band tee HUD", creator),
        object(3, "Fox tail", creator),
        object(4, "Fox tail 2", creator),
        object(5, "Fox tail (short)", creator),
        object(6, "Unrelated thing", creator),
        InvItem {
            id: Uuid::from_u128(7),
            name: "tee-icon".into(),
            asset_type: 0,
            creator,
            ..InvItem::default()
        },
        InvItem {
            id: Uuid::from_u128(8),
            name: "Fox tail".into(),
            asset_type: 24,
            creator,
            ..InvItem::default()
        },
    ]
}

fn planned(wardrobe: &Wardrobe, text: &str, card_creator: Uuid, items: &[InvItem]) -> (Card, Plan) {
    let card = card::parse(text).expect("a card");
    let refs: Vec<&InvItem> = items.iter().collect();
    let plan = matching::plan(&card, CARD, card_creator, "Delivered folder", &refs, &wardrobe.catalogue);
    (card, plan)
}

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

#[test]
fn names_fold_to_tell_copies() {
    assert_eq!(fold("Fox Tail (short) v2.1"), "fox tail");
    assert_eq!(fold("Fox tail 2"), "fox tail");
    assert_eq!(fold("  [Static Bloom] Oversized-Band_Tee  "), "oversized band tee");
    assert_eq!(fold("[HUD]"), "hud");
    assert_eq!(fold("2024"), "2024");
    assert_eq!(hud_stem("Sora head [HUD]").as_deref(), Some("sora head"));
    assert_eq!(hud_stem("Sora head"), None);
    assert_eq!(hud_stem("HUD"), None);
    assert_eq!(slug("Night market set v1.2"), "night-market-set");
    assert_eq!(slug("!!!"), "unnamed");
}

#[test]
fn a_card_binds_the_items_of_its_folder_and_their_copies() {
    let wardrobe = Wardrobe::in_memory();
    let (card, plan) = planned(&wardrobe, TEXT, CREATOR, &folder(CREATOR));
    assert!(plan.trusted);
    assert_eq!(plan.items_creator, Some(CREATOR));
    assert_eq!(plan.pack_key, format!("{CREATOR}/night-market-set"));
    assert_eq!(plan.matched, vec![(0, vec![id(1)]), (1, vec![id(3), id(4), id(5)])]);
    // the HUD line finds its item and names the tee's line; the link is not bound
    assert_eq!(plan.huds, vec![(0, vec![id(2)], Some(0))]);
    assert_eq!(plan.pictures.get(&0), Some(&id(7)));
    assert_eq!(plan.skipped, vec!["Ghost".to_owned()]);
    assert_eq!(plan.unnamed, 1);
    assert_eq!(plan.item_count(), 4);
    assert_eq!(card.items.len(), 3);
}

#[test]
fn a_trusted_card_fills_the_catalogue_and_undo_puts_everything_back() {
    let mut wardrobe = Wardrobe::in_memory();
    let (card, plan) = planned(&wardrobe, TEXT, CREATOR, &folder(CREATOR));
    let embedded = vec![("Fox tail".to_owned(), id(0x1003))];
    let done = wardrobe.change(true, |txn| matching::apply(txn, &card, &plan, Groups::ALL, &embedded));
    assert_eq!(done.store.as_deref(), Some("Static Bloom"));
    assert_eq!((done.added, done.updated), (4, 0));
    let store = &wardrobe.catalogue.stores[&CREATOR];
    assert_eq!((store.source, store.card_version.as_deref()), (Source::Card, Some("1.2")));
    let tee = wardrobe.entry(id(1)).expect("tee");
    assert_eq!(tee.categories(), [Tag::new("tops", "t-shirt")]);
    assert_eq!(tee.fits(), ["Aster".to_owned(), "Nova".to_owned()]);
    assert_eq!(tee.event(), Some("Moonlit Market"));
    assert_eq!(tee.version(), Some("1.2"));
    assert_eq!(tee.store.map(|s| s.name.as_str()), Some("Static Bloom"));
    assert_eq!(tee.product.map(|p| p.huds.clone()), Some(vec!["Band tee HUD".to_owned()]));
    assert_eq!(tee.binding.picture, Some(id(7)));
    assert!(tee.binding.new);
    // the HUD is not a Wardrobe item of its own
    assert!(wardrobe.entry(id(2)).is_none());
    // the three tails are copies of one product, which is a kit
    let tail = wardrobe.entry(id(4)).expect("tail");
    assert_eq!(tail.kit(), ["Fox tail".to_owned()]);
    assert_eq!(tail.part(), Part::Tail);
    assert_eq!(tail.note(), Some("Fluffy"));
    assert_eq!(tail.product.map(|p| p.asset_ids.clone()), Some(vec![id(0x1003)]));
    let copies = wardrobe.copies();
    assert_eq!(copies[format!("{CREATOR}/night-market-set/fox-tail").as_str()].len(), 3);
    assert_eq!(wardrobe.catalogue.kits_known.len(), 1);
    assert_eq!(
        wardrobe.account.cards[&CARD],
        CardState::Applied {
            version: Some("1.2".into())
        }
    );

    assert!(wardrobe.undo());
    assert!(wardrobe.catalogue.stores.is_empty() && wardrobe.catalogue.packs.is_empty() && wardrobe.catalogue.products.is_empty());
    assert!(wardrobe.catalogue.kits_known.is_empty());
    assert!(wardrobe.account.bindings.is_empty() && wardrobe.account.cards.is_empty());
    assert!(!wardrobe.undo());
}

#[test]
fn a_card_by_someone_else_is_not_trusted_and_claims_no_store() {
    let mut wardrobe = Wardrobe::in_memory();
    let (card, plan) = planned(&wardrobe, TEXT, STRANGER, &folder(CREATOR));
    assert!(!plan.trusted);
    assert_eq!(plan.items_creator, Some(CREATOR));
    let done = wardrobe.change(true, |txn| {
        matching::apply(txn, &card, &plan, Groups::ALL, &[(String::from("Fox tail"), id(0x1003))])
    });
    assert_eq!(done.store, None);
    assert!(wardrobe.catalogue.stores.is_empty());
    let tee = wardrobe.entry(id(1)).expect("tee");
    assert_eq!(tee.product.map(|p| p.trusted), Some(false));
    assert_eq!(tee.pack.map(|p| p.source), Some(Source::User));
    // no asset id is recorded from a stranger's card
    assert!(wardrobe.catalogue.products.values().all(|p| p.asset_ids.is_empty()));

    // items by two creators: nobody's card is trusted
    let mut mixed = folder(CREATOR);
    mixed[2].creator = STRANGER;
    let (_, plan) = planned(&wardrobe, TEXT, CREATOR, &mixed);
    assert!(!plan.trusted);
    assert_eq!(plan.items_creator, None);
}

#[test]
fn the_creators_record_is_never_replaced_by_someone_elses_card() {
    let mut wardrobe = Wardrobe::in_memory();
    let items = folder(CREATOR);
    let (card, plan) = planned(&wardrobe, TEXT, CREATOR, &items);
    wardrobe.change(true, |txn| matching::apply(txn, &card, &plan, Groups::ALL, &[]));
    let other = TEXT.replace("Tops/T-shirt", "Bottoms/Skirt").replace("Moonlit Market", "Elsewhere");
    let (card, plan) = planned(&wardrobe, &other, STRANGER, &items);
    wardrobe.change(true, |txn| matching::apply(txn, &card, &plan, Groups::ALL, &[]));
    let tee = wardrobe.entry(id(1)).expect("tee");
    assert_eq!(tee.categories(), [Tag::new("tops", "t-shirt")]);
    assert_eq!(tee.event(), Some("Moonlit Market"));
}

#[test]
fn the_users_values_win_card_after_card() {
    let mut wardrobe = Wardrobe::in_memory();
    let items = folder(CREATOR);
    let (card, plan) = planned(&wardrobe, TEXT, CREATOR, &items);
    wardrobe.change(true, |txn| matching::apply(txn, &card, &plan, Groups::ALL, &[]));
    wardrobe.change(false, |txn| {
        let binding = txn.binding(id(1));
        binding.categories = Some(vec![Tag::new("full-body", "costume")]);
        binding.fits = Some(vec!["Nova".into()]);
        binding.note = Some("Mine".into());
        binding.picture = Some(id(99));
        binding.picture_by_user = true;
        binding.new = false;
    });
    // a small edit is not undoable: the stack still holds the card alone
    let newer = TEXT.replace("version = 1.2", "version = 1.3").replace("Tops/T-shirt", "Tops/Shirt");
    let (card, plan) = planned(&wardrobe, &newer, CREATOR, &items);
    let done = wardrobe.change(true, |txn| matching::apply(txn, &card, &plan, Groups::ALL, &[]));
    assert_eq!((done.added, done.updated), (0, 4));
    let tee = wardrobe.entry(id(1)).expect("tee");
    assert_eq!(tee.categories(), [Tag::new("full-body", "costume")]);
    assert_eq!(tee.fits(), ["Nova".to_owned()]);
    assert_eq!(tee.note(), Some("Mine"));
    assert_eq!(tee.binding.picture, Some(id(99)));
    assert!(!tee.binding.new);
    // the record itself follows the newest card
    assert_eq!(tee.product.map(|p| p.categories.clone()), Some(vec![Tag::new("tops", "shirt")]));
    assert_eq!(tee.version(), Some("1.3"));
    // an edited copy keeps its own values: the other tails are untouched
    wardrobe.change(false, |txn| {
        txn.binding(id(4)).categories = Some(vec![Tag::new("accessories", OTHER)])
    });
    assert_eq!(wardrobe.entry(id(4)).expect("copy").part(), Part::Other);
    assert_eq!(wardrobe.entry(id(3)).expect("original").part(), Part::Tail);
}

#[test]
fn an_older_card_leaves_the_pack_alone() {
    let mut wardrobe = Wardrobe::in_memory();
    let items = folder(CREATOR);
    let (card, plan) = planned(&wardrobe, TEXT, CREATOR, &items);
    wardrobe.change(true, |txn| matching::apply(txn, &card, &plan, Groups::ALL, &[]));
    let older = TEXT.replace("version = 1.2", "version = 1.1").replace("Tops/T-shirt", "Tops/Vest");
    let (card, plan) = planned(&wardrobe, &older, CREATOR, &items);
    assert!(plan.stale);
    wardrobe.change(true, |txn| matching::apply(txn, &card, &plan, Groups::ALL, &[]));
    assert_eq!(wardrobe.entry(id(1)).expect("tee").categories(), [Tag::new("tops", "t-shirt")]);
    assert_eq!(
        wardrobe.catalogue.packs.values().next().and_then(|p| p.version.as_deref()),
        Some("1.2")
    );
}

#[test]
fn only_the_chosen_groups_apply() {
    let mut wardrobe = Wardrobe::in_memory();
    let (card, plan) = planned(&wardrobe, TEXT, CREATOR, &folder(CREATOR));
    let groups = Groups {
        store: false,
        items: true,
        pack: false,
    };
    assert_eq!(groups.count(), 1);
    wardrobe.change(true, |txn| matching::apply(txn, &card, &plan, groups, &[]));
    assert!(wardrobe.catalogue.stores.is_empty());
    let tee = wardrobe.entry(id(1)).expect("tee");
    // the pack exists by its name alone, without its event
    assert_eq!(tee.pack.map(|p| p.name.as_str()), Some("Night market set"));
    assert_eq!(tee.event(), None);
}

#[test]
fn the_creators_card_replaces_a_store_the_user_linked() {
    let mut wardrobe = Wardrobe::in_memory();
    wardrobe.change(false, |txn| {
        txn.set_store(Store {
            key: CREATOR,
            name: "My guess".into(),
            source: Source::User,
            ..Store::default()
        });
    });
    let (card, plan) = planned(&wardrobe, TEXT, CREATOR, &folder(CREATOR));
    let done = wardrobe.change(true, |txn| matching::apply(txn, &card, &plan, Groups::ALL, &[]));
    assert!(done.replaced_user_store);
    assert_eq!(wardrobe.catalogue.stores[&CREATOR].name, "Static Bloom");
    assert!(wardrobe.undo());
    assert_eq!(wardrobe.catalogue.stores[&CREATOR].name, "My guess");
}

#[test]
fn a_renamed_copy_binds_by_its_asset_or_by_the_users_word() {
    let mut wardrobe = Wardrobe::in_memory();
    let items = folder(CREATOR);
    let (card, plan) = planned(&wardrobe, TEXT, CREATOR, &items);
    wardrobe.change(true, |txn| {
        matching::apply(txn, &card, &plan, Groups::ALL, &[("Fox tail".into(), id(0x1003))])
    });
    // an untouched copy under another name: same asset
    let mut renamed = object(20, "My favourite", CREATOR);
    renamed.asset_id = id(0x1003);
    // an edited copy: a new asset, a name beyond the fold
    let edited = object(21, "Stubby", CREATOR);
    let all = [&renamed, &edited];
    assert_eq!(wardrobe.change(true, |txn| matching::rebind_by_asset(txn, &all)), 1);
    assert_eq!(
        wardrobe.entry(id(20)).and_then(|e| e.product.map(|p| p.name.clone())).as_deref(),
        Some("Fox tail")
    );
    assert!(wardrobe.entry(id(21)).is_none());
    assert!(wardrobe.change(true, |txn| matching::same_product(txn, &edited, &items[2])));
    assert_eq!(wardrobe.entry(id(21)).expect("edited").kit(), ["Fox tail".to_owned()]);
    assert_eq!(wardrobe.copies()[format!("{CREATOR}/night-market-set/fox-tail").as_str()].len(), 5);
    // another creator's item is another product: nothing is joined, and
    // no store passes from one creator to the other
    let foreign = object(22, "Fox tail", STRANGER);
    assert!(!wardrobe.change(true, |txn| matching::same_product(txn, &foreign, &items[2])));
    assert!(!wardrobe.contains(&id(22)));
    // an item that is not in the Wardrobe has nothing to share
    assert!(!wardrobe.change(true, |txn| matching::same_product(txn, &edited, &items[5])));
    // two items no card describes become one product of the user's own
    let (first, second) = (object(30, "Plain cap", CREATOR), object(31, "Plain cap (blue)", CREATOR));
    wardrobe.change(false, |txn| txn.binding(id(30)).sorted = vec![Tag::new("accessories", "hat")]);
    assert!(wardrobe.change(true, |txn| matching::same_product(txn, &second, &first)));
    let key = format!("{CREATOR}/user/plain-cap");
    assert_eq!(wardrobe.copies()[key.as_str()].len(), 2);
    assert_eq!(wardrobe.entry(id(31)).expect("copy").categories(), [Tag::new("accessories", "hat")]);
    assert!(!wardrobe.catalogue.products[&key].trusted);
    // and apart again, each keeping its category
    wardrobe.change(true, |txn| matching::separate(txn, id(31)));
    assert_eq!(wardrobe.entry(id(31)).and_then(|e| e.product.map(|p| p.key.clone())), None);
    assert_eq!(wardrobe.entry(id(31)).expect("copy").categories(), [Tag::new("accessories", "hat")]);
    // removing an item from the Wardrobe removes its binding only
    wardrobe.change(true, |txn| txn.remove_binding(id(21)));
    assert!(!wardrobe.contains(&id(21)));
    assert!(wardrobe.undo());
    assert!(wardrobe.contains(&id(21)));
}

#[test]
fn kits_come_from_the_items_that_declare_them() {
    let mut wardrobe = Wardrobe::in_memory();
    let items = folder(CREATOR);
    let (card, plan) = planned(&wardrobe, TEXT, CREATOR, &items);
    wardrobe.change(true, |txn| matching::apply(txn, &card, &plan, Groups::ALL, &[]));
    let refs: Vec<&InvItem> = items.iter().collect();
    let worn = [id(4)].into_iter().collect();
    let kits = fit::kits(&wardrobe, &refs, &worn);
    let tail = kits.find("fox tail").expect("kit");
    assert!(tail.worn && tail.owned);
    assert_eq!(tail.part, Part::Tail);
    assert_eq!(fit::fit(&["Fox tail".to_owned()], &kits), fit::Fit::Fits("Fox tail".into()));
    let none = fit::kits(&wardrobe, &refs, &Default::default());
    assert_eq!(
        fit::fit(&["Fox tail".to_owned()], &none),
        fit::Fit::NotFor {
            kit: "Fox tail".into(),
            part: Part::Tail,
            nothing_worn: true
        }
    );
    // a seeded body is known by name even when nobody owns it
    assert_eq!(none.find("Lara Petite").map(|k| (k.part, k.owned)), Some((Part::Body, false)));
}

#[test]
fn no_store_stands_for_an_unknown_creator() {
    let mut wardrobe = Wardrobe::in_memory();
    wardrobe.change(false, |txn| {
        txn.set_store(Store {
            key: Uuid::nil(),
            name: "Nobody".into(),
            source: Source::User,
            ..Store::default()
        });
        txn.set_store(Store {
            key: CREATOR,
            name: "Static Bloom".into(),
            source: Source::User,
            ..Store::default()
        });
        txn.binding(id(1)).sorted = vec![Tag::new("tops", OTHER)];
    });
    assert_eq!(wardrobe.catalogue.stores.len(), 1);
    let entry = wardrobe.entry(id(1));
    // an item whose creator is not loaded yet shows no store at all
    assert!(wardrobe.store_of(entry.as_ref(), Uuid::nil()).is_none());
    assert!(wardrobe.store_of(entry.as_ref(), STRANGER).is_none());
    assert_eq!(
        wardrobe.store_of(entry.as_ref(), CREATOR).map(|s| s.name.as_str()),
        Some("Static Bloom")
    );
}
