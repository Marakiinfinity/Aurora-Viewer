# Wardrobe — design

The Wardrobe is a local index of the clothing, bodies and accessories of an
account, with the data the inventory does not hold: the store that made an
item, what it was created for (a body, a head, a pair of ears, a size of
them…), its category, its HUDs, the pack and the event it came from, and a
picture. Creators ship that data in a **Wardrobe card**, a plain notecard in
the product box; users add or correct it by hand. The inventory window shows it as a **Wardrobe mode** next to the list,
and the tooltips explain it.

Nothing leaves the viewer: the Wardrobe lives in files on the PC, never on
the grid. The Legacy Viewer has no equivalent; the related sources are named
where a behaviour follows one.

This page is the specification agreed on 2026-11-15, before any code. It is
the reference for the implementation and the human validation, and it is
updated with the code. The mockups it describes are in the team's design
canvas ("Aurora Inventory Wardrobe mockups").

## 1. Principles

- **Local.** Which items are in the Wardrobe, their categories, notes,
  kits, store links and HUD links are files on the PC. The viewer creates
  no inventory folder and no link for the Wardrobe. A "mirror as inventory
  links" option (an `#Aurora/Wardrobe/<Category>` folder of links, visible
  in other viewers) is a later opt-in, not part of the first release.
- **The user's notes always win.** Whatever a card says, a value the user
  set by hand is kept, card after card. Undo is always offered after a card
  is applied.
- **Cards are data.** A notecard is parsed against the grammar of section 5
  and nothing else: no script, no link followed, unknown keys ignored,
  values capped in length. Only the creator of the items can speak for
  their store (section 6).
- **Creators' permissions.** No asset id is shown, copied, exported or
  written to a card unless the existing rule allows it
  (`world::inventory::actions::asset_uuid_shown`). A logo texture is
  rendered, never printed. A main store is an SLurl, which the viewer
  opens through its existing SLurl handling (`slurl.rs`), never a landmark
  asset.
- **Hints, never refusals.** A fit mismatch, a card from a stranger or an
  item that fits no known kit is shown and explained; wearing is never
  blocked. The only refusals are the existing rules of the viewer (layer
  order, permissions, worn-object drops), and they say why (section 11).
- **Performance.** The catalogue is in memory, indexed by inventory item id
  and by asset id; a lookup per item per frame is the budget. Fit states and
  HUD matches are computed once per inventory generation and per outfit
  change. Files are written by a background thread, like the saved smart
  searches (`ui/inventory/smart_store.rs`).

## 2. Vocabulary

| Word | Meaning |
|---|---|
| Store | A creator's shop: name, logo, Marketplace page, main store SLurl, group. One creator may have one store in the catalogue. |
| Pack | A product box: name, version, event, demo flag, and the items it holds. |
| Item | One inventory item of a pack, with its category, subtype, kits, HUDs, note and picture. |
| Card | The `Aurora Wardrobe` notecard that describes a store, a pack and its items. |
| Kit | Anything the user owns or wears that other items are created for: a mesh body, a head, ears, hands, feet, a tail base, an add-on, and a size of any of them. An item is created for one or several kits; a kit is itself an item (or a known product) with a part. |
| Part | The kind of a kit, used to compare like with like: body, head, ears, hands, feet, tail, wings, add-on, other. |
| Category | One of nine fixed groups (section 4), with fixed subtypes. An item may carry several; the first is its main one. |
| Binding | The link between an inventory item id of one account and an item record of the catalogue. |

## 3. Storage

Two kinds of files, both pretty JSON with a `format` and a `version` field,
like `world/outfit_backup.rs`, read at login and written by a background
thread.

**The catalogue**, shared by every account on the PC, in the application
data folder (`wardrobe/catalogue.json`):

