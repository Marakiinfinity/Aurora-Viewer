//! Where the picture of a worn item looks on the avatar: the Wardrobe draws
//! the user's own avatar as it looks now, framed on the part an item
//! covers (the head for hair, the feet for shoes…). Aurora's own; the
//! Legacy Viewer has no Wardrobe. The drawing is in
//! `ui/inventory/wardrobe/look.rs`.

use super::categories::Tag;

/// One framing of the avatar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shot {
    Head,
    Neck,
    Torso,
    Waist,
    Legs,
    Feet,
    Hands,
    /// The tail, from behind.
    Tail,
    /// The wings, from behind.
    Wings,
    Body,
}

/// Every framing, built together.
pub const SHOTS: [Shot; 10] = [
    Shot::Head,
    Shot::Neck,
    Shot::Torso,
    Shot::Waist,
    Shot::Legs,
    Shot::Feet,
    Shot::Hands,
    Shot::Tail,
    Shot::Wings,
    Shot::Body,
];

/// Where a framing looks: the joints whose average is the centre, a lift
/// above it in metres, how far the camera stands (in units of the
/// preview's radius of 1 m: the picture shows about 0.79 × this in
/// height), and the turn of the camera about the avatar (0 = in front).
pub struct Aim {
    pub joints: &'static [&'static str],
    pub lift: f32,
    pub distance: f32,
    pub yaw: f32,
}

/// A little to the avatar's left, so that a face is not flat.
const THREE_QUARTER: f32 = 0.35;
const BEHIND: f32 = std::f32::consts::PI - 0.35;

impl Shot {
    /// The framing of an item, from its main category and subtype. A HUD
    /// is not on the avatar: it has none.
    pub fn of(tag: Option<&Tag>) -> Option<Shot> {
        let Some(tag) = tag else {
            return Some(Shot::Body);
        };
        let sub = |keys: &[&str]| keys.iter().any(|key| tag.has_subtype(key));
        Some(match tag.category() {
            "tops" => Shot::Torso,
            "bottoms" => Shot::Legs,
            "shoes" => Shot::Feet,
            "hair" | "heads" => Shot::Head,
            "huds" => return None,
            "body-parts" if sub(&["hands", "nails"]) => Shot::Hands,
            "body-parts" if sub(&["feet"]) => Shot::Feet,
            "body-parts" if sub(&["ears"]) => Shot::Head,
            "body-parts" if sub(&["tail"]) => Shot::Tail,
            "body-parts" if sub(&["wings"]) => Shot::Wings,
            "accessories" if sub(&["earrings", "piercing", "glasses", "hat"]) => Shot::Head,
            "accessories" if sub(&["necklace", "scarf"]) => Shot::Neck,
            "accessories" if sub(&["ring", "bracelet", "gloves", "watch"]) => Shot::Hands,
            "accessories" if sub(&["belt"]) => Shot::Waist,
            "accessories" if sub(&["bag"]) => Shot::Torso,
            _ => Shot::Body,
        })
    }

    pub fn aim(self) -> Aim {
        let aim = |joints, lift, distance, yaw| Aim {
            joints,
            lift,
            distance,
            yaw,
        };
        match self {
            Shot::Head => aim(&["mHead"], 0.06, 0.55, THREE_QUARTER),
            Shot::Neck => aim(&["mNeck"], -0.04, 0.8, THREE_QUARTER),
            Shot::Torso => aim(&["mChest", "mTorso"], 0.0, 1.35, THREE_QUARTER),
            Shot::Waist => aim(&["mPelvis"], 0.0, 0.9, THREE_QUARTER),
            Shot::Legs => aim(&["mPelvis", "mKneeLeft", "mKneeRight"], 0.0, 1.7, THREE_QUARTER),
            Shot::Feet => aim(&["mAnkleLeft", "mAnkleRight"], -0.03, 0.65, THREE_QUARTER),
            Shot::Hands => aim(&["mWristLeft"], -0.05, 0.6, THREE_QUARTER),
            Shot::Tail => aim(&["mPelvis"], -0.15, 1.6, BEHIND),
            Shot::Wings => aim(&["mChest"], 0.1, 2.4, BEHIND),
            Shot::Body => aim(&["mHead", "mAnkleLeft"], 0.04, 2.9, THREE_QUARTER),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(text: &str) -> Tag {
        match text.split_once('/') {
            Some((category, subtype)) => Tag::new(category, subtype),
            None => Tag(text.to_owned(), Vec::new()),
        }
    }

    #[test]
    fn each_category_looks_where_its_items_are_worn() {
        assert_eq!(Shot::of(Some(&tag("tops/hoodie"))), Some(Shot::Torso));
        assert_eq!(Shot::of(Some(&tag("bottoms/skirt"))), Some(Shot::Legs));
        assert_eq!(Shot::of(Some(&tag("shoes/boots"))), Some(Shot::Feet));
        assert_eq!(Shot::of(Some(&tag("hair/long"))), Some(Shot::Head));
        assert_eq!(Shot::of(Some(&tag("heads/mesh-head"))), Some(Shot::Head));
        assert_eq!(Shot::of(Some(&tag("full-body/dress"))), Some(Shot::Body));
        assert_eq!(Shot::of(Some(&tag("body-parts/mesh-body"))), Some(Shot::Body));
        assert_eq!(Shot::of(Some(&tag("body-parts/ears"))), Some(Shot::Head));
        assert_eq!(Shot::of(Some(&tag("body-parts/hands"))), Some(Shot::Hands));
        assert_eq!(Shot::of(Some(&tag("body-parts/tail"))), Some(Shot::Tail));
        assert_eq!(Shot::of(Some(&tag("body-parts/wings"))), Some(Shot::Wings));
        assert_eq!(Shot::of(Some(&tag("accessories/necklace"))), Some(Shot::Neck));
        assert_eq!(Shot::of(Some(&tag("accessories/glasses"))), Some(Shot::Head));
        assert_eq!(Shot::of(Some(&tag("accessories/ring"))), Some(Shot::Hands));
        assert_eq!(Shot::of(Some(&tag("accessories/belt"))), Some(Shot::Waist));
    }

    #[test]
    fn a_hud_has_no_picture_and_an_unsorted_item_shows_the_whole_avatar() {
        assert_eq!(Shot::of(Some(&tag("huds/animation-ao"))), None);
        assert_eq!(Shot::of(None), Some(Shot::Body));
        assert_eq!(Shot::of(Some(&tag("accessories"))), Some(Shot::Body));
    }

    #[test]
    fn every_framing_names_joints_and_a_camera_in_front_of_them() {
        for shot in SHOTS {
            let aim = shot.aim();
            assert!(!aim.joints.is_empty(), "{shot:?}");
            assert!(aim.distance > 0.3 && aim.distance < 4.0, "{shot:?}");
        }
    }
}
