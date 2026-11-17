//! Which HUDs belong to a Wardrobe item (docs/WARDROBE.md, section 8). Five
//! rules in order, the first that finds something wins; the user's links
//! and exclusions then correct the result. HUDs are added, never worn in
//! place of another (LLAppearanceMgr::wearItemOnAvatar with replace false,
//! indra/newview/llappearancemgr.cpp): that part is the caller's.

use super::categories::ASSET_OBJECT;
use super::matching::{fold, hud_stem, is_link, names_hud};
use super::store::Account;
use aurora_net::inventory::InvItem;
use uuid::Uuid;

/// The HUD attachment points (ATTACHMENT_ID_HUD_*: 31 to 38).
pub fn hud_point(point: u8) -> bool {
    (31..=38).contains(&point)
}

/// What the rules read around one item.
pub struct Around<'a> {
    /// The objects of the item's own folder.
    pub siblings: &'a [&'a InvItem],
    /// The objects of a `HUD` or `HUDs` subfolder of that folder.
    pub hud_folder: &'a [&'a InvItem],
    /// The folder holds one product: one creator, and one item once its
    /// copies and its HUDs are set aside.
    pub one_product: bool,
    /// HUD names the item's card declares (`hud-for`).
    pub declared: &'a [String],
}

/// Which rule found the HUDs, for the tests and the Dev Toolbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    Card,
    SameName,
    InFolder,
    Remembered,
    User,
    None,
}