```json
{
  "format": "aurora-wardrobe-catalogue",
  "version": 1,
  "stores": [
    { "key": "<creator uuid>", "name": "Static Bloom", "logo": "<texture uuid or null>",
      "marketplace": "https://marketplace.secondlife.com/stores/…", "mainstore": "secondlife://Region/128/128/25",
      "group": "<group uuid or null>", "source": "card|user", "card_version": "1.2" }
  ],
  "packs": [
    { "key": "<creator uuid>/night-market-set", "store": "<creator uuid>", "name": "Night market set",
      "version": "1.2", "event": "Moonlit Market", "demo": false, "source": "card|user" }
  ],
  "items": [
    { "key": "<creator uuid>/night-market-set/oversized-band-tee", "pack": "<creator uuid>/night-market-set",
      "name": "Oversized band tee", "categories": [["tops", ["t-shirt", "crop-top"]], ["full-body", ["costume"]]],
      "fits": ["Aster", "Nova"], "huds": ["Band tee HUD"], "picture": "sweater-icon",
      "note": "Rigged, 6 colours", "version": null, "asset_ids": ["<uuid>"] },
    { "key": "<creator uuid>/aster-body/aster-body", "pack": "<creator uuid>/aster-body",
      "name": "Aster Body", "categories": [["body-parts", ["mesh-body"]]],
      "kit": ["Aster", "Aster Petite"], "part": "body" }
  ],
  "kits_known": [ { "name": "Aster", "part": "body", "creator": "<uuid>", "patterns": ["Aster Body"] },
                  { "name": "Fae ears", "part": "ears", "creator": "<uuid>", "patterns": ["Fae Ears"] } ]
}
```

- `key` values are slugs derived from the creator id and the names, so two
  cards for the same pack update one record.
- `asset_ids` are optional, filled only from a card that embeds the items
  (the creator's own full-permission records). They are never displayed.
- `kit` on an item declares the kit names that item provides (a body and
  its sizes, a head, a pair of ears…); `part` says what kind of kit it is.
  Any item with a `kit` is something other items can be created for.
- `kits_known` is the alias table (section 9), seeded from a data file in
  the repository and extended by cards and by the user.

**The account file**, per account, in the account folder
(`wardrobe-account.json`): bindings from item ids to catalogue keys, the
user's overrides per item (category, subtype, fits, picture, note,
favourite), the user's store links, the HUD links and exclusions, the
ignored cards, the chosen kit and the view settings, and the undo stack of
the last applied cards.

The demo keeps both in memory, like the saved searches.

## 4. Categories and subtypes

Nine categories, fixed, shown in the rail in this order. Subtypes are
fixed too. The keys are the English words below in kebab case; the
interface shows them through `aurora_i18n::tr!` (English, French,
Japanese). The list ships as a data file; a unit test checks that every key
has its three translations.

| Category | Subtypes |
|---|---|
| Tops | T-shirt, Shirt, Blouse, Sweater, Hoodie, Jacket, Coat, Vest, Crop top, Bra, Bodysuit, Other |
| Bottoms | Jeans, Trousers, Shorts, Skirt, Leggings, Underwear, Other |
| Full body | Dress, Jumpsuit, Suit, Swimwear, Lingerie set, Costume, Other |
| Shoes | Sneakers, Boots, Heels, Sandals, Flats, Socks and stockings, Other |
| Hair | Short, Long, Updo, Ponytail, Braids, Facial hair, Other |
| Heads | Mesh head, Head skin, Eyes, Mouth, Head shape, Makeup, Brows, Hairbase, Freckles and scars, Other |
| Body parts | Mesh body, Hands, Feet, Ears, Tail, Wings, Add-on, Body skin, Body shape, Tattoo, Nails, Body hair, Alpha mask, Other |
| Accessories | Earrings, Necklace, Ring, Bracelet, Piercing, Glasses, Hat, Bag, Belt, Gloves, Scarf, Watch, Other |
| HUDs | Animation (AO), Utility, RLVa, Camera and photo, Communication, Shopping, Games, Other |

- A card gives a category alone (`Tops`) or both (`Tops/T-shirt`). Every
  category has an `Other` subtype for what the list does not name; a
  category given alone is the same as its `Other`.
- An item may carry **several categories**, up to four, as a comma
  separated list, and **up to two subtypes per category**, joined by `+`
  (`Tops/Shirt+Jacket, Bottoms/Shorts` for a romper with an open shirt,
  `HUDs/RLVa+Animation (AO)` for a HUD that is both). The first category is
  the **main** one, and its first subtype the main subtype: they give the
  type shown under the card in the All view, the slot of Wearing now, and
  the destination of Auto-sort. The item is listed under each of its
  categories and subtypes in the rail and the filters, counted once in All
  and once per category elsewhere. A category repeated in the list merges
  into one entry. Wearable types and Auto-sort give one category and one
  subtype; the user and the cards may give more.
- An unknown category is not refused: the item is kept with the nearest
  category when one can be told from the wearable type or the attachment
  point, otherwise it stays unsorted, and the review sheet says so. An
  unknown subtype of a known category becomes that category's `Other`, and
  the review sheet shows the subtype as written.
