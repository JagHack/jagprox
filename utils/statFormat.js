// One place that knows where each game's numbers live in the Hypixel player object
// and how to print them. Used by /sc, /psc, queue stats, tab tags and the launcher.

function ratio(numerator, denominator) {
    return (numerator || 0) / (denominator || 1);
}

function fmt(value) {
    return (value || 0).toLocaleString();
}

function fixed(value) {
    return value.toFixed(2);
}

// Normalised numbers for one game mode. Missing values are 0.
function getModeStats(player, gameInfo) {
    const allStats = (player && player.stats) || {};
    const d = allStats[gameInfo.apiName] || {};
    const a = (player && player.achievements) || {};
    const s = {
        hasStats: !!allStats[gameInfo.apiName],
        wins: 0, losses: 0, kills: 0, deaths: 0,
        finalKills: 0, finalDeaths: 0, bedsBroken: 0, bedsLost: 0,
        games: 0, score: 0, assists: 0,
        winstreak: 0, bestWinstreak: 0,
        level: null, levelText: null,
    };

    switch (gameInfo.apiName) {
        case 'Bedwars':
            s.wins = d.wins_bedwars || 0;
            s.losses = d.losses_bedwars || 0;
            s.kills = d.kills_bedwars || 0;
            s.deaths = d.deaths_bedwars || 0;
            s.finalKills = d.final_kills_bedwars || 0;
            s.finalDeaths = d.final_deaths_bedwars || 0;
            s.bedsBroken = d.beds_broken_bedwars || 0;
            s.bedsLost = d.beds_lost_bedwars || 0;
            s.winstreak = d.winstreak || 0;
            s.bestWinstreak = d.winstreak_best || 0;
            s.level = a.bedwars_level || 0;
            s.levelText = `${s.level}✫`;
            break;
        case 'SkyWars':
            s.wins = d.wins || 0;
            s.losses = d.losses || 0;
            s.kills = d.kills || 0;
            s.deaths = d.deaths || 0;
            s.winstreak = d.winstreak || 0;
            s.bestWinstreak = d.winstreak_best || 0;
            s.levelText = d.levelFormatted || '0✫';
            s.level = parseInt(s.levelText.replace(/§./g, '').replace(/[^0-9]/g, ''), 10) || 0;
            break;
        case 'Duels': {
            const prefix = gameInfo.prefix || '';
            const key = (stat) => (prefix ? `${prefix}_${stat}` : stat);
            s.wins = d[key('wins')] || 0;
            s.losses = d[key('losses')] || 0;
            s.kills = d[key('kills')] || 0;
            s.deaths = d[key('deaths')] || 0;
            if (prefix) {
                // Current streak keys are missing when the streak is 0.
                s.winstreak = d[`current_${prefix}_winstreak`] || 0;
                s.bestWinstreak = d[`best_${prefix}_winstreak`] || a[`duels_${prefix}_winstreak`] || 0;
            } else {
                s.winstreak = d.current_winstreak || 0;
                s.bestWinstreak = d.best_winstreak || a.duels_duels_win_streak || 0;
            }
            break;
        }
        case 'Walls3':
            s.wins = d.wins || 0;
            s.losses = d.losses || 0;
            s.kills = d.kills || 0;
            s.deaths = d.deaths || 0;
            s.finalKills = d.final_kills || 0;
            s.finalDeaths = d.final_deaths || 0;
            break;
        case 'UHC':
            s.wins = d.wins || 0;
            s.kills = d.kills || 0;
            s.deaths = d.deaths || 0;
            s.score = d.score || 0;
            break;
        case 'MurderMystery':
            s.wins = d.wins || 0;
            s.kills = d.kills || 0;
            s.games = d.games || 0;
            break;
        case 'BuildBattle':
            s.wins = d.wins || 0;
            s.games = d.games_played || 0;
            s.score = d.score || 0;
            break;
        case 'Pit': {
            const pit = d.pit_stats_ptl || {};
            s.kills = pit.kills || 0;
            s.deaths = pit.deaths || 0;
            break;
        }
        case 'WoolGames': {
            const ww = (d.wool_wars && d.wool_wars.stats) || {};
            s.wins = ww.wins || 0;
            s.games = ww.games_played || 0;
            s.losses = Math.max(0, s.games - s.wins);
            s.kills = ww.kills || 0;
            s.assists = ww.assists || 0;
            break;
        }
        default:
            s.wins = d.wins || 0;
            s.losses = d.losses || 0;
            s.kills = d.kills || 0;
            s.deaths = d.deaths || 0;
    }

    if (s.winstreak > s.bestWinstreak) s.bestWinstreak = s.winstreak;
    return s;
}

