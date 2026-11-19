//! Port of `utils/constants.js`. Tables are ordered slices because the original relies
//! on JS object order (help output, title lookup, "last match wins" mappings).

/// Address the proxy listens on by default (same as config.yml).
pub const DEFAULT_PROXY_PORT: u16 = 2107;

/// Chat lines Hypixel prints when a match actually begins (after the pre-game lobby).
pub const GAME_START_MESSAGES: &[&str] = &[
    "Protect your bed and destroy the enemy beds.",
    "Eliminate your opponents!",
    "Gather resources and equipment on your",
];

/// The 16 Minecraft chat colors and their RGB values.
pub const MC_COLORS: &[(&str, [u8; 3])] = &[
    ("§0", [0, 0, 0]),
    ("§1", [0, 0, 170]),
    ("§2", [0, 170, 0]),
    ("§3", [0, 170, 170]),
    ("§4", [170, 0, 0]),
    ("§5", [170, 0, 170]),
    ("§6", [255, 170, 0]),
    ("§7", [170, 170, 170]),
    ("§8", [85, 85, 85]),
    ("§9", [85, 85, 255]),
    ("§a", [85, 255, 85]),
    ("§b", [85, 255, 255]),
    ("§c", [255, 85, 85]),
    ("§d", [255, 85, 255]),
    ("§e", [255, 255, 85]),
    ("§f", [255, 255, 255]),
];

/// Closest chat color by Euclidean RGB distance (first one wins on a tie).
pub fn find_closest_minecraft_color(r: u8, g: u8, b: u8) -> &'static str {
    let mut best = MC_COLORS[0].0;
    let mut best_distance = u32::MAX;
    for (code, [cr, cg, cb]) in MC_COLORS {
        let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2) as u32;
        let distance = d(r, *cr) + d(g, *cg) + d(b, *cb);
        if distance < best_distance {
            best_distance = distance;
            best = code;
        }
    }
    best
}

/// Where a game mode's stats live in the Hypixel player object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameInfo {
    pub api_name: &'static str,
    pub display_name: &'static str,
    /// Duels sub-mode key prefix (`classic_duel`); empty for everything else.
    pub prefix: &'static str,
}

const fn game(api_name: &'static str, display_name: &'static str) -> GameInfo {
    GameInfo {
        api_name,
        display_name,
        prefix: "",
    }
}

const fn duel(display_name: &'static str, prefix: &'static str) -> GameInfo {
    GameInfo {
        api_name: "Duels",
        display_name,
        prefix,
    }
}

pub mod duels {
    use super::{GameInfo, duel};

    pub const GENERAL: GameInfo = duel("Duels", "");
    pub const CLASSIC: GameInfo = duel("Classic Duels", "classic_duel");
    pub const BRIDGE: GameInfo = duel("Bridge Duels", "bridge_duel");
    pub const UHC: GameInfo = duel("UHC Duels", "uhc_duel");
    pub const SKYWARS: GameInfo = duel("SkyWars Duels", "sw_duel");
    pub const SUMO: GameInfo = duel("Sumo Duels", "sumo_duel");
    pub const BOW: GameInfo = duel("Bow Duels", "bow_duel");
    pub const COMBO: GameInfo = duel("Combo Duels", "combo_duel");
    pub const OP: GameInfo = duel("OP Duels", "op_duel");
    pub const SPLEEF: GameInfo = duel("Spleef Duels", "spleef_duel");
    pub const BOXING: GameInfo = duel("Boxing Duels", "boxing_duel");
    pub const NDB: GameInfo = duel("NoDebuff Duels", "potion_duel");
    pub const BLITZ: GameInfo = duel("Blitz Duels", "blitz_duel");
    pub const BWRUSH: GameInfo = duel("BedWars Rush", "bw_duel_rush");
    pub const BWN: GameInfo = duel("BedWars (Normal)", "bw_duel_doubles");
    pub const MEGAWALLS: GameInfo = duel("Mega Walls Duels", "mega_walls_duel");
    pub const QUAKE: GameInfo = duel("Quake Duels", "quake_duel");
    pub const PARKOUR: GameInfo = duel("Parkour Duels", "parkour_duel");
    pub const BOW_SPLEEF: GameInfo = duel("Bow Spleef Duels", "bowspleef_duel");
}