- The HUDs category is for **standalone** HUDs only: an AO, a radar, a
  camera tool, a translator, a shopping or a game HUD. A HUD that belongs to
  an item (declared by `hud-for`, or found by the rules of section 8) is
  not a card of its own: it shows as "+HUD" on its item, which means the
  item comes with a HUD, not that it is one.
- Animations, landmarks, notecards, scripts and gift cards are not Wardrobe
  items. Demo is a flag of the pack.
- The list is namespaced per tab (`wardrobe.*`), so the later Decoration tab
  brings its own list without a format change.

**Wearable types** map to categories without a card: shirt, undershirt,
jacket → Tops; pants, underpants → Bottoms; skirt → Bottoms/Skirt; shoes,
socks → Shoes; hair → Hair; skin → Body parts/Body skin; shape → Body
parts/Body shape; eyes → Heads/Eyes; alpha → Body parts/Alpha mask; tattoo,
universal → Body parts/Tattoo; gloves → Accessories/Gloves; physics →
not a Wardrobe item.

## 5. The Wardrobe card

A notecard whose first line is `# Aurora Wardrobe 1`. Plain text, so it can
be read and written in any viewer. At most 64 KiB (the notecard limit). Three
blocks, each optional, in any order; `[items]` is the one that matters.

```text
# Aurora Wardrobe 1
[store]
name        = Static Bloom
logo        = <texture UUID>
marketplace = https://marketplace.secondlife.com/stores/…
mainstore   = secondlife://Region/128/128/25
group       = <group UUID>

[pack]
name    = Night market set
version = 1.2
event   = Moonlit Market
demo    = no

[items]
Oversized band tee = Tops/T-shirt   fits Aster, Nova   picture band-tee-icon
Band tee HUD       = hud-for: Oversized band tee
Rib tattoo         = Body parts/Tattoo   fits everyone   version 1.3   note Faded ink, BOM
Fae ears           = Body parts/Ears   kit Fae ears   fits Sora, Lumen
Moon ear cuffs     = Accessories/Earrings   fits Fae ears
```

Grammar:

- A line that starts with `#` is a comment. Blank lines are ignored.
- `[store]`, `[pack]`, `[items]` open a block. Keys of `[store]` and
  `[pack]` are `key = value`, one per line, unknown keys ignored.
- An `[items]` line is
  `<item name> = <category>[/<subtype>[+<subtype>]][, <category>[/…]…]`
  (the first category and its first subtype are the main ones) followed by
  clauses separated by two or more spaces: `fits <kit>[, <kit>…]`,
  `fits everyone`, `kit <name>[, <name>…]`, `picture <texture name>`,
  `version <text>`, `note <text>`, `event <text>`, `demo yes|no`. Clause
  order is free.
- `fits` names what the item was created for: a body, a head, a pair of
  ears, a size (`Aster Petite`), or another item of the same card that
  declares a `kit`. An item may name several; it fits when any one of them
  is worn. An item may both declare a kit and fit others: the ears above
  are created for two heads and the cuffs for those ears.
- `kit` declares the names an item provides to others, with its sizes as
  separate names (`kit Aster, Aster Petite`). Its part comes from the
  item's category and subtype (Mesh body → body, Mesh head → head, Ears →
  ears, Hands, Feet, Tail, Wings, Add-on, else other).
- `<item name> = hud-for: <item name>` declares a HUD of another item of the
  same card. Several `hud-for` lines may name one item.
- Names match the inventory names of the items in the folder that holds the
  card, case-insensitively, with trailing version numbers and bracketed
  tags ignored (the same name folding as the HUD rules, section 8). A line
  that names no item of the folder is skipped and reported.
- `picture` names a texture item of the same folder; the viewer shows that
  texture as the card picture and falls back to the item's inventory image.
- `mainstore` is an SLurl (`secondlife://Region/x/y/z` or a
  `maps.secondlife.com` link); anything else is ignored. `logo` is the only
  id a card may carry; it is rendered and never printed.
- Caps: 256 bytes per value, 24 characters for `note`, 64 items per card,
  4 categories per item, 2 subtypes per category, 16 names per `fits` or
  `kit` clause.
- Version strings compare as dotted integers when they parse as such,
  otherwise as text. A card whose pack version is lower than the
  catalogue's is ignored for that pack and the review sheet says so.

