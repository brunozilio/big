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

function pairing() {
  element("title").textContent = `Use this browser for ${NAME}?`;
  element("body").textContent = "An agent asked to pick a browser. Connect this one and its browser tools will act here, in their own tab group.";
  element("name").value = params.get("name") || "";
  button("Not this one", "decline", false);
  button("Connect", "connect", true);
}

function relaying() {
  element("title").textContent = `${NAME} sign-in`;
  element("body").textContent = `Let ${params.get("origin") || ""} receive your sign-ins that come back to this computer?`;
  element("name").hidden = true;
  button("Not now", "decline", false);
  button("Allow", "allow", true);
}

if (params.get("kind") === "relay") relaying();
else pairing();
