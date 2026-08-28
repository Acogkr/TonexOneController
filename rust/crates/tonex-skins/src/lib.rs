#![no_std]

use tonex_domain::PresetIndex;

pub const SKIN_COUNT: usize = 50;
pub const AUTO_SKIN_BYTE: u8 = u8::MAX;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SkinId(u8);

impl SkinId {
    pub const JCM: Self = Self(0);
    pub const SILVERFACE: Self = Self(1);
    pub const TONEX_AMP_BLACK: Self = Self(2);
    pub const AMP_5150: Self = Self(3);
    pub const AMPEG_CHROME: Self = Self(4);
    pub const FENDER_TWEED: Self = Self(5);
    pub const FENDER_HOT_ROD: Self = Self(6);
    pub const MESA_DUAL: Self = Self(7);
    pub const ELEGANT_BLUE: Self = Self(8);
    pub const MODERN_WHITE_PLEXI: Self = Self(9);
    pub const ROLAND_JAZZ: Self = Self(10);
    pub const ORANGE_OR120: Self = Self(11);
    pub const MODERN_BLACK_PLEXI: Self = Self(12);
    pub const FENDER_TWIN: Self = Self(13);
    pub const BA500: Self = Self(14);
    pub const MESA_MARK_WOOD: Self = Self(15);
    pub const MESA_MARK_V: Self = Self(16);
    pub const JTM: Self = Self(17);
    pub const DUMBLE: Self = Self(18);
    pub const JET_CITY: Self = Self(19);
    pub const AC30: Self = Self(20);
    pub const EVH: Self = Self(21);
    pub const TONEX_AMP_RED: Self = Self(22);
    pub const FRIEDMAN: Self = Self(23);
    pub const SUPRO: Self = Self(24);
    pub const DIEZEL: Self = Self(25);
    pub const WHITE_MODERN: Self = Self(26);
    pub const WOOD_AMP: Self = Self(27);
    pub const BIG_MUFF: Self = Self(28);
    pub const BOSS_BLACK: Self = Self(29);
    pub const BOSS_SILVER: Self = Self(30);
    pub const BOSS_YELLOW: Self = Self(31);
    pub const FUZZ_RED: Self = Self(32);
    pub const FUZZ_SILVER: Self = Self(33);
    pub const IBANEZ_BLUE: Self = Self(34);
    pub const IBANEZ_DARK_BLUE: Self = Self(35);
    pub const IBANEZ_GREEN: Self = Self(36);
    pub const IBANEZ_RED: Self = Self(37);
    pub const KLON_GOLD: Self = Self(38);
    pub const LIFE_PEDAL: Self = Self(39);
    pub const MORNING_GLORY: Self = Self(40);
    pub const MXR_DOUBLE_BLACK: Self = Self(41);
    pub const MXR_DOUBLE_RED: Self = Self(42);
    pub const MXR_SINGLE_BLACK: Self = Self(43);
    pub const MXR_SINGLE_GOLD: Self = Self(44);
    pub const MXR_SINGLE_GREEN: Self = Self(45);
    pub const MXR_SINGLE_ORANGE: Self = Self(46);
    pub const MXR_SINGLE_WHITE: Self = Self(47);
    pub const MXR_SINGLE_YELLOW: Self = Self(48);
    pub const RAT_YELLOW: Self = Self(49);

