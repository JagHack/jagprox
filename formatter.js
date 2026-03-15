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

function log(message) {
  const timestamp = new Date().toLocaleTimeString();
  console.log(`[${timestamp}] ${message}`);
}

function extractText(chatObj, fullText = "") {
  if (typeof chatObj === "string") return chatObj;
  if (chatObj.text) fullText += chatObj.text;
  if (chatObj.extra)
    chatObj.extra.forEach((part) => (fullText += extractText(part)));
  return fullText;
}

function reconstructLegacyText(component) {
  if (typeof component === "string") {
    try {
      component = JSON.parse(component);
    } catch (e) {
      return component;
    }
  }

  let fullText = "";

  function processPart(part) {
    let text = "";
    if (part.color && colorMap[part.color]) {
      text += colorMap[part.color];
    }
    if (part.bold) text += colorMap["bold"];
    if (part.italic) text += colorMap["italic"];
    if (part.underline) text += colorMap["underline"];
    if (part.strikethrough) text += colorMap["strikethrough"];
    if (part.obfuscated) text += colorMap["obfuscated"];

    if (part.text) {
      text += part.text;
    }

    if (part.extra) {
      part.extra.forEach((extraPart) => {
        text += processPart(extraPart);
      });
    }
    return text;
  }

  return processPart(component);
}

function formatRank(player) {
  if (!player) return "§7";

  const rank = (player.monthlyPackageRank && player.monthlyPackageRank === "SUPERSTAR") ? "MVP_PLUS_PLUS" : (player.newPackageRank || player.rank || "NONE");

  const plusMap = {
    'RED': '§c',
    'GOLD': '§6',
    'LIGHT_PURPLE': '§d',
    'DARK_PURPLE': '§5',
    'DARK_BLUE': '§1',
    'DARK_GREEN': '§2',
    'DARK_AQUA': '§3',
    'DARK_RED': '§4',
    'DARK_GRAY': '§8',
    'GRAY': '§7',
    'BLUE': '§9',
    'GREEN': '§a',
    'AQUA': '§b',
    'YELLOW': '§e',
    'WHITE': '§f',
    'BLACK': '§0'
  };

  const plusColor = plusMap[player.rankPlusColor] || '§c';
  const mvpPlusPlusColor = plusMap[player.monthlyRankColor] || '§6';

  switch (rank) {
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
    case "YOUTUBE":
      return "§f[§cYT§f]";
    case "ADMIN":
      return "§c[ADMIN]";
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

  const rank = (player.monthlyPackageRank && player.monthlyPackageRank === "SUPERSTAR") ? "MVP_PLUS_PLUS" : (player.newPackageRank || player.rank || "NONE");
  
  const plusMap = {
    'RED': '§c',
    'GOLD': '§6',
    'LIGHT_PURPLE': '§d',
    'DARK_PURPLE': '§5',
    'DARK_BLUE': '§1',
    'DARK_GREEN': '§2',
    'DARK_AQUA': '§3',
    'DARK_RED': '§4',
    'DARK_GRAY': '§8',
    'GRAY': '§7',
    'BLUE': '§9',
    'GREEN': '§a',
    'AQUA': '§b',
    'YELLOW': '§e',
    'WHITE': '§f',
    'BLACK': '§0'
  };

  switch (rank) {
    case "MVP_PLUS_PLUS":
      return plusMap[player.monthlyRankColor] || '§6';
    case "MVP_PLUS":
    case "MVP":
      return "§b";
    case "VIP_PLUS":
    case "VIP":
      return "§a";
    case "YOUTUBE":
    case "ADMIN":
      return "§c";
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
  extractText,
  reconstructLegacyText,
  formatRank,
  getPlayerNameColor,
};