/// User-facing game mode aliases (`/sc <mode>`), in the original order.
pub const GAME_MODES: &[(&str, GameInfo)] = &[
    ("arcade", game("Arcade", "Arcade")),
    ("arena", game("Arena", "Arena Brawl")),
    ("bedwars", game("Bedwars", "Bed Wars")),
    ("bw", game("Bedwars", "Bed Wars")),
    ("blitz", game("HungerGames", "Blitz SG")),
    ("sg", game("HungerGames", "Blitz SG")),
    ("buildbattle", game("BuildBattle", "Build Battle")),
    ("bb", game("BuildBattle", "Build Battle")),
    ("cvc", game("MCGO", "Cops and Crims")),
    ("crazywalls", game("TrueCombat", "Crazy Walls")),
    ("cw", game("TrueCombat", "Crazy Walls")),
    ("duels", duels::GENERAL),
    ("classic", duels::CLASSIC),
    ("classicduels", duels::CLASSIC),
    ("bridge", duels::BRIDGE),
    ("bridgeduels", duels::BRIDGE),
    ("uhc", duels::UHC),
    ("uhcduels", duels::UHC),
    ("skywarsduels", duels::SKYWARS),
    ("sumo", duels::SUMO),
    ("sumoduels", duels::SUMO),
    ("bow", duels::BOW),
    ("bowduels", duels::BOW),
    ("combo", duels::COMBO),
    ("comboduels", duels::COMBO),
    ("op", duels::OP),
    ("opduels", duels::OP),
    ("megawalls", game("Walls3", "Mega Walls")),
    ("mw", game("Walls3", "Mega Walls")),
    ("murder", game("MurderMystery", "Murder Mystery")),
    ("mm", game("MurderMystery", "Murder Mystery")),
    ("paintball", game("Paintball", "Paintball")),
    ("pit", game("Pit", "The Pit")),
    ("quake", game("Quake", "Quake")),
    ("quakecraft", game("Quake", "Quake")),
    ("skyclash", game("SkyClash", "SkyClash")),
    ("skywars", game("SkyWars", "SkyWars")),
    ("sw", game("SkyWars", "SkyWars")),
    ("smash", game("SuperSmash", "Smash Heroes")),
    ("speeduhc", game("SpeedUHC", "Speed UHC")),
    ("tnt", game("TNTGames", "TNT Games")),
    ("tkr", game("GingerBread", "Turbo Kart Racers")),
    ("turbokartracers", game("GingerBread", "Turbo Kart Racers")),
    ("uhcchampions", game("UHC", "UHC Champions")),
    ("vampirez", game("VampireZ", "VampireZ")),
    ("walls", game("Walls", "Walls")),
    ("warlords", game("Battleground", "Warlords")),
    ("wool", game("WoolGames", "Wool Wars")),
    ("woolwars", game("WoolGames", "Wool Wars")),
];

/// Looks up a game mode alias (exact, case-sensitive like the JS object).
pub fn game_mode(alias: &str) -> Option<&'static GameInfo> {
    GAME_MODES.iter().find(|(k, _)| *k == alias).map(|(_, g)| g)
}

