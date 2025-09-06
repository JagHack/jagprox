function log(message) {
    const timestamp = new Date().toLocaleTimeString();
    console.log(`[${timestamp}] ${message}`);
}

function extractText(chatObj, fullText = "") {
    if (typeof chatObj === 'string') return chatObj;
    if (chatObj.text) fullText += chatObj.text;
    if (chatObj.extra) chatObj.extra.forEach(part => fullText += extractText(part));
    return fullText;
}

function formatRank(rank) {
    switch (rank) {
        case "MVP_PLUS_PLUS": return "§6[MVP§c++§6]";
        case "MVP_PLUS": return "§b[MVP§a+§b]";
        case "MVP": return "§a[MVP]";
        case "VIP_PLUS": return "§a[VIP§6+§a]";
        case "VIP": return "§a[VIP]";
        default: return "§7";
    }
}

module.exports = {
    log,
    extractText,
    formatRank
};