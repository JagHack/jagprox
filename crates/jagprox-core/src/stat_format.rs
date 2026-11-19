//! Port of `utils/statFormat.js`: one place that knows where each game's numbers live
//! in the Hypixel player object and how to print them. Used by `/sc`, `/psc`, queue
//! stats, tab tags, the launcher and `jagprox stats`.

use serde_json::Value;

use crate::constants::GameInfo;
use crate::formatter::strip_colors;
use crate::js::{field, to_fixed, to_locale};

/// `numerator / (denominator || 1)`.
pub fn ratio(numerator: f64, denominator: f64) -> f64 {
    numerator / if denominator == 0.0 { 1.0 } else { denominator }
}

fn fixed(value: f64) -> String {
    to_fixed(value, 2)
}

fn fmt(value: f64) -> String {
    to_locale(value)
}

/// Normalised numbers for one game mode. Missing values are 0.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModeStats {
    pub has_stats: bool,
    pub wins: f64,
    pub losses: f64,
    pub kills: f64,
    pub deaths: f64,
    pub final_kills: f64,
    pub final_deaths: f64,
    pub beds_broken: f64,
    pub beds_lost: f64,
    pub games: f64,
    pub score: f64,
    pub assists: f64,
    pub winstreak: f64,
    pub best_winstreak: f64,
    pub level: Option<f64>,
    pub level_text: Option<String>,
}

/// JS `a || b || 0` over already-read numbers.
fn first_nonzero(values: &[f64]) -> f64 {
    values.iter().copied().find(|v| *v != 0.0).unwrap_or(0.0)
}

pub fn get_mode_stats(player: Option<&Value>, game: &GameInfo) -> ModeStats {
    let all_stats = player
        .and_then(|p| p.get("stats"))
        .filter(|v| crate::js::truthy(Some(v)));
    let raw = all_stats.and_then(|s| s.get(game.api_name));
    let d = raw.filter(|v| crate::js::truthy(Some(v)));
    let a = player
        .and_then(|p| p.get("achievements"))
        .filter(|v| crate::js::truthy(Some(v)));
    let get = |key: &str| field(d, key);

    let mut s = ModeStats {
        has_stats: crate::js::truthy(raw),
        ..ModeStats::default()
    };

    match game.api_name {
        "Bedwars" => {
            s.wins = get("wins_bedwars");
            s.losses = get("losses_bedwars");
            s.kills = get("kills_bedwars");
            s.deaths = get("deaths_bedwars");
            s.final_kills = get("final_kills_bedwars");
            s.final_deaths = get("final_deaths_bedwars");
            s.beds_broken = get("beds_broken_bedwars");
            s.beds_lost = get("beds_lost_bedwars");
            s.winstreak = get("winstreak");
            s.best_winstreak = get("winstreak_best");
            let level = field(a, "bedwars_level");
            s.level = Some(level);
            s.level_text = Some(format!("{}✫", to_locale_plain(level)));
        }
        "SkyWars" => {
            s.wins = get("wins");
            s.losses = get("losses");
            s.kills = get("kills");
            s.deaths = get("deaths");
            s.winstreak = get("winstreak");
            s.best_winstreak = get("winstreak_best");
            let text = d
                .and_then(|d| d.get("levelFormatted"))
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .unwrap_or("0✫")
                .to_string();
            let digits: String = strip_colors(&text)
                .chars()
                .filter(char::is_ascii_digit)
                .collect();
            s.level = Some(parse_int(&digits));
            s.level_text = Some(text);
        }
        "Duels" => {
            let prefix = game.prefix;
            let key = |stat: &str| {
                if prefix.is_empty() {
                    stat.to_string()
                } else {
                    format!("{prefix}_{stat}")
                }
            };
            s.wins = get(&key("wins"));
            s.losses = get(&key("losses"));
            s.kills = get(&key("kills"));
            s.deaths = get(&key("deaths"));
            if prefix.is_empty() {
                s.winstreak = get("current_winstreak");
                s.best_winstreak =
                    first_nonzero(&[get("best_winstreak"), field(a, "duels_duels_win_streak")]);
            } else {
                // Current streak keys are missing when the streak is 0.
                s.winstreak = get(&format!("current_{prefix}_winstreak"));
                s.best_winstreak = first_nonzero(&[
                    get(&format!("best_{prefix}_winstreak")),
                    field(a, &format!("duels_{prefix}_winstreak")),
                ]);
            }
        }
        "Walls3" => {
            s.wins = get("wins");
            s.losses = get("losses");
            s.kills = get("kills");
            s.deaths = get("deaths");
            s.final_kills = get("final_kills");
            s.final_deaths = get("final_deaths");
        }
        "UHC" => {
            s.wins = get("wins");
            s.kills = get("kills");
            s.deaths = get("deaths");
            s.score = get("score");
        }
        "MurderMystery" => {
            s.wins = get("wins");
            s.kills = get("kills");
            s.games = get("games");
        }
        "BuildBattle" => {
            s.wins = get("wins");
            s.games = get("games_played");
            s.score = get("score");
        }
        "Pit" => {
            let pit = d.and_then(|d| d.get("pit_stats_ptl"));
            s.kills = field(pit, "kills");
            s.deaths = field(pit, "deaths");
        }
        "WoolGames" => {
            let ww = d
                .and_then(|d| d.get("wool_wars"))
                .and_then(|w| w.get("stats"));
            s.wins = field(ww, "wins");
            s.games = field(ww, "games_played");
            s.losses = (s.games - s.wins).max(0.0);
            s.kills = field(ww, "kills");
            s.assists = field(ww, "assists");
        }
        _ => {
            s.wins = get("wins");
            s.losses = get("losses");
            s.kills = get("kills");
            s.deaths = get("deaths");
        }
    }

    if s.winstreak > s.best_winstreak {
        s.best_winstreak = s.winstreak;
    }
    s
}