/// `/q <alias>` shortcuts: (alias, display name, command).
pub const QUICK_QUEUE: &[(&str, &str, &str)] = &[
    (
        "voidless2",
        "Voidless Doubles",
        "/play bedwars_eight_two_voidless",
    ),
    (
        "voidless4",
        "Voidless 4v4v4v4",
        "/play bedwars_four_four_voidless",
    ),
    ("blitz", "Blitz Solo", "/play blitz_solo_normal"),
    ("blitzteams", "Blitz Teams", "/play blitz_teams_normal"),
    (
        "buildbattlepro",
        "Build Battle Pro",
        "/play build_battle_solo_pro",
    ),
    (
        "buildbattle",
        "Build Battle Solo",
        "/play build_battle_solo_normal",
    ),
    ("bb", "Build Battle Solo", "/play build_battle_solo_normal"),
    (
        "buildbattlesolo114",
        "Build Battle Solo (1.14+)",
        "/play build_battle_solo_normal_latest",
    ),
    (
        "bb114",
        "Build Battle Solo (1.14+)",
        "/play build_battle_solo_normal_latest",
    ),
    (
        "buildbattleteams",
        "Build Battle Teams",
        "/play build_battle_teams_normal",
    ),
    (
        "bbteams",
        "Build Battle Teams",
        "/play build_battle_teams_normal",
    ),
    ("cvc", "Cops and Crims", "/play mcgo"),
    ("doubles", "OP Duels (2v2)", "/play duels_op_doubles"),
    (
        "infection",
        "Murder Mystery Infection",
        "/play murder_infection",
    ),
    ("hole", "Hole in the Wall", "/play arcade_hole_in_the_wall"),
    ("hypixelsays", "Hypixel Says", "/play arcade_simon_says"),
    (
        "labblizzard",
        "SkyWars Laboratory (Blizzard)",
        "/play solo_insane_blizzard",
    ),
    (
        "labhunters",
        "SkyWars Laboratory (Hunters vs. Beasts)",
        "/play solo_insane_hunters_vs_beasts",
    ),
    (
        "labkill",
        "SkyWars Laboratory (Kill by Color)",
        "/play solo_insane_kill_by_color",
    ),
    (
        "lablava",
        "SkyWars Laboratory (Floor is Lava)",
        "/play solo_insane_floor_is_lava",
    ),
    (
        "lucky2",
        "Lucky Blocks Doubles",
        "/play bedwars_eight_two_lucky",
    ),
    (
        "lucky4",
        "Lucky Blocks 4v4v4v4",
        "/play bedwars_four_four_lucky",
    ),
    ("megadoubles", "Mega SkyWars Doubles", "/play mega_doubles"),
    ("megawars", "Mega Walls", "/play mw_standard"),
    ("megasolo", "Mega SkyWars Normal", "/play mega_normal"),
    ("miniwalls", "Mini Walls", "/play arcade_mini_walls"),
    ("parkour", "Hypixel Parkour", "/play duels_parkour_eight"),
    ("pit", "The Pit", "/play pit"),
    ("pixelparty", "Pixel Party", "/play arcade_pixel_party"),
    (
        "pixelpainters",
        "Pixel Painters",
        "/play arcade_pixel_painters",
    ),
    (
        "protosheep",
        "Sheep Wars",
        "/play prototype_sheepwars_two_six",
    ),
    ("quakesolo", "Quakecraft (Solo)", "/play quake_solo"),
    ("quaketeams", "Quakecraft (Teams)", "/play quake_teams"),
    ("ranked", "Ranked SkyWars", "/play ranked_normal"),
    ("rush2", "Rush Doubles", "/play bedwars_eight_two_rush"),
    ("rush4", "Rush 4v4v4v4", "/play bedwars_four_four_rush"),
    ("scuba", "Scuba Simulator", "/play arcade_scuba_simulator"),
    ("santa", "Santa Simulator", "/play arcade_santa_simulator"),
    ("santasays", "Santa Says", "/play arcade_santa_says"),
    ("sheepwars", "Sheep Wars", "/play wool_sheep_wars_two_six"),
    ("skyblock", "SkyBlock", "/play sb"),
    ("slime2", "Teams Slime", "/play teams_insane_slime"),
    ("slimesolo", "Solo Slime", "/play solo_insane_slime"),
    ("sumo", "Sumo Duels", "/play duels_sumo_duel"),
    ("sw1", "SkyWars Duels (1v1)", "/play duels_sw_duel"),
    ("sw2", "SkyWars Duels (2v2)", "/play duels_sw_doubles"),
    ("tnt", "TNT Run", "/play tnt_tntrun"),
    ("tntag", "TNT Tag", "/play tnt_tntag"),
    ("tntwizards", "TNT Wizards", "/play tnt_capture"),
    ("tkr", "Turbo Kart Racers", "/play tkr"),
    (
        "towerwars2",
        "Tower Wars (Doubles)",
        "/play prototype_towerwars_team_of_two",
    ),
    (
        "towerwars1",
        "Tower Wars (Solo)",
        "/play prototype_towerwars_solo",
    ),
    ("uhc1", "UHC Duels (1v1)", "/play duels_uhc_duel"),
    ("uhc2", "UHC Duels (2v2)", "/play duels_uhc_doubles"),
    ("uhc4", "UHC Duels (4v4)", "/play duels_uhc_four"),
    ("uhcmeetup", "UHC Deathmatch", "/play duels_uhc_meetup"),
    ("uhcevents", "UHC Events Mode", "/play uhc_events"),
    ("vampirez", "VampireZ", "/play vampirez"),
    ("bw4s", "Bedwars 4v4v4v4", "/play bedwars_four_four"),
    (
        "zombiesp",
        "Zombies (Prison)",
        "/play arcade_zombies_prison",
    ),
    (
        "zombiesd",
        "Zombies (Dead End)",
        "/play arcade_zombies_dead_end",
    ),
    (
        "zombiesb",
        "Zombies (Bad Blood)",
        "/play arcade_zombies_bad_blood",
    ),
    (
        "zombiesa",
        "Zombies (Alien Arcadium)",
        "/play arcade_zombies_alien_arcadium",
    ),
    ("throwout", "Throw Out", "/play arcade_throw_out"),
    ("walls", "The Walls", "/play walls"),
    (
        "hideprop",
        "Hide and Seek (Prop Hunt)",
        "/play arcade_hide_and_seek_prop_hunt",
    ),
    (
        "hideparty",
        "Hide and Seek (Party Pooper)",
        "/play arcade_hide_and_seek_party_pooper",
    ),
    (
        "halloween",
        "Halloween Simulator",
        "/play arcade_halloween_simulator",
    ),
    ("classic", "Classic Duels", "/play duels_classic_duel"),
    ("op1", "OP Duels (1v1)", "/play duels_op_duel"),
    ("combo", "Combo Duels", "/play duels_combo_duel"),
    ("boxing", "Boxing Duels", "/play duels_boxing_duel"),
    ("ndb", "NoDebuff Duels", "/play duels_potion_duel"),
    ("bridge", "Bridge Duels (1v1)", "/play duels_bridge_duel"),
    ("bowduel", "Bow Duels", "/play duels_bow_duel"),
    ("spleefduel", "Spleef Duels", "/play duels_spleef_duel"),
    (
        "megawallsduel",
        "Mega Walls Duels",
        "/play duels_mega_walls_duel",
    ),
    ("quakeduel", "Quake Duels", "/play duels_quake_duel"),
];