// Full breakdown, one string per chat line (used by /sc and the launcher).
function formatStatLines(s, apiName) {
    const lines = [];
    switch (apiName) {
        case 'Bedwars':
            lines.push(`§dWins §5» §f${fmt(s.wins)} §8| §dLosses §5» §f${fmt(s.losses)}`);
            lines.push(`§dKills §5» §f${fmt(s.kills)} §8| §dDeaths §5» §f${fmt(s.deaths)}`);
            lines.push(`§dFinals §5» §f${fmt(s.finalKills)} §8| §dF-Deaths §5» §f${fmt(s.finalDeaths)}`);
            lines.push(`§dWLR §5» §f${fixed(ratio(s.wins, s.losses))} §8| §dKDR §5» §f${fixed(ratio(s.kills, s.deaths))}`);
            lines.push(`§dFKDR §5» §f${fixed(ratio(s.finalKills, s.finalDeaths))} §8| §dBBLR §5» §f${fixed(ratio(s.bedsBroken, s.bedsLost))}`);
            break;
        case 'Walls3':
            lines.push(`§dWins §5» §f${fmt(s.wins)} §8| §dLosses §5» §f${fmt(s.losses)}`);
            lines.push(`§dFinals §5» §f${fmt(s.finalKills)} §8| §dF-Deaths §5» §f${fmt(s.finalDeaths)}`);
            lines.push(`§dWLR §5» §f${fixed(ratio(s.wins, s.losses))} §8| §dFKDR §5» §f${fixed(ratio(s.finalKills, s.finalDeaths))}`);
            break;
        case 'UHC':
            lines.push(`§dWins §5» §f${fmt(s.wins)} §8| §dScore §5» §f${fmt(s.score)}`);
            lines.push(`§dKills §5» §f${fmt(s.kills)} §8| §dDeaths §5» §f${fmt(s.deaths)}`);
            lines.push(`§dKDR §5» §f${fixed(ratio(s.kills, s.deaths))}`);
            break;
        case 'MurderMystery':
            lines.push(`§dGames §5» §f${fmt(s.games)} §8| §dWins §5» §f${fmt(s.wins)}`);
            lines.push(`§dKills §5» §f${fmt(s.kills)} §8| §dWin Rate §5» §f${(ratio(s.wins, s.games) * 100).toFixed(2)}%`);
            break;
        case 'BuildBattle':
            lines.push(`§dWins §5» §f${fmt(s.wins)} §8| §dGames §5» §f${fmt(s.games)}`);
            lines.push(`§dScore §5» §f${fmt(s.score)} §8| §dWin Rate §5» §f${(ratio(s.wins, s.games) * 100).toFixed(2)}%`);
            break;
        case 'Pit':
            lines.push(`§dKills §5» §f${fmt(s.kills)} §8| §dDeaths §5» §f${fmt(s.deaths)}`);
            lines.push(`§dKDR §5» §f${fixed(ratio(s.kills, s.deaths))}`);
            break;
        case 'WoolGames':
            lines.push(`§dWins §5» §f${fmt(s.wins)} §8| §dGames §5» §f${fmt(s.games)}`);
            lines.push(`§dKills §5» §f${fmt(s.kills)} §8| §dAssists §5» §f${fmt(s.assists)}`);
            lines.push(`§dWLR §5» §f${fixed(ratio(s.wins, s.losses))}`);
            break;
        default:
            lines.push(`§dWins §5» §f${fmt(s.wins)} §8| §dLosses §5» §f${fmt(s.losses)}`);
            lines.push(`§dKills §5» §f${fmt(s.kills)} §8| §dDeaths §5» §f${fmt(s.deaths)}`);
            lines.push(`§dWLR §5» §f${fixed(ratio(s.wins, s.losses))} §8| §dKDR §5» §f${fixed(ratio(s.kills, s.deaths))}`);
    }
    return lines;
}

// Hypixel hides winstreaks when the player turned the API setting off; a winning
// player with no recorded best streak is the tell.
function formatWinstreakLine(s) {
    if (s.bestWinstreak === 0 && ratio(s.wins, s.losses) > 1.01) {
        return '§cWINSTREAK API DISABLED';
    }
    return `§dWinstreak §5» §f${fmt(s.winstreak)} §8| §dBest §5» §f${fmt(s.bestWinstreak)}`;
}

function hasWinstreaks(apiName) {
    return apiName === 'Bedwars' || apiName === 'SkyWars' || apiName === 'Duels';
}

// One-line summary used in party/queue stat blocks.
function formatCompactLine(s, apiName) {
    switch (apiName) {
        case 'Bedwars':
            return `§dFKDR §8» §f${fixed(ratio(s.finalKills, s.finalDeaths))} §8| §dWLR §8» §f${fixed(ratio(s.wins, s.losses))}`;
        case 'SkyWars':
        case 'Duels':
            return `§dWLR §8» §f${fixed(ratio(s.wins, s.losses))} §8| §dKDR §8» §f${fixed(ratio(s.kills, s.deaths))}`;
        case 'Walls3':
            return `§dWins §8» §f${fmt(s.wins)} §8| §dFKDR §8» §f${fixed(ratio(s.finalKills, s.finalDeaths))}`;
        case 'UHC':
            return `§dWins §8» §f${fmt(s.wins)} §8| §dKDR §8» §f${fixed(ratio(s.kills, s.deaths))}`;
        default:
            return `§dWins §8» §f${fmt(s.wins)} §8| §dKills §8» §f${fmt(s.kills)}`;
    }
}

module.exports = { ratio, getModeStats, formatStatLines, formatWinstreakLine, hasWinstreaks, formatCompactLine };