/// JS template-literal number formatting (`${n}`), no thousands separators.
fn to_locale_plain(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e21 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

/// JS `parseInt(digits, 10) || 0` for a digits-only string.
fn parse_int(digits: &str) -> f64 {
    if digits.is_empty() {
        0.0
    } else {
        digits.parse::<f64>().unwrap_or(0.0)
    }
}

/// Full breakdown, one string per chat line (used by `/sc` and the launcher).
pub fn format_stat_lines(s: &ModeStats, api_name: &str) -> Vec<String> {
    match api_name {
        "Bedwars" => vec![
            format!(
                "§dWins §5» §f{} §8| §dLosses §5» §f{}",
                fmt(s.wins),
                fmt(s.losses)
            ),
            format!(
                "§dKills §5» §f{} §8| §dDeaths §5» §f{}",
                fmt(s.kills),
                fmt(s.deaths)
            ),
            format!(
                "§dFinals §5» §f{} §8| §dF-Deaths §5» §f{}",
                fmt(s.final_kills),
                fmt(s.final_deaths)
            ),
            format!(
                "§dWLR §5» §f{} §8| §dKDR §5» §f{}",
                fixed(ratio(s.wins, s.losses)),
                fixed(ratio(s.kills, s.deaths))
            ),
            format!(
                "§dFKDR §5» §f{} §8| §dBBLR §5» §f{}",
                fixed(ratio(s.final_kills, s.final_deaths)),
                fixed(ratio(s.beds_broken, s.beds_lost))
            ),
        ],
        "Walls3" => vec![
            format!(
                "§dWins §5» §f{} §8| §dLosses §5» §f{}",
                fmt(s.wins),
                fmt(s.losses)
            ),
            format!(
                "§dFinals §5» §f{} §8| §dF-Deaths §5» §f{}",
                fmt(s.final_kills),
                fmt(s.final_deaths)
            ),
            format!(
                "§dWLR §5» §f{} §8| §dFKDR §5» §f{}",
                fixed(ratio(s.wins, s.losses)),
                fixed(ratio(s.final_kills, s.final_deaths))
            ),
        ],
        "UHC" => vec![
            format!(
                "§dWins §5» §f{} §8| §dScore §5» §f{}",
                fmt(s.wins),
                fmt(s.score)
            ),
            format!(
                "§dKills §5» §f{} §8| §dDeaths §5» §f{}",
                fmt(s.kills),
                fmt(s.deaths)
            ),
            format!("§dKDR §5» §f{}", fixed(ratio(s.kills, s.deaths))),
        ],
        "MurderMystery" => vec![
            format!(
                "§dGames §5» §f{} §8| §dWins §5» §f{}",
                fmt(s.games),
                fmt(s.wins)
            ),
            format!(
                "§dKills §5» §f{} §8| §dWin Rate §5» §f{}%",
                fmt(s.kills),
                fixed(ratio(s.wins, s.games) * 100.0)
            ),
        ],
        "BuildBattle" => vec![
            format!(
                "§dWins §5» §f{} §8| §dGames §5» §f{}",
                fmt(s.wins),
                fmt(s.games)
            ),
            format!(
                "§dScore §5» §f{} §8| §dWin Rate §5» §f{}%",
                fmt(s.score),
                fixed(ratio(s.wins, s.games) * 100.0)
            ),
        ],
        "Pit" => vec![
            format!(
                "§dKills §5» §f{} §8| §dDeaths §5» §f{}",
                fmt(s.kills),
                fmt(s.deaths)
            ),
            format!("§dKDR §5» §f{}", fixed(ratio(s.kills, s.deaths))),
        ],
        "WoolGames" => vec![
            format!(
                "§dWins §5» §f{} §8| §dGames §5» §f{}",
                fmt(s.wins),
                fmt(s.games)
            ),
            format!(
                "§dKills §5» §f{} §8| §dAssists §5» §f{}",
                fmt(s.kills),
                fmt(s.assists)
            ),
            format!("§dWLR §5» §f{}", fixed(ratio(s.wins, s.losses))),
        ],
        _ => vec![
            format!(
                "§dWins §5» §f{} §8| §dLosses §5» §f{}",
                fmt(s.wins),
                fmt(s.losses)
            ),
            format!(
                "§dKills §5» §f{} §8| §dDeaths §5» §f{}",
                fmt(s.kills),
                fmt(s.deaths)
            ),
            format!(
                "§dWLR §5» §f{} §8| §dKDR §5» §f{}",
                fixed(ratio(s.wins, s.losses)),
                fixed(ratio(s.kills, s.deaths))
            ),
        ],
    }
}

/// Hypixel hides winstreaks when the player turned the API setting off; a winning
/// player with no recorded best streak is the tell.
pub fn format_winstreak_line(s: &ModeStats) -> String {
    if s.best_winstreak == 0.0 && ratio(s.wins, s.losses) > 1.01 {
        return "§cWINSTREAK API DISABLED".into();
    }
    format!(
        "§dWinstreak §5» §f{} §8| §dBest §5» §f{}",
        fmt(s.winstreak),
        fmt(s.best_winstreak)
    )
}

pub fn has_winstreaks(api_name: &str) -> bool {
    matches!(api_name, "Bedwars" | "SkyWars" | "Duels")
}

/// One-line summary used in party/queue stat blocks.
pub fn format_compact_line(s: &ModeStats, api_name: &str) -> String {
    match api_name {
        "Bedwars" => format!(
            "§dFKDR §8» §f{} §8| §dWLR §8» §f{}",
            fixed(ratio(s.final_kills, s.final_deaths)),
            fixed(ratio(s.wins, s.losses))
        ),
        "SkyWars" | "Duels" => format!(
            "§dWLR §8» §f{} §8| §dKDR §8» §f{}",
            fixed(ratio(s.wins, s.losses)),
            fixed(ratio(s.kills, s.deaths))
        ),
        "Walls3" => format!(
            "§dWins §8» §f{} §8| §dFKDR §8» §f{}",
            fmt(s.wins),
            fixed(ratio(s.final_kills, s.final_deaths))
        ),
        "UHC" => format!(
            "§dWins §8» §f{} §8| §dKDR §8» §f{}",
            fmt(s.wins),
            fixed(ratio(s.kills, s.deaths))
        ),
        _ => format!(
            "§dWins §8» §f{} §8| §dKills §8» §f{}",
            fmt(s.wins),
            fmt(s.kills)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::game_mode;
    use serde_json::json;

    fn player() -> Value {
        json!({
            "achievements": { "bedwars_level": 123 },
            "stats": {
                "Bedwars": { "wins_bedwars": 10, "losses_bedwars": 5, "final_kills_bedwars": 30,
                             "final_deaths_bedwars": 10, "winstreak": 3, "winstreak_best": 2 },
                "Duels": { "classic_duel_wins": 8, "classic_duel_losses": 2, "current_classic_duel_winstreak": 4 }
            }
        })
    }

    #[test]
    fn reads_bed_wars_fields() {
        let p = player();
        let s = get_mode_stats(Some(&p), game_mode("bedwars").unwrap());
        assert!(s.has_stats);
        assert_eq!(s.final_kills, 30.0);
        assert_eq!(s.level_text.as_deref(), Some("123✫"));
        assert_eq!(
            s.best_winstreak, 3.0,
            "current streak above best counts as best"
        );
        assert!(format_compact_line(&s, "Bedwars").contains("FKDR §8» §f3.00"));
    }

    #[test]
    fn reads_prefixed_duels_fields() {
        let p = player();
        let s = get_mode_stats(Some(&p), game_mode("classic").unwrap());
        assert_eq!(s.wins, 8.0);
        assert_eq!(s.winstreak, 4.0);
    }

    #[test]
    fn missing_game_data_yields_zeros() {
        let p = player();
        let s = get_mode_stats(Some(&p), game_mode("skywars").unwrap());
        assert!(!s.has_stats);
        assert_eq!(s.wins, 0.0);
        assert_eq!(s.level, Some(0.0));
    }

    #[test]
    fn winstreak_line_flags_a_disabled_api() {
        let s = ModeStats {
            wins: 10.0,
            losses: 1.0,
            ..ModeStats::default()
        };
        assert_eq!(format_winstreak_line(&s), "§cWINSTREAK API DISABLED");
    }

    #[test]
    fn skywars_level_is_parsed_from_the_colored_text() {
        let p = json!({ "stats": { "SkyWars": { "levelFormatted": "§b17⋆" } } });
        let s = get_mode_stats(Some(&p), game_mode("sw").unwrap());
        assert_eq!(s.level, Some(17.0));
        assert_eq!(s.level_text.as_deref(), Some("§b17⋆"));
    }

    #[test]
    fn bed_wars_lines_use_thousands_separators() {
        let p = json!({ "stats": { "Bedwars": { "wins_bedwars": 12345, "losses_bedwars": 8 } } });
        let s = get_mode_stats(Some(&p), game_mode("bw").unwrap());
        let lines = format_stat_lines(&s, "Bedwars");
        assert_eq!(lines[0], "§dWins §5» §f12,345 §8| §dLosses §5» §f8");
        assert_eq!(lines[3], "§dWLR §5» §f1543.13 §8| §dKDR §5» §f0.00");
    }
}
