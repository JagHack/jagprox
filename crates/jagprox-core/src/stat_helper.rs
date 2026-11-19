//! Port of `utils/stat-helper.js`: named stats for `/goal`.

use serde_json::Value;

use crate::js::{num, to_fixed};

enum StatPath {
    Single(&'static str),
    /// Numeric value parsed out of a formatted string (SkyWars level).
    Text(&'static str),
    /// `numerator / (denominator || 1)`, rounded to 2 decimals.
    Ratio(&'static str, &'static str),
}

use StatPath::{Ratio, Single, Text};

/// Goal-able stats per game, in the original order: (game, [(alias, path, name)]).
/// (alias, where the value lives, display name)
type StatDef = (&'static str, StatPath, &'static str);

static STAT_ALIASES: &[(&str, &[StatDef])] = &[
    (
        "bedwars",
        &[
            ("level", Single("achievements.bedwars_level"), "Level"),
            ("wins", Single("stats.Bedwars.wins_bedwars"), "Wins"),
            (
                "wlr",
                Ratio("stats.Bedwars.wins_bedwars", "stats.Bedwars.losses_bedwars"),
                "WLR",
            ),
            (
                "fkills",
                Single("stats.Bedwars.final_kills_bedwars"),
                "Final Kills",
            ),
            (
                "fdeaths",
                Single("stats.Bedwars.final_deaths_bedwars"),
                "Final Deaths",
            ),
            (
                "fkdr",
                Ratio(
                    "stats.Bedwars.final_kills_bedwars",
                    "stats.Bedwars.final_deaths_bedwars",
                ),
                "FKDR",
            ),
            (
                "beds",
                Single("stats.Bedwars.beds_broken_bedwars"),
                "Beds Broken",
            ),
            (
                "bblr",
                Ratio(
                    "stats.Bedwars.beds_broken_bedwars",
                    "stats.Bedwars.beds_lost_bedwars",
                ),
                "BBLR",
            ),
        ],
    ),
    (
        "skywars",
        &[
            ("level", Text("stats.SkyWars.levelFormatted"), "Level"),
            ("wins", Single("stats.SkyWars.wins"), "Wins"),
            (
                "wlr",
                Ratio("stats.SkyWars.wins", "stats.SkyWars.losses"),
                "WLR",
            ),
            ("kills", Single("stats.SkyWars.kills"), "Kills"),
            ("deaths", Single("stats.SkyWars.deaths"), "Deaths"),
            (
                "kdr",
                Ratio("stats.SkyWars.kills", "stats.SkyWars.deaths"),
                "KDR",
            ),
        ],
    ),
    (
        "duels",
        &[
            ("wins", Single("stats.Duels.wins"), "Wins"),
            (
                "wlr",
                Ratio("stats.Duels.wins", "stats.Duels.losses"),
                "WLR",
            ),
            ("kills", Single("stats.Duels.kills"), "Kills"),
            ("deaths", Single("stats.Duels.deaths"), "Deaths"),
            (
                "kdr",
                Ratio("stats.Duels.kills", "stats.Duels.deaths"),
                "KDR",
            ),
        ],
    ),
    (
        "megawalls",
        &[
            ("wins", Single("stats.Walls3.wins"), "Wins"),
            ("fkills", Single("stats.Walls3.final_kills"), "Final Kills"),
            (
                "fkdr",
                Ratio("stats.Walls3.final_kills", "stats.Walls3.final_deaths"),
                "FKDR",
            ),
        ],
    ),
    (
        "blitz",
        &[
            ("wins", Single("stats.HungerGames.wins"), "Wins"),
            ("kills", Single("stats.HungerGames.kills"), "Kills"),
        ],
    ),
    (
        "uhc",
        &[
            ("wins", Single("stats.UHC.wins"), "Wins"),
            ("kills", Single("stats.UHC.kills"), "Kills"),
            ("score", Single("stats.UHC.score"), "Score"),
        ],
    ),
];

/// A resolved stat: its value and display name.
#[derive(Debug, Clone, PartialEq)]
pub struct StatValue {
    pub value: f64,
    pub name: &'static str,
}

/// Games that have goal-able stats, in order.
pub fn goal_games() -> impl Iterator<Item = &'static str> {
    STAT_ALIASES.iter().map(|(game, _)| *game)
}

/// Stat aliases of one game, in order.
pub fn goal_stats(game: &str) -> Option<impl Iterator<Item = &'static str>> {
    STAT_ALIASES
        .iter()
        .find(|(g, _)| *g == game)
        .map(|(_, stats)| stats.iter().map(|(alias, _, _)| *alias))
}

/// Walks a dotted path such as `stats.Bedwars.wins_bedwars`.
pub fn get_property<'a>(obj: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(obj, |o, key| o.get(key))
}

pub fn get_stat_value(player: &Value, gamemode: &str, alias: &str) -> Option<StatValue> {
    let (_, stats) = STAT_ALIASES.iter().find(|(g, _)| *g == gamemode)?;
    let (_, path, name) = stats.iter().find(|(a, _, _)| *a == alias)?;

    let value = match path {
        Ratio(numerator, denominator) => {
            let n = num(get_property(player, numerator));
            let d = num(get_property(player, denominator));
            let ratio = n / if d == 0.0 { 1.0 } else { d };
            to_fixed(ratio, 2).parse().unwrap_or(0.0)
        }
        Text(p) => match get_property(player, p) {
            Some(Value::String(text)) => {
                let digits: String = text.chars().filter(char::is_ascii_digit).collect();
                digits.parse().unwrap_or(0.0)
            }
            other => num(other),
        },
        Single(p) => num(get_property(player, p)),
    };
    Some(StatValue { value, name })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn computes_ratios_and_returns_none_for_unknown_stats() {
        let player = json!({ "stats": { "Bedwars": { "final_kills_bedwars": 30, "final_deaths_bedwars": 10 } } });
        assert_eq!(
            get_stat_value(&player, "bedwars", "fkdr"),
            Some(StatValue {
                value: 3.0,
                name: "FKDR"
            })
        );
        assert_eq!(get_stat_value(&player, "nope", "fkdr"), None);
        assert_eq!(get_stat_value(&player, "bedwars", "nope"), None);
    }

    #[test]
    fn parses_numbers_out_of_formatted_levels() {
        let player = json!({ "stats": { "SkyWars": { "levelFormatted": "§b17⋆" } } });
        assert_eq!(
            get_stat_value(&player, "skywars", "level").unwrap().value,
            17.0
        );
    }
}
