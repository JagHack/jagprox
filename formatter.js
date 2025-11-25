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

function formatRank(rank) {
  switch (rank) {
    case "MVP_PLUS_PLUS":
      return "§6[MVP§c++§6]";
    case "MVP_PLUS":
      return "§b[MVP§a+§b]";
    case "MVP":
      return "§b[MVP]";
    case "VIP_PLUS":
      return "§a[VIP§6+§a]";
    case "VIP":
      return "§a[VIP]";
    default:
      return "§7";
  }
}

module.exports = {
  log,
  extractText,
  reconstructLegacyText,
  formatRank,
};