/// A Duels title tier: wins needed for division I, number of divisions, wins per division.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Division {
    pub name: &'static str,
    pub wins: u32,
    pub levels: u32,
    pub step: u32,
    pub color: &'static str,
}

/// Duels title tiers, lowest first (the title lookup depends on this order).
pub const DUELS_DIVISIONS: &[Division] = &[
    Division {
        name: "rookie",
        wins: 50,
        levels: 5,
        step: 10,
        color: "§8",
    },
    Division {
        name: "iron",
        wins: 100,
        levels: 5,
        step: 30,
        color: "§f",
    },
    Division {
        name: "gold",
        wins: 250,
        levels: 5,
        step: 50,
        color: "§6",
    },
    Division {
        name: "diamond",
        wins: 500,
        levels: 5,
        step: 100,
        color: "§b",
    },
    Division {
        name: "master",
        wins: 1000,
        levels: 5,
        step: 200,
        color: "§2",
    },
    Division {
        name: "legend",
        wins: 2000,
        levels: 5,
        step: 600,
        color: "§4",
    },
    Division {
        name: "grandmaster",
        wins: 5000,
        levels: 5,
        step: 1000,
        color: "§e",
    },
    Division {
        name: "godlike",
        wins: 10000,
        levels: 5,
        step: 3000,
        color: "§5",
    },
    Division {
        name: "celestial",
        wins: 25000,
        levels: 5,
        step: 5000,
        color: "§3",
    },
    Division {
        name: "divine",
        wins: 50000,
        levels: 5,
        step: 10000,
        color: "§d",
    },
    Division {
        name: "ascended",
        wins: 100000,
        levels: 5,
        step: 20000,
        color: "§c",
    },
];