fn objects<'a>(items: &'a [&'a InvItem], item: &InvItem) -> impl Iterator<Item = &'a InvItem> {
    items
        .iter()
        .copied()
        .filter(move |o| o.asset_type == ASSET_OBJECT && !is_link(o) && o.id != item.id)
}

/// The HUD items of `item`, and the rule that found them.
pub fn huds_of(item: &InvItem, around: &Around, account: &Account) -> (Vec<Uuid>, Rule) {
    let excluded = account.hud_excluded.get(&item.id).map(Vec::as_slice).unwrap_or_default();
    let linked = account.hud_links.get(&item.id).map(Vec::as_slice).unwrap_or_default();
    // a HUD has no HUD of its own
    let mut found: Vec<Uuid> = Vec::new();
    let mut rule = Rule::None;
    if !names_hud(&item.name) {
        let name = fold(&item.name);
        let steps: [(Rule, Box<dyn Fn(&InvItem) -> bool + '_>, bool); 4] = [
            (
                Rule::Card,
                Box::new(|o: &InvItem| {
                    let lower = o.name.trim().to_lowercase();
                    around
                        .declared
                        .iter()
                        .any(|d| d.trim().to_lowercase() == lower || fold(d) == fold(&o.name))
                }),
                true,
            ),
            (
                Rule::SameName,
                Box::new(|o: &InvItem| hud_stem(&o.name).as_deref() == Some(name.as_str())),
                true,
            ),
            (Rule::InFolder, Box::new(|o: &InvItem| names_hud(&o.name)), around.one_product),
            (
                Rule::Remembered,
                Box::new(|o: &InvItem| account.worn_huds.contains(&o.id)),
                around.one_product,
            ),
        ];
        for (step, test, allowed) in steps {
            if !allowed || (step == Rule::Card && around.declared.is_empty()) {
                continue;
            }
            let in_sub = matches!(step, Rule::Card | Rule::InFolder);
            let pool = objects(around.siblings, item).chain(objects(around.hud_folder, item).filter(|_| in_sub));
            found = pool.filter(|o| test(o)).map(|o| o.id).collect();
            if !found.is_empty() {
                rule = step;
                break;
            }
        }
    }
    found.retain(|id| !excluded.contains(id));
    if found.is_empty() && rule != Rule::None {
        rule = Rule::None;
    }
    for id in linked {
        if !found.contains(id) {
            found.push(*id);
            if rule == Rule::None {
                rule = Rule::User;
            }
        }
    }
    (found, rule)
}

/// The kind of a HUD as its name tells it, for the "N HUDs" menu: what the
/// HUD's name adds to the item's ("Sora head HUD – Emotes" → "Emotes").
pub fn kind(hud_name: &str, item_name: &str) -> Option<String> {
    let item: Vec<String> = fold(item_name).split(' ').map(str::to_owned).collect();
    let cleaned: String = hud_name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' })
        .collect();
    let rest: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|w| {
            let lower = w.to_lowercase();
            lower != "hud"
                && lower != "huds"
                && !item.contains(&lower)
                && !lower.chars().all(|c| c.is_ascii_digit())
                && !(lower.starts_with('v') && lower[1..].chars().all(|c| c.is_ascii_digit()) && lower.len() > 1)
        })
        .collect();
    (!rest.is_empty()).then(|| rest.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(id: u128, name: &str) -> InvItem {
        InvItem {
            id: Uuid::from_u128(id),
            name: name.into(),
            asset_type: ASSET_OBJECT,
            ..InvItem::default()
        }
    }

    fn around<'a>(siblings: &'a [&'a InvItem], hud_folder: &'a [&'a InvItem], one_product: bool, declared: &'a [String]) -> Around<'a> {
        Around {
            siblings,
            hud_folder,
            one_product,
            declared,
        }
    }

    #[test]
    fn the_rules_apply_in_order() {
        let head = object(1, "Sora head v3");
        let named = object(2, "Sora head [HUD]");
        let other = object(3, "Emotes HUD");
        let sub = object(4, "Skin applier HUD");
        let worn = object(5, "Controller");
        let all = [&head, &named, &other, &worn];
        let subfolder = [&sub];
        let mut account = Account::default();
        account.worn_huds.insert(worn.id);

        // 1. the card's line wins, even over the same name
        let declared = vec!["Emotes HUD".to_owned()];
        assert_eq!(
            huds_of(&head, &around(&all, &subfolder, true, &declared), &account),
            (vec![other.id], Rule::Card)
        );
        // 2. the same name plus HUD
        assert_eq!(
            huds_of(&head, &around(&all, &subfolder, true, &[]), &account),
            (vec![named.id], Rule::SameName)
        );
        // 3. HUD in the name, the folder and its HUD subfolder, one product only
        let rest = [&head, &other, &worn];
        assert_eq!(
            huds_of(&head, &around(&rest, &subfolder, true, &[]), &account),
            (vec![other.id, sub.id], Rule::InFolder)
        );
        assert_eq!(
            huds_of(&head, &around(&rest, &subfolder, false, &[]), &account),
            (vec![], Rule::None)
        );
        // 4. remembered: worn on a HUD point before
        let last = [&head, &worn];
        assert_eq!(
            huds_of(&head, &around(&last, &[], true, &[]), &account),
            (vec![worn.id], Rule::Remembered)
        );
        assert_eq!(
            huds_of(&head, &around(&last, &[], true, &[]), &Account::default()),
            (vec![], Rule::None)
        );
    }

    #[test]
    fn the_same_name_ignores_case_brackets_dashes_and_versions() {
        let head = object(1, "Sora Head");
        for name in [
            "Sora head [HUD]",
            "sora head HUD",
            "Sora head (HUD)",
            "Sora head – HUD",
            "SORA HEAD - HUD v2.1",
        ] {
            let hud = object(2, name);
            let all = [&head, &hud];
            assert_eq!(
                huds_of(&head, &around(&all, &[], false, &[]), &Account::default()).1,
                Rule::SameName,
                "{name}"
            );
        }
        let hud = object(2, "Sora body HUD");
        let all = [&head, &hud];
        assert_eq!(huds_of(&head, &around(&all, &[], false, &[]), &Account::default()).1, Rule::None);
    }

    #[test]
    fn the_users_choice_corrects_the_rules() {
        let head = object(1, "Sora head");
        let hud = object(2, "Sora head HUD");
        let extra = object(3, "Lash controller");
        let all = [&head, &hud, &extra];
        let mut account = Account::default();
        account.hud_excluded.insert(head.id, vec![hud.id]);
        assert_eq!(huds_of(&head, &around(&all, &[], false, &[]), &account), (vec![], Rule::None));
        account.hud_links.insert(head.id, vec![extra.id]);
        assert_eq!(
            huds_of(&head, &around(&all, &[], false, &[]), &account),
            (vec![extra.id], Rule::User)
        );
        // a HUD has no HUD of its own
        assert_eq!(
            huds_of(&hud, &around(&all, &[], true, &[]), &Account::default()),
            (vec![], Rule::None)
        );
    }

    #[test]
    fn a_huds_kind_is_what_its_name_adds() {
        assert_eq!(kind("Sora head HUD – Emotes", "Sora head").as_deref(), Some("Emotes"));
        assert_eq!(kind("Sora head [HUD] v2", "Sora head"), None);
        assert_eq!(kind("Colours HUD", "Band tee").as_deref(), Some("Colours"));
        assert!(hud_point(31) && hud_point(38) && !hud_point(30) && !hud_point(39));
    }
}
