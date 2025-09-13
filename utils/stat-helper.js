const statAliases = {
    'bedwars': {
        'level': { path: 'achievements.bedwars_level', name: 'Level' },
        'wins': { path: 'stats.Bedwars.wins_bedwars', name: 'Wins' },
        'wlr': { path: ['stats.Bedwars.wins_bedwars', 'stats.Bedwars.losses_bedwars'], name: 'WLR', fixed: 2 },
        'fkills': { path: 'stats.Bedwars.final_kills_bedwars', name: 'Final Kills' },
        'fdeaths': { path: 'stats.Bedwars.final_deaths_bedwars', name: 'Final Deaths' },
        'fkdr': { path: ['stats.Bedwars.final_kills_bedwars', 'stats.Bedwars.final_deaths_bedwars'], name: 'FKDR', fixed: 2 },
        'beds': { path: 'stats.Bedwars.beds_broken_bedwars', name: 'Beds Broken' },
        'bblr': { path: ['stats.Bedwars.beds_broken_bedwars', 'stats.Bedwars.beds_lost_bedwars'], name: 'BBLR', fixed: 2 },
    },
    'skywars': {
        'level': { path: 'stats.SkyWars.levelFormatted', name: 'Level', isString: true },
        'wins': { path: 'stats.SkyWars.wins', name: 'Wins' },
        'wlr': { path: ['stats.SkyWars.wins', 'stats.SkyWars.losses'], name: 'WLR', fixed: 2 },
        'kills': { path: 'stats.SkyWars.kills', name: 'Kills' },
        'deaths': { path: 'stats.SkyWars.deaths', name: 'Deaths' },
        'kdr': { path: ['stats.SkyWars.kills', 'stats.SkyWars.deaths'], name: 'KDR', fixed: 2 },
    },
    'duels': {
        'wins': { path: 'stats.Duels.wins', name: 'Wins' },
        'wlr': { path: ['stats.Duels.wins', 'stats.Duels.losses'], name: 'WLR', fixed: 2 },
        'kills': { path: 'stats.Duels.kills', name: 'Kills' },
        'deaths': { path: 'stats.Duels.deaths', name: 'Deaths' },
        'kdr': { path: ['stats.Duels.kills', 'stats.Duels.deaths'], name: 'KDR', fixed: 2 },
    },
    'megawalls': {
        'wins': { path: 'stats.Walls3.wins', name: 'Wins' },
        'fkills': { path: 'stats.Walls3.final_kills', name: 'Final Kills' },
        'fkdr': { path: ['stats.Walls3.final_kills', 'stats.Walls3.final_deaths'], name: 'FKDR', fixed: 2 },
    },
    'blitz': {
        'wins': { path: 'stats.HungerGames.wins', name: 'Wins' },
        'kills': { path: 'stats.HungerGames.kills', name: 'Kills' },
    },
    'uhc': {
        'wins': { path: 'stats.UHC.wins', name: 'Wins' },
        'kills': { path: 'stats.UHC.kills', name: 'Kills' },
        'score': { path: 'stats.UHC.score', name: 'Score' },
    }
};

function getProperty(obj, path) {
    return path.split('.').reduce((o, i) => (o ? o[i] : undefined), obj);
}

function getStatValue(playerObject, gamemode, alias) {
    const gameAliases = statAliases[gamemode];
    if (!gameAliases) return null;

    const statInfo = gameAliases[alias];
    if (!statInfo) return null;

    if (Array.isArray(statInfo.path)) {
        const numerator = getProperty(playerObject, statInfo.path[0]) || 0;
        const denominator = getProperty(playerObject, statInfo.path[1]) || 1;
        const value = numerator / denominator;
        return { value: statInfo.fixed ? parseFloat(value.toFixed(statInfo.fixed)) : value, name: statInfo.name };
    }

    const value = getProperty(playerObject, statInfo.path);

    if (statInfo.isString && typeof value === 'string') {
        const numericValue = parseInt(value.replace(/[^0-9]/g, ''));
        return { value: isNaN(numericValue) ? 0 : numericValue, name: statInfo.name };
    }

    return { value: value || 0, name: statInfo.name };
}

module.exports = { getStatValue, statAliases };