    pub const DEFAULT: Self = Self::TONEX_AMP_BLACK;

    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if (value as usize) < SKIN_COUNT {
            Some(Self(value))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn descriptor(self) -> &'static SkinDescriptor {
        &SKINS[self.0 as usize]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkinKind {
    Amp,
    Pedal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkinDescriptor {
    pub id: SkinId,
    pub kind: SkinKind,
    pub label: &'static str,
    pub asset: &'static str,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SkinSelection {
    #[default]
    Auto,
    Specific(SkinId),
}

impl SkinSelection {
    #[must_use]
    pub const fn encode(self) -> u8 {
        match self {
            Self::Auto => AUTO_SKIN_BYTE,
            Self::Specific(id) => id.get(),
        }
    }

    #[must_use]
    pub const fn decode(value: u8) -> Option<Self> {
        if value == AUTO_SKIN_BYTE {
            Some(Self::Auto)
        } else {
            match SkinId::new(value) {
                Some(id) => Some(Self::Specific(id)),
                None => None,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SkinMatchSource {
    Manual,
    ToneModelMetadata,
    PresetName,
    Fallback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedSkin {
    pub id: SkinId,
    pub source: SkinMatchSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetSkins {
    selections: [SkinSelection; tonex_domain::PRESET_COUNT],
}

impl Default for PresetSkins {
    fn default() -> Self {
        Self {
            selections: [SkinSelection::Auto; tonex_domain::PRESET_COUNT],
        }
    }
}

impl PresetSkins {
    #[must_use]
    pub const fn get(&self, preset: PresetIndex) -> SkinSelection {
        self.selections[preset.get() as usize]
    }

    pub fn set(&mut self, preset: PresetIndex, selection: SkinSelection) {
        self.selections[preset.get() as usize] = selection;
    }

    #[must_use]
    pub const fn selections(&self) -> &[SkinSelection; tonex_domain::PRESET_COUNT] {
        &self.selections
    }
}

/// Resolves a visual skin without pretending that a preset-name heuristic is
/// authoritative Tone Model metadata.
#[must_use]
pub fn resolve_skin(
    selection: SkinSelection,
    tone_model_name: Option<&[u8]>,
    preset_name: &[u8],
    _model_gain: f32,
) -> ResolvedSkin {
    if let SkinSelection::Specific(id) = selection {
        return ResolvedSkin {
            id,
            source: SkinMatchSource::Manual,
        };
    }
    if let Some(name) = tone_model_name
        && let Some(id) = infer_skin(name)
    {
        return ResolvedSkin {
            id,
            source: SkinMatchSource::ToneModelMetadata,
        };
    }
    if let Some(id) = infer_skin(preset_name) {
        return ResolvedSkin {
            id,
            source: SkinMatchSource::PresetName,
        };
    }
    ResolvedSkin {
        id: SkinId::DEFAULT,
        source: SkinMatchSource::Fallback,
    }
}

fn infer_skin(name: &[u8]) -> Option<SkinId> {
    for (id, keywords) in SKIN_KEYWORDS {
        if keywords
            .iter()
            .any(|keyword| contains_ascii_case_insensitive(name, keyword))
        {
            return Some(*id);
        }
    }
    None
}

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.windows(needle.len()).any(|window| {
            window
                .iter()
                .zip(needle)
                .all(|(left, right)| left.eq_ignore_ascii_case(right))
        })
}

const SKIN_KEYWORDS: &[(SkinId, &[&[u8]])] = &[
    (SkinId::AMP_5150, &[b"5150", b"6505"]),
    (SkinId::AC30, &[b"ac30", b"vox"]),
    (SkinId::AMPEG_CHROME, &[b"ampeg", b"svt"]),
    (SkinId::BA500, &[b"ba500"]),
    (SkinId::BIG_MUFF, &[b"big muff", b"bigmuff", b"muff"]),
    (SkinId::DIEZEL, &[b"diezel", b"vh4"]),
    (SkinId::DUMBLE, &[b"dumble", b"ods"]),
    (SkinId::EVH, &[b"evh"]),
    (SkinId::FENDER_HOT_ROD, &[b"hot rod", b"hotrod"]),
    (SkinId::FENDER_TWEED, &[b"tweed", b"bassman"]),
    (SkinId::FENDER_TWIN, &[b"twin reverb", b"fender twin"]),
    (SkinId::FRIEDMAN, &[b"friedman", b"be-100", b"pink taco"]),
    (
        SkinId::IBANEZ_GREEN,
        &[b"tube screamer", b"tubescreamer", b"ts9", b"ts808"],
    ),
    (SkinId::JCM, &[b"jcm", b"800", b"900"]),
    (SkinId::JET_CITY, &[b"jet city", b"jetcity"]),
    (SkinId::JTM, &[b"jtm"]),
    (SkinId::KLON_GOLD, &[b"klon", b"centaur"]),
    (SkinId::LIFE_PEDAL, &[b"life pedal"]),
    (SkinId::MESA_DUAL, &[b"dual rect", b"rectifier", b"recto"]),
    (SkinId::MESA_MARK_V, &[b"mark v", b"markv"]),
    (SkinId::MORNING_GLORY, &[b"morning glory"]),
    (SkinId::ORANGE_OR120, &[b"or120", b"orange"]),
    (SkinId::RAT_YELLOW, &[b"rat"]),
    (SkinId::ROLAND_JAZZ, &[b"jazz chorus", b"jc-120", b"jc120"]),
    (SkinId::SILVERFACE, &[b"silverface"]),
    (SkinId::SUPRO, &[b"supro"]),
    (SkinId::MODERN_BLACK_PLEXI, &[b"plexi", b"marshall"]),
    (SkinId::FUZZ_RED, &[b"fuzz"]),
];

pub const SKINS: [SkinDescriptor; SKIN_COUNT] = [
    skin(SkinId::JCM, SkinKind::Amp, "JCM", "jcm.png"),
    skin(
        SkinId::SILVERFACE,
        SkinKind::Amp,
        "Silverface",
        "slvrface.png",
    ),
    skin(
        SkinId::TONEX_AMP_BLACK,
        SkinKind::Amp,
        "TONEX Black",
        "tnxablk.png",
    ),
    skin(SkinId::AMP_5150, SkinKind::Amp, "5150", "5150.png"),
    skin(SkinId::AMPEG_CHROME, SkinKind::Amp, "Ampeg", "ampgchrm.png"),
    skin(SkinId::FENDER_TWEED, SkinKind::Amp, "Tweed", "fndrtwdg.png"),
    skin(
        SkinId::FENDER_HOT_ROD,
        SkinKind::Amp,
        "Hot Rod",
        "fndrhtrd.png",
    ),
    skin(
        SkinId::MESA_DUAL,
        SkinKind::Amp,
        "Dual Rectifier",
        "msbogdul.png",
    ),
    skin(
        SkinId::ELEGANT_BLUE,
        SkinKind::Amp,
        "Elegant Blue",
        "elgntblu.png",
    ),
    skin(
        SkinId::MODERN_WHITE_PLEXI,
        SkinKind::Amp,
        "White Plexi",
        "mdnwhplx.png",
    ),
    skin(
        SkinId::ROLAND_JAZZ,
        SkinKind::Amp,
        "Jazz Chorus",
        "roljazz.png",
    ),
    skin(SkinId::ORANGE_OR120, SkinKind::Amp, "OR120", "orngr120.png"),
    skin(
        SkinId::MODERN_BLACK_PLEXI,
        SkinKind::Amp,
        "Black Plexi",
        "mdnbkplx.png",
    ),
    skin(SkinId::FENDER_TWIN, SkinKind::Amp, "Twin", "fndrtwin.png"),
    skin(SkinId::BA500, SkinKind::Amp, "BA500", "ba500.png"),
    skin(
        SkinId::MESA_MARK_WOOD,
        SkinKind::Amp,
        "Mark Wood",
        "msamkwd.png",
    ),
    skin(SkinId::MESA_MARK_V, SkinKind::Amp, "Mark V", "mesamkv.png"),
    skin(SkinId::JTM, SkinKind::Amp, "JTM", "jtm.png"),
    skin(SkinId::DUMBLE, SkinKind::Amp, "D Style", "jbdumble.png"),
    skin(SkinId::JET_CITY, SkinKind::Amp, "Jet City", "jetcity.png"),
    skin(SkinId::AC30, SkinKind::Amp, "AC30", "ac30.png"),
    skin(SkinId::EVH, SkinKind::Amp, "EVH", "evh.png"),
    skin(
        SkinId::TONEX_AMP_RED,
        SkinKind::Amp,
        "TONEX Red",
        "tnxared.png",
    ),
    skin(SkinId::FRIEDMAN, SkinKind::Amp, "Friedman", "friedman.png"),
    skin(SkinId::SUPRO, SkinKind::Amp, "Supro", "supro.png"),
    skin(SkinId::DIEZEL, SkinKind::Amp, "Diezel", "diezel.png"),
    skin(
        SkinId::WHITE_MODERN,
        SkinKind::Amp,
        "White Modern",
        "whtmdrn.png",
    ),
    skin(SkinId::WOOD_AMP, SkinKind::Amp, "Wood Amp", "woodamp.png"),
    skin(SkinId::BIG_MUFF, SkinKind::Pedal, "Big Muff", "bigmuff.png"),
    skin(
        SkinId::BOSS_BLACK,
        SkinKind::Pedal,
        "Black Stomp",
        "bossblk.png",
    ),
    skin(
        SkinId::BOSS_SILVER,
        SkinKind::Pedal,
        "Silver Stomp",
        "bossslvr.png",
    ),
    skin(
        SkinId::BOSS_YELLOW,
        SkinKind::Pedal,
        "Yellow Stomp",
        "bossyel.png",
    ),
    skin(SkinId::FUZZ_RED, SkinKind::Pedal, "Red Fuzz", "fuzzred.png"),
    skin(
        SkinId::FUZZ_SILVER,
        SkinKind::Pedal,
        "Silver Fuzz",
        "fuzzslvr.png",
    ),
    skin(
        SkinId::IBANEZ_BLUE,
        SkinKind::Pedal,
        "Blue Drive",
        "ibnzblue.png",
    ),
    skin(
        SkinId::IBANEZ_DARK_BLUE,
        SkinKind::Pedal,
        "Dark Blue Drive",
        "ibnzdblu.png",
    ),
    skin(
        SkinId::IBANEZ_GREEN,
        SkinKind::Pedal,
        "Green Drive",
        "ibnzgrn.png",
    ),
    skin(
        SkinId::IBANEZ_RED,
        SkinKind::Pedal,
        "Red Drive",
        "ibnzred.png",
    ),
    skin(
        SkinId::KLON_GOLD,
        SkinKind::Pedal,
        "Gold Drive",
        "klongld.png",
    ),
    skin(
        SkinId::LIFE_PEDAL,
        SkinKind::Pedal,
        "Life Pedal",
        "lifepdl.png",
    ),
    skin(
        SkinId::MORNING_GLORY,
        SkinKind::Pedal,
        "Morning Glory",
        "mngglry.png",
    ),
    skin(
        SkinId::MXR_DOUBLE_BLACK,
        SkinKind::Pedal,
        "Double Black",
        "mxrdbbl.png",
    ),
    skin(
        SkinId::MXR_DOUBLE_RED,
        SkinKind::Pedal,
        "Double Red",
        "mxrdblrd.png",
    ),
    skin(
        SkinId::MXR_SINGLE_BLACK,
        SkinKind::Pedal,
        "Black Stomp",
        "mxrsngbk.png",
    ),
    skin(
        SkinId::MXR_SINGLE_GOLD,
        SkinKind::Pedal,
        "Gold Stomp",
        "mxrsnggd.png",
    ),
    skin(
        SkinId::MXR_SINGLE_GREEN,
        SkinKind::Pedal,
        "Green Stomp",
        "mxrsnggn.png",
    ),
    skin(
        SkinId::MXR_SINGLE_ORANGE,
        SkinKind::Pedal,
        "Orange Stomp",
        "mxrsgorg.png",
    ),
    skin(
        SkinId::MXR_SINGLE_WHITE,
        SkinKind::Pedal,
        "White Stomp",
        "mxrsngwh.png",
    ),
    skin(
        SkinId::MXR_SINGLE_YELLOW,
        SkinKind::Pedal,
        "Yellow Stomp",
        "mxrsngyl.png",
    ),
    skin(SkinId::RAT_YELLOW, SkinKind::Pedal, "RAT", "ratyell.png"),
];

const fn skin(
    id: SkinId,
    kind: SkinKind,
    label: &'static str,
    asset: &'static str,
) -> SkinDescriptor {
    SkinDescriptor {
        id,
        kind,
        label,
        asset,
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn legacy_catalog_order_is_stable_and_complete() {
        assert_eq!(SKINS.len(), SKIN_COUNT);
        for (index, descriptor) in SKINS.iter().enumerate() {
            assert_eq!(usize::from(descriptor.id.get()), index);
            assert!(!descriptor.label.is_empty());
            assert!(
                std::path::Path::new(descriptor.asset)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
            );
        }
    }

    #[test]
    fn selection_encoding_rejects_unknown_values() {
        for value in 0..u8::try_from(SKIN_COUNT).expect("skin count fits encoded byte") {
            assert_eq!(
                SkinSelection::decode(value),
                SkinId::new(value).map(SkinSelection::Specific)
            );
        }
        assert_eq!(
            SkinSelection::decode(AUTO_SKIN_BYTE),
            Some(SkinSelection::Auto)
        );
        assert_eq!(SkinSelection::decode(50), None);
    }

    #[test]
    fn resolver_preserves_evidence_source_and_priority() {
        let manual = resolve_skin(
            SkinSelection::Specific(SkinId::SUPRO),
            Some(b"Mesa Dual Rectifier"),
            b"JCM 800",
            8.0,
        );
        assert_eq!(manual.source, SkinMatchSource::Manual);
        assert_eq!(manual.id, SkinId::SUPRO);

        let metadata = resolve_skin(
            SkinSelection::Auto,
            Some(b"Mesa Dual Rectifier"),
            b"JCM 800",
            8.0,
        );
        assert_eq!(metadata.source, SkinMatchSource::ToneModelMetadata);
        assert_eq!(metadata.id, SkinId::MESA_DUAL);

        let inferred = resolve_skin(SkinSelection::Auto, None, b"JCM 800 Lead", 8.0);
        assert_eq!(inferred.source, SkinMatchSource::PresetName);
        assert_eq!(inferred.id, SkinId::JCM);

        let fallback = resolve_skin(SkinSelection::Auto, None, b"My preset", 2.0);
        assert_eq!(fallback.source, SkinMatchSource::Fallback);
        assert_eq!(fallback.id, SkinId::TONEX_AMP_BLACK);
    }
}
