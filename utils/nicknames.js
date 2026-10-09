function escapeRegExp(text) {
    return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

// Matches `name` only as a whole Minecraft name (letters, digits, underscore),
// so "Bob" doesn't hit "Bobby".
function wholeNameRegex(name) {
    return new RegExp(`(?<![A-Za-z0-9_])${escapeRegExp(name)}(?![A-Za-z0-9_])`, 'g');
}

function replaceNamesInText(text, nicknames) {
    let result = text;
    for (const [realName, nickname] of Object.entries(nicknames)) {
        if (!realName || typeof nickname !== 'string') continue;
        if (!result.includes(realName)) continue;
        result = result.replace(wholeNameRegex(realName), () => nickname);
    }
    return result;
}

// Rewrites every real name in a chat component tree (in place).
function replaceNamesInComponent(component, nicknames) {
    if (!component || typeof component !== 'object') return;
    if (typeof component.text === 'string') {
        component.text = replaceNamesInText(component.text, nicknames);
    }
    if (Array.isArray(component.extra)) {
        component.extra.forEach((part) => replaceNamesInComponent(part, nicknames));
    }
}

module.exports = { escapeRegExp, replaceNamesInText, replaceNamesInComponent };