Legacy Viewer source for the notecard itself: `LLNotecard`
(indra/llinventory/llnotecard.cpp) through the existing
`aurora_net::inventory::notecard::Notecard` decoder; the card is its text.

## 6. Matching and trust

- **Scope.** A card applies to the items of the folder that holds it, and
  of that folder's subfolders when the folder holds one product (one pack
  name, one creator). A card imported from a file applies to the folder the
  user chooses.
- **Trust.** A card is *trusted* when the notecard's creator is the creator
  of every item it names. The review sheet shows "Made by X, the creator of
  these items" with the green check. Otherwise it shows "Made by X, not by
  the items' creator (Y)": the `[store]` block is ignored, since only the
  creator can say which store is theirs, and the `[items]` and `[pack]`
  groups are applied only if the user chooses them. This covers resold and
  edited objects, and the user's own cards.
- **Asset ids.** When a card embeds the items (the creator dropped them into
  the notecard), their asset ids are recorded and used to rebind a renamed
  or redelivered copy. They are never shown.
- **Precedence** for every field: the user's override, then the newest
  trusted card, then an untrusted card the user applied, then the
  heuristics (wearable type, attachment point, HUD name rules).
- **Copies.** Every copy of an item has its own binding to the same product
  record and its own overrides, so an edited tail keeps the store, the
  kits and the HUDs of the product and can have its own category, fits,
  picture and note. The copies a card's folder holds bind to the same line
  through the name folding ("Fox tail", "Fox tail 2", "Fox tail (short)").
  An edited copy has a new asset id, so only an untouched copy is ever
  rebound by asset id; the others follow their names. A copy renamed
  beyond the fold binds to nothing until the user says what it is with
  "Same product as…" (section 7). The copies of one product show as a
  variant stack (section 10).

## 7. Import, review, Auto-sort

**Detection.** A notecard named or starting with `# Aurora Wardrobe` is
recognised when its folder arrives in Received items
(`world/received_items.rs`), when a box is unpacked, when the user opens it,
or from the notecard's right-click menu ("Wardrobe card › Review and
apply…, Apply all, Ignore this card"). The notecard row carries an
"Applied" or "Review" badge in the list.

