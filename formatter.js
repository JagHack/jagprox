const colorMap = {
  black: "§0",
  dark_blue: "§1",
  dark_green: "§2",
  dark_aqua: "§3",
  dark_red: "§4",
  dark_purple: "§5",
  gold: "§6",
  gray: "§7",
  dark_gray: "§8",
  blue: "§9",
  green: "§a",
  aqua: "§b",
  red: "§c",
  light_purple: "§d",
  yellow: "§e",
  white: "§f",
  obfuscated: "§k",
  bold: "§l",
  strikethrough: "§m",
  underline: "§n",
  italic: "§o",
  reset: "§r",
};

// Hypixel's rankPlusColor / monthlyRankColor values.
const plusMap = {
  RED: "§c",
  GOLD: "§6",
  LIGHT_PURPLE: "§d",
  DARK_PURPLE: "§5",
  DARK_BLUE: "§1",
  DARK_GREEN: "§2",
  DARK_AQUA: "§3",
  DARK_RED: "§4",
  DARK_GRAY: "§8",
  GRAY: "§7",
  BLUE: "§9",
  GREEN: "§a",
  AQUA: "§b",
  YELLOW: "§e",
  WHITE: "§f",
  BLACK: "§0",
};

let debugEnabled = false;

function log(message) {
  const timestamp = new Date().toLocaleTimeString();
  console.log(`[${timestamp}] ${message}`);
}

function setDebug(enabled) {
  debugEnabled = !!enabled;
}

function debug(message) {
  if (debugEnabled) log(`DEBUG: ${message}`);
}

function stripColors(text) {
  return String(text).replace(/§./g, "");
}

// Plain text of a chat component (or string), including all `extra` children.
function extractText(chatObj) {
  if (chatObj === null || chatObj === undefined) return "";
  if (typeof chatObj === "string") return chatObj;
  let text = chatObj.text || "";
  if (Array.isArray(chatObj.extra)) {
    text += chatObj.extra.map(extractText).join("");
  }
  return text;
}

// Parses a JSON chat string to plain text with color codes removed.
// Returns null if the string isn't valid JSON.
function chatToCleanText(json) {
  try {
    return stripColors(extractText(JSON.parse(json))).trim();
  } catch (e) {
    return null;
  }
}

function reconstructLegacyText(component) {
  if (typeof component === "string") {
    try {
      component = JSON.parse(component);
    } catch (e) {
      return component;
    }
  }

  function processPart(part) {
    if (typeof part === "string") return part;
    let text = "";
    if (part.color && colorMap[part.color]) {
      text += colorMap[part.color];
    }
    if (part.bold) text += colorMap.bold;
    if (part.italic) text += colorMap.italic;
    if (part.underlined) text += colorMap.underline;
    if (part.strikethrough) text += colorMap.strikethrough;
    if (part.obfuscated) text += colorMap.obfuscated;

    if (part.text) {
      text += part.text;
    }

    if (Array.isArray(part.extra)) {
      part.extra.forEach((extraPart) => {
        text += processPart(extraPart);
      });
    }
    return text;
  }

  return processPart(component);
}

// The single source of truth for a Hypixel player's displayed rank.
// Staff ranks (player.rank) win over purchased ranks.
function getRank(player) {
  if (!player) return "NONE";
  if (player.rank && player.rank !== "NORMAL") return player.rank;
  if (player.monthlyPackageRank === "SUPERSTAR") return "MVP_PLUS_PLUS";
  if (player.newPackageRank && player.newPackageRank !== "NONE") return player.newPackageRank;
  if (player.packageRank && player.packageRank !== "NONE") return player.packageRank;
  return "NONE";
}

function formatRank(player) {
  if (!player) return "§7";

  const plusColor = plusMap[player.rankPlusColor] || "§c";
  const mvpPlusPlusColor = plusMap[player.monthlyRankColor] || "§6";

  switch (getRank(player)) {
    case "MVP_PLUS_PLUS":
      return `${mvpPlusPlusColor}[MVP${plusColor}++${mvpPlusPlusColor}]`;
    case "MVP_PLUS":
      return `§b[MVP${plusColor}+§b]`;
    case "MVP":
      return "§b[MVP]";
    case "VIP_PLUS":
      return "§a[VIP§6+§a]";
    case "VIP":
      return "§a[VIP]";
    case "YOUTUBER":
    case "YOUTUBE":
      return "§c[§fYOUTUBE§c]";
    case "ADMIN":
      return "§c[ADMIN]";
    case "GAME_MASTER":
      return "§2[GM]";
    case "MODERATOR":
      return "§2[MOD]";
    case "HELPER":
      return "§9[HELPER]";
    default:
      return "§7";
  }
}

function getPlayerNameColor(player) {
  if (!player) return "§7";

  switch (getRank(player)) {
    case "MVP_PLUS_PLUS":
      return plusMap[player.monthlyRankColor] || "§6";
    case "MVP_PLUS":
    case "MVP":
      return "§b";
    case "VIP_PLUS":
    case "VIP":
      return "§a";
    case "YOUTUBER":
    case "YOUTUBE":
    case "ADMIN":
      return "§c";
    case "GAME_MASTER":
    case "MODERATOR":
      return "§2";
    case "HELPER":
      return "§9";
    default:
      return "§7";
  }
}

module.exports = {
  log,
  setDebug,
  debug,
  stripColors,
  extractText,
  chatToCleanText,
  reconstructLegacyText,
  getRank,
  formatRank,
  getPlayerNameColor,
};
