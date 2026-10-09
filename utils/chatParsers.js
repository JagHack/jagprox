// Pure parsers for Hypixel chat lines. Input is always color-stripped plain text.

function cleanPlayerName(raw) {
    // "[MVP+] Name" -> "Name"; also drops trailing dots and stray whitespace.
    return raw.replace(/\[.*?\]\s?/g, '').trim().replace(/\.$/, '');
}

// "ONLINE: a, b, c" (from /who) -> ['a', 'b', 'c'], or null if the line isn't one.
function parseOnlineLine(clean) {
    if (!clean.startsWith('ONLINE: ')) return null;
    return clean
        .slice('ONLINE: '.length)
        .split(/,\s*/)
        .map(cleanPlayerName)
        .filter(Boolean);
}

// "Team #1: a, b" (team-mode /who output) -> ['a', 'b'], or null.
function parseTeamLine(clean) {
    if (!clean.startsWith('Team #')) return null;
    const colon = clean.indexOf(':');
    if (colon === -1) return [];
    return clean
        .slice(colon + 1)
        .split(/,\s*/)
        .map(cleanPlayerName)
        .filter(Boolean);
}

// "Party Members: [VIP] A ● B ●" / "Party Leader: [MVP+] C ●" -> names, or null.
function parsePartyLine(clean) {
    const match = clean.match(/^Party (?:Leader|Moderators|Members):\s*(.*)$/);
    if (!match) return null;
    return match[1]
        .split(/[●,]/)
        .map((part) => cleanPlayerName(part))
        .map((part) => part.split(/\s+/).pop())
        .filter(Boolean);
}

// End-of-game "... WINNER!" lines. Returns 'win', 'loss' or null.
//   "Name WINNER!"            (one word before)  -> win  (solo modes name the winner; that's us when we're tracking)
//   "Red Team WINNER!"        (2+ words before)  -> loss
//   "WINNER! Name"            (text after)       -> win
// Player chat (contains ':') and lobby/replay/spectator lines are ignored.
function detectGameResult(text) {
    const upper = text.replace(/§./g, '').toUpperCase().trim();
    if (!upper.includes('WINNER!')) return null;
    if (text.includes(':')) return null;
    if (upper.includes('LOBBY') || upper.includes('REPLAY') || upper.includes('SPECTATOR')) return null;

    const [beforeRaw, ...rest] = upper.split('WINNER!');
    const before = beforeRaw.trim();
    const after = rest.join('WINNER!').trim();

    if (after.length > 0) return 'win';
    const wordsBefore = before.split(' ').filter((w) => w.length > 0);
    if (wordsBefore.length >= 2) return 'loss';
    if (wordsBefore.length === 1) return 'win';
    return null;
}

module.exports = { cleanPlayerName, parseOnlineLine, parseTeamLine, parsePartyLine, detectGameResult };