**Trusted cards apply at once.** A toast over the world says what happened
("Static Bloom · 6 items added to your Wardrobe · Read from their Wardrobe
card. Your own notes stay as they are.") with Review and Undo. Undo restores
every field the card changed, from the account file's undo stack, until the
next login.

**Other cards wait.** The toast says "A Wardrobe card is waiting · made by
someone other than the items' creator, so nothing was applied" with Review
and Ignore.

**The review sheet** (from the menu or the toast): header with store, pack
and the count of items in the folder; the trust line; three groups with a
checkbox each, Store (name, logo, links), Items (matched lines with
category, fits, HUDs, pictures), Pack and event; warnings for skipped lines
("1 line names an item that isn't in this folder"); the footnote "Your own
notes always win over the card"; Cancel and "Apply N groups".

**Auto-sort** files the items that are not in the Wardrobe yet and that the
viewer can place without a card: wearables by type (section 4),
attachments whose name or attachment point gives a category with no
ambiguity, and objects the user wore on a HUD attachment point that the
rules of section 8 attach to no item, which go to HUDs/Other. Everything else stays unsorted and is listed under "Unsorted" in
the rail's All view. The button shows the count waiting; the result is a
dismissable note "Filed 5 new items · 2 Tops, 1 Shoes…" with Undo.

**Add to Wardrobe** by hand: the inventory right-click menu of any wearable
or attachment, with the category submenu; the card's right-click menu
offers Categories… (a check per category and subtype, two subtypes at
most per category, the main ones marked, "Make main" on any other), Fits…,
Change picture…, Favorite, Same product as… (pick another item: this one
binds to its product record and inherits store, pack, kits and HUDs, its
own overrides kept; also offered on an item that is not in the Wardrobe,
for renamed copies and items received without a card), Show original in
the inventory, Remove from Wardrobe (which removes the binding and the
overrides, never the item).

## 8. Stores and HUDs

**A creator's store** comes, in order, from the creator's trusted card;
else from a store the user linked ("Link a store to kiko.makes": name,
Marketplace page, a logo chosen among the user's textures or the initials,
"Use for all N items by this creator", saved on this computer only); else
nothing, and the creator shows with the round profile picture, as residents
do everywhere in Aurora, with "Link a store…" and "Profile" buttons. A
trusted card arriving later replaces a user link and says so. Marketplace
links open through the existing link trust rules (`link_trust.rs`).

**Finding an item's HUDs**, in order; the first rule that matches wins:

1. The card's `hud-for` line.
2. Same name plus "HUD": `Sora head [HUD]`, `Sora head HUD`,
   `Sora head (HUD)`, `Sora head – HUD`; case, brackets, dashes and version
   numbers ignored.
3. "HUD" in the name, in the same folder or in a `HUD` / `HUDs` subfolder,
   only when the folder holds one product.
4. Remembered: an object of that folder the user already wore on a HUD
   attachment point.
5. The user's choice: "Link another HUD…" and "Not a HUD of this item",
   kept for that item.

HUDs are **added**, never worn (`LLAppearanceMgr::wearItemOnAvatar` with
replace false, indra/newview/llappearancemgr.cpp), so a HUD already on the
same screen point stays. "Wear all HUDs" does the same and reports when the
screen points or the attachment limit run out. The HUD button of the
selection bar reads Wear HUD, Detach HUD, or "N HUDs" with a menu naming
each HUD's kind (Customise, Emotes, Colours…) and its own Wear / Detach.
The filter chip "Has a HUD" joins the status chips.

**Standalone HUDs** (an AO, a utility, a camera tool…) are Wardrobe items
of the HUDs category (section 4), with their own cards, Wear and Detach. A
HUD that the rules above attach to an item never also appears there; a HUD
of the same folder that matches no item does, when the user adds it or
Auto-sort files it. "Not a HUD of this item" moves a wrongly matched HUD to
the HUDs category.

## 9. Kits, dependencies and fit

A **kit** is anything other items are created for: a body, a head, ears,
hands, feet, a tail base, an add-on, and a size of any of them. The
relation is a dependency, not a body check: a shirt is created for a body
or a size of it, a tattoo for a head, ear cuffs for a pair of ears, and
those ears for a head. Creators make one product for several kits, so an
item names every kit it was created for and fits when any one is worn.

The account's kits come from, in order: a worn or owned item that declares
a `kit` (section 5); the alias table (`kits_known`, seeded by a data file in
the repository with the mainstream bodies, heads and add-ons, matched by
creator id and name pattern against worn attachments); the user, who can
add or correct a kit by hand, its sizes included. Every kit has a **part**:
body, head, ears, hands, feet, tail, wings, add-on or other.

The **"Created for" chip** chooses one kit at a time: the menu lists every
kit the items name, grouped by part, with "✓ wearing" when it is worn and
the count of items created for it; a size lists under its kit. The Wide
layout shows a segment instead: My kits, each worn kit with a check, Any,
Fits everyone.

**Fit of an item**, computed per part. The item's `fits` names are resolved
to kits; only the worn kits of the same parts are compared, so changing
heads never turns a shirt grey and swapping ears only affects what was
created for ears:

| State | Rule | Shown as |
|---|---|---|
| Fits | one of the item's `fits` names is worn (the kit or one of its sizes) | green check and that kit's name |
| Fits everyone | no `fits` line, `fits everyone`, a BOM layer, an unrigged attachment | "Fits everyone" |
| Not for what you wear | the item names kits of a part, and none of them is worn | grey "Not for Aster", or "Needs Fae ears" when nothing of that part is worn |
| Unknown kit | a `fits` name no kit of the catalogue or the alias table provides | grey, the name as written, "unknown kit" in the tooltip |

An item may depend on kits of several parts at once (ear cuffs created for
one pair of ears, which in turn exist for two heads); each part is judged
on its own and the card shows the first that fails. Wearing an item in the
"not for" state shows one notification naming both sides; it never
refuses. Fit states are computed once per inventory generation and per
outfit change, from the same worn set the smart search's `Worn` rule uses.

## 10. Wardrobe mode of the inventory window

A third view toggle in the title bar: List, Wardrobe, Decoration (the last
one greyed until it exists). Wardrobe mode uses the Aurora window's existing
breakpoints: Thin under 420 px, Compact to 700 px, Medium to 1020 px, Wide
above. The Legacy layout keeps its tree and gets the tooltips, the search
scopes, the badges and the import sheet, since the rules are shared.

**Common parts at every width:**

- A category rail at the left: All wardrobe, then the nine categories, with
  a dot on a category that has new items. Wide adds "Wearing now" at the
  top.
- Cards: a square picture (the card's picture, else the inventory image,
  else the category icon on a neutral tile), the worn check at the top
  left, a NEW tag at the top right (pulses once, the first time its category
  is shown), "+HUD" or "+2 HUDs" at the bottom left, the store logo at the
  bottom right; the name under the picture, and the main category's type
  and the store under it in the All view.
- Chips: Worn, New, Favorites, Has a HUD; "Created for"; More filters. Thin
  opens a bottom filter sheet instead: Show (Worn, Not worn, Favorites, New
  this week, Unopened boxes), Created for, Event, Store with counts, "Show N
  items".
- Sort: Date added (default, newest first), Last worn, Name, Store,
  Category; an order toggle.
- The selection bar at the bottom: picture, name, store logo and name, the
  fit chip, the keycaps Space (Quick look) and Enter (Wear) where there is
  room, Wear HUD when there is one, Wear / Take off, More.
- Worn items are never grouped apart: the check mark is enough.
- **Variant stacks.** The copies bound to one product record show as one
  card with a count badge ("×3") at the top right, next to NEW. Clicking
  the stack opens its copies in place, one card each, with the worn one
  checked, the newest edit marked, and the untouched original marked when
  one still exists (the copy whose asset id is the product's). Wear, Add
  and Quick Look act on the copy chosen in the stack, the main copy being
  the worn one, else the newest. Stacks are on by default; Preferences ›
  Inventory › Wardrobe turns them off, and every copy is then its own
  card. "Show original in the inventory" points at the chosen copy.

**Browse by** (Medium and Wide): a menu switching the sidebar between
Creator, Category, Folder & set and Event, each entry with its count;
creators with a square store logo or a round profile picture and "no store
linked" or "set by you"; the grid grouped by set, with the set's event. The
heading of a creator shows the Marketplace store button.

**Search**: scopes Everything, Names, Creators & stores, Events, Folders &
sets, Descriptions; results grouped by Events, Stores and creators, Folders
and sets, Items, with the matched text highlighted; shortcuts `creator:`,
`event:`, `body:`, `folder:`, `worn`, `new`. The scopes and shortcuts are
rules of the smart search (`ui/inventory/smart.rs`), so saved searches use
them too.

**Quick Look** (Space, or the selection bar): a card over the dimmed window
with a 256 px picture, previous and next, the store with its Marketplace
link, the fits chips, the HUD line, the event, Try on and Wear; keys ← →,
Enter, Space or Esc to close.

**Try on**: a toggle in the tools row. When on, clicking a card wears the
item at once and the bar shows "Trying on · in place of X" with Revert and
Keep; the card shows TRYING ON. Revert restores the previous item through
the ordinary outfit mutation. Off by default.

**Wearing now** (Wide rail, and the avatar tab in Thin): slots head to toe,
Hair, Head, Top, Bottom, Full body, Shoes, Body parts, Accessories, "6 of 8
slots", an empty slot reads "Nothing worn" and opens its category, and a
Save outfit button that uses the existing outfit saving.

**Motion**, every width, honouring the viewer's reduce-motion setting:

| Moment | What moves | Time |
|---|---|---|
| Hover a tile | rises 2 px, edge brightens, name strip fades up | 160 ms |
| Select a tile | ring grows, tile settles from 97 % | 240 ms |
| Selection changes | bar text slides 8 px and fades, picture cross-fades | 180 ms |
| Try on starts | Keep and Revert rise and fade in | 220 ms |
| Change category | grid fades out, tiles fade in 12 ms apart, 120 ms at most | 80 out, 140 in |
| Tile size | tiles glide to their new place (Ctrl + wheel) | 200 ms |
| Quick Look | window dims, card scales from 96 % | 180 ms, close 120 |
| New dot | one pulse 300 ms after the category is first shown | 1000 ms |

Never wait for an animation: a click, a key or a drag acts at once. With
reduce motion, only fades of 80 ms or less stay. Nothing moves under a
drag. Animations use egui's per-id value animation and repaint only while a
value moves.

## 11. Tooltips

One tip model shared with the world's hover tips (`hover_tips.rs`,
`ui/hover_tips.rs`), drawn in the same three forms (bubble, card, readout)
plus the Legacy text form, so the item card and the world tip look alike.
Four kinds:

| Kind | Content | Trigger |
|---|---|---|
| Item card | category and edition as the kicker, name with Worn or New, store logo and name, fit line, HUD line, rights line (No modify, No copy, No transfer), event line, footer "Shift · Details" | pointer rests on a card or a list row, 0.5 s, then instant between items, 80 ms fade |
| Item details | the card plus Creator (store and handle, or "no store linked"), Acquired, Folder, Permissions | Shift held while hovering |
| Hint | one line and a keycap ("Wear · Enter"), or one line explaining a chip ("Fits my body: shows only items made for the body you wear") | pointer rests on a button or chip; setting "mouse hints", off by default |
| Refusal | icon, what was refused, why in two sentences ("Tattoo layers always stay under Undershirt layers…") | a drag or an action the rules refuse |

The item card tooltip is on by default in Wardrobe mode; in List mode the
existing inventory tooltip setting governs it. Refusals never name an asset
id; a permissions refusal says "no copy" or "no transfer". Every string is a
`tr!` key in the three catalogues. The keycap drawing moves from the
movement window to `ui/widgets.rs` first, as `widgets::keycap`.

## 12. Creator tool

A window for creators: choose a folder (the box), fill the store block
(once per store, remembered), the pack block, and the items with their
category, subtype, kits, HUDs, picture and note, chosen from the folder's
contents. Export writes the card as a notecard into the folder through the
existing notecard upload path, or as a file. Items are embedded in the
notecard only when `asset_uuid_shown` allows it, which is the creator's own
full-permission items; otherwise the card carries names only. The logo
field offers the creator's own textures only.

## 13. Permissions and safety

- `asset_uuid_shown` guards every place an id could leave: the creator
  tool's embedding, the catalogue export, the Dev Toolbox views of the
  catalogue.
- A card's `logo`, the store's Marketplace link and its main store SLurl
  are the only outside references; the logo is fetched and rendered like a
  profile picture, the link goes through link trust, the SLurl through the
  viewer's SLurl handling with its teleport confirmation.
- Cards are parsed with caps (section 5); a malformed card is reported and
  ignored, never partially applied without the review sheet.
- The demo's own assets may show their ids, as everywhere; no other
  `AURORA_*` variable relaxes a check.
- A published build never serves the catalogue to anything.

## 14. Demo and tests

- `AURORA_DEMO_WARDROBE=<scenario>`: `cards` (a delivered folder with a
  trusted card, one with a stranger's card, one with a skipped line),
  `browse` (a few hundred items across stores, packs, events and kits, worn
  set and kits as in the mockups), `huds` (the five HUD rules), `fit` (a
  worn body and items for three kits). Documented in `docs/TESTING.md`.
- Unit tests: the card grammar (every clause, caps, comments, versions),
  name folding, the trust rule, precedence, the wearable-type mapping, the
  HUD rules in order, fit states per part (body, head, ears, a size, a kit
  declared by another item of the card, an unknown kit, several parts on
  one item), copies (folded names bind to one line, an edited copy keeps
  its binding and its own overrides, a renamed copy binds through "Same
  product as…", stacks count and choose the main copy), the category data
  file's translations, the catalogue read and write, undo.
- Theme guard: every new window and widget through `widgets::*`; the theme
  guard test covers the new files.
- Human validation in demo mode for the visuals and on the grid for a real
  delivery with a card.

## 15. Out of scope for the first release

- The Decoration tab (furniture, land impact, Placed, Rez or Find it, Copy
  only, tile sizes): designed in the same canvas, deferred; its category
  list is a second namespace of the same data file.
- Mirroring the Wardrobe as inventory links.
- Sharing a catalogue between users.

## 16. Module plan

- `world/wardrobe/`: catalogue and account files (`store.rs`), card parser
  (`card.rs`), matching and trust (`matching.rs`), categories and the alias
  table (`categories.rs`, `kits.rs`), HUD rules (`huds.rs`), fit
  (`fit.rs`), Auto-sort (`sort.rs`), undo.
- `ui/inventory/wardrobe/`: the rail, cards, chips, filter sheet, Browse by,
  selection bar, Quick Look, Try on, Wearing now, the review sheet, the
  Link a store dialog, the creator tool.
- `ui/inventory/smart.rs`: the new `Field` variants (category, subtype,
  kit, store, pack, event, has HUD) and the search scopes.
- `hover_tips.rs`: `TipKind::Item`, hints and refusals.
- `ARCHITECTURE.md` and `docs/TESTING.md` updated with the code; this page
  updated where the implementation departs from it.
