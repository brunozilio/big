import { type Creds, type Language, fail, pick, unknownAction } from "sdk/connectors/index.ts";
import { assistant } from "sdk/connectors/contracts.ts";
import { type Asked, ask } from "./chat.ts";
import { heldBy } from "./login.ts";

export { signIn } from "./login.ts";

export function provider() {
  return { name: "Amazon Quick", logo: "logo.svg", capabilities: ["assistant"], auth: { kind: "loopback" }, actions: assistant() };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Your Amazon Quick account", "Tu cuenta de Amazon Quick", "Sua conta Amazon Quick"),
    group: "assistant",
    button: pick(language, "Sign in with Amazon Quick", "Entrar con Amazon Quick", "Entrar com o Amazon Quick"),
    site: pick(language, "Amazon Quick", "de Amazon Quick", "do Amazon Quick"),
    choose_title: pick(language, "Amazon Quick account", "Cuenta de Amazon Quick", "Conta do Amazon Quick"),
    hidden: false,
  };
}

function signedIn(creds: Creds) {
  return heldBy(creds.token) ?? fail("auth", "sign in to Amazon Quick again");
}

export function call({ action, args, creds }: { action: string; args: Record<string, unknown>; creds: Creds }, ctx) {
  if (action === "assistant.ask") return ask(ctx, signedIn(creds), (args ?? {}) as Asked);
  return unknownAction(action);
}

export function accounts({ creds }: { creds: Creds }) {
  const held = signedIn(creds);
  const account = held.account === "" ? held.tenant_url.replace(/^https:\/\//, "").split(".")[0] : held.account;
  return [{ id: `quick-${account}`, label: `Amazon Quick · ${account}`, settings: {} }];
}