pub const ROMAN_NUMERALS: &[&str] = &["I", "II", "III", "IV", "V"];

/// `/playercount <mode>` names to `/counts` DUELS mode keys.
pub const DUELS_PLAYER_COUNTS: &[(&str, &str)] = &[
    ("bed wars duel", "BEDWARS_TWO_ONE_DUELS"),
    ("bed wars", "BEDWARS_TWO_ONE_DUELS"),
    ("bw", "BEDWARS_TWO_ONE_DUELS"),
    ("bed rush duel", "BEDWARS_TWO_ONE_DUELS_RUSH"),
    ("bed rush", "BEDWARS_TWO_ONE_DUELS_RUSH"),
    ("blitz duel", "DUELS_BLITZ_DUEL"),
    ("blitz", "DUELS_BLITZ_DUEL"),
    ("bow duel", "DUELS_BOW_DUEL"),
    ("bow", "DUELS_BOW_DUEL"),
    ("bow spleef duel", "DUELS_BOWSPLEEF_DUEL"),
    ("bow spleef", "DUELS_BOWSPLEEF_DUEL"),
    ("boxing duel", "DUELS_BOXING_DUEL"),
    ("boxing", "DUELS_BOXING_DUEL"),
    ("bridge duel", "DUELS_BRIDGE_DUEL"),
    ("bridge", "DUELS_BRIDGE_DUEL"),
    ("classic duel", "DUELS_CLASSIC_DUEL"),
    ("classic", "DUELS_CLASSIC_DUEL"),
    ("combo duel", "DUELS_COMBO_DUEL"),
    ("combo", "DUELS_COMBO_DUEL"),
    ("mega walls duel", "DUELS_MW_DUEL"),
    ("mw", "DUELS_MW_DUEL"),
    ("nodebuff duel", "DUELS_POTION_DUEL"),
    ("nodebuff", "DUELS_POTION_DUEL"),
    ("potion", "DUELS_POTION_DUEL"),
    ("ndb", "DUELS_POTION_DUEL"),
    ("op duel", "DUELS_OP_DUEL"),
    ("op", "DUELS_OP_DUEL"),
    ("quake duel", "DUELS_QUAKE_DUEL"),
    ("quake", "DUELS_QUAKE_DUEL"),
    ("skywars duel", "DUELS_SW_DUEL"),
    ("sw", "DUELS_SW_DUEL"),
    ("spleef duel", "DUELS_SPLEEF_DUEL"),
    ("spleef", "DUELS_SPLEEF_DUEL"),
    ("sumo duel", "DUELS_SUMO_DUEL"),
    ("sumo", "DUELS_SUMO_DUEL"),
    ("uhc duel", "DUELS_UHC_DUEL"),
    ("uhc", "DUELS_UHC_DUEL"),
    ("bridge doubles", "DUELS_BRIDGE_DOUBLES"),
    ("bridge 3v3", "DUELS_BRIDGE_THREES"),
    ("bridge teams", "DUELS_BRIDGE_FOUR"),
    ("classic doubles", "DUELS_CLASSIC_DOUBLES"),
    ("op doubles", "DUELS_OP_DOUBLES"),
    ("skywars doubles", "DUELS_SW_DOUBLES"),
    ("uhc teams", "DUELS_UHC_FOUR"),
    ("duel arena", "DUELS_DUEL_ARENA"),
    ("arena", "DUELS_DUEL_ARENA"),
    ("parkour duels", "DUELS_PARKOUR_EIGHT"),
    ("parkour", "DUELS_PARKOUR_EIGHT"),
    ("uhc deathmatch", "DUELS_UHC_MEETUP"),
    ("meetup", "DUELS_UHC_MEETUP"),
];

