import { NAME } from "../config.js";

const params = new URLSearchParams(location.search);
const id = params.get("id");
const element = (key) => document.getElementById(key);

function answer(choice) {
  chrome.runtime.sendMessage({ type: "ask-answer", id, choice, name: element("name").value });
}

function button(label, choice, primary) {
  const made = document.createElement("button");
  made.textContent = label;
  if (primary) made.className = "primary";
  made.addEventListener("click", () => answer(choice));
  element("actions").append(made);
}

if (params.get("kind") === "pair") {
  element("title").textContent = `Use this browser for ${NAME}?`;
  element("body").textContent = `An agent asked to pick a browser. Connect this one and its browser tools will act here, in their own tab group.`;
  element("name").hidden = false;
  element("name").value = params.get("name") || "";
  button("Not this one", "decline", false);
  button("Connect", "connect", true);
} else {
  element("title").textContent = `Allow ${NAME} on this site?`;
  element("body").textContent = `An agent wants to use ${params.get("tool") || "the browser"} on:`;
  element("host").textContent = params.get("host") || "";
  element("summary").textContent = params.get("summary") || "";
  button("Block", "never", false);
  button("Decline", "decline", false);
  button("Allow once", "once", false);
  button("Always allow", "always", true);
}