/// Duels sub-mode display names to their stat key prefix (`/spread`, `/jtitles`).
pub const DUELS_STATS: &[(&str, &str)] = &[
    ("Bed Wars Duel", "bw_duel_doubles"),
    ("Bed Rush Duel", "bw_duel_rush"),
    ("Blitz Duel", "blitz_duel"),
    ("Bow Duel", "bow_duel"),
    ("Bow Spleef Duel", "bowspleef_duel"),
    ("Boxing Duel", "boxing_duel"),
    ("Bridge Duel", "bridge_duel"),
    ("Classic Duel", "classic_duel"),
    ("Combo Duel", "combo_duel"),
    ("Mega Walls Duel", "mega_walls_duel"),
    ("NoDebuff Duel", "potion_duel"),
    ("OP Duel", "op_duel"),
    ("Quake Duel", "quake_duel"),
    ("SkyWars Duel", "sw_duel"),
    ("Spleef Duel", "spleef_duel"),
    ("Sumo Duel", "sumo_duel"),
    ("UHC Duel", "uhc_duel"),
    ("Bridge Doubles", "bridge_doubles"),
    ("Bridge 3v3", "bridge_3v3"),
    ("Bridge Teams", "bridge_four"),
    ("Classic Doubles", "classic_doubles"),
    ("OP Doubles", "op_doubles"),
    ("SkyWars Doubles", "sw_doubles"),
    ("UHC Teams", "uhc_four"),
    ("Duel Arena", "duel_arena"),
    ("Parkour Duels", "parkour_eight"),
    ("UHC Deathmatch", "uhc_meetup"),
];

/// Duels title for `wins` in one mode: `(tier, roman division)`, or `None` below 50 wins.
pub fn duels_title(wins: f64) -> Option<(&'static Division, &'static str)> {
    let mut best = None;
    for division in DUELS_DIVISIONS {
        if wins >= f64::from(division.wins) {
            let index =
                ((wins - f64::from(division.wins)) / f64::from(division.step)).floor() as usize;
            let index = index.min(division.levels as usize - 1);
            best = Some((division, ROMAN_NUMERALS[index]));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_sizes_match_the_js_original() {
        assert_eq!(GAME_MODES.len(), 49);
        assert_eq!(QUICK_QUEUE.len(), 78);
        assert_eq!(DUELS_DIVISIONS.len(), 11);
        assert_eq!(DUELS_PLAYER_COUNTS.len(), 50);
        assert_eq!(DUELS_STATS.len(), 27);
    }

    #[test]
    fn closest_color() {
        assert_eq!(find_closest_minecraft_color(250, 250, 250), "§f");
        assert_eq!(find_closest_minecraft_color(10, 0, 160), "§1");
    }

    #[test]
    fn duels_titles() {
        assert_eq!(duels_title(49.0), None);
        let (d, roman) = duels_title(50.0).unwrap();
        assert_eq!((d.name, roman), ("rookie", "I"));
        let (d, roman) = duels_title(95.0).unwrap();
        assert_eq!((d.name, roman), ("rookie", "V"));
        let (d, roman) = duels_title(1450.0).unwrap();
        assert_eq!((d.name, roman), ("master", "III"));
        let (d, roman) = duels_title(10_000_000.0).unwrap();
        assert_eq!((d.name, roman), ("ascended", "V"));
    }
}
