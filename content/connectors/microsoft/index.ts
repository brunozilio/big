import { type Creds, type Language, fail, pick, text, tokenAlternative, tokenField, unknownAction } from "sdk/connectors/index.ts";
import { binding, email, files, messaging } from "sdk/connectors/contracts.ts";
import { type Api, apiOf, get } from "./graph.ts";
import { createEvent, listEvents } from "./calendar.ts";
import { downloadFile, list, search, sites } from "./drive.ts";
import { listMessages as listChatMessages, listChats, sendMessage } from "./chat.ts";
import { listMessages, message, reply, send } from "./mail.ts";

export { signIn } from "./login.ts";

type Ctx = { config: { graph: string } } & Record<string, any>;

export function provider() {
  return {
    name: "Microsoft 365",
    logo: "logo.svg",
    capabilities: ["email", "files", "messaging"],
    auth: { kind: "loopback" },
    actions: [...email(), ...files(), ...messaging()],
  };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Your Microsoft 365 account", "Tu cuenta de Microsoft 365", "Sua conta Microsoft 365"),
    group: "corporate",
    button: pick(language, "Sign in with Microsoft", "Entrar con Microsoft", "Entrar com a Microsoft"),
    site: pick(language, "Microsoft", "de Microsoft", "da Microsoft"),
    choose_title: pick(language, "Microsoft account", "Cuenta de Microsoft", "Conta Microsoft"),
    hidden: false,
    bindings: [
      binding("files", "application", pick(language, "SharePoint site", "Sitio de SharePoint", "Site do SharePoint"), { multiple: true, section: pick(language, "Documents", "Documentos", "Documentos"), order: 20 }),
    ],
  };
}

export function alternatives({ language }: { language: Language }) {
  const label = pick(language, "Microsoft Graph access token", "Token de acceso de Microsoft Graph", "Token de acesso do Microsoft Graph");
  const help = pick(
    language,
    "Paste a Microsoft Graph access token with the mail, files and chat scopes.",
    "Pega un token de acceso de Microsoft Graph con los scopes de correo, archivos y chat.",
    "Cole um token de acesso do Microsoft Graph com os escopos de e-mail, arquivos e chat.",
  );
  const way = pick(language, "Use an access token", "Usar un token de acceso", "Usar token de acesso");
  const site = pick(language, "Microsoft", "de Microsoft", "da Microsoft");
  return [tokenAlternative(language, "access_token", way, site, [tokenField("token", label, help, true)], "bearer")];
}

function dispatch(api: Api, action: string, args: Record<string, unknown>) {
  switch (action) {
    case "email.list_messages":
      return listMessages(api, args);
    case "email.message":
      return message(api, text(args, "message"));
    case "email.send":
      return send(api, args);
    case "email.reply":
      return reply(api, args);
    case "email.list_events":
      return listEvents(api, args);
    case "email.create_event":
      return createEvent(api, args);
    case "files.list":
      return list(api, args);
    case "files.search":
      return search(api, args);
    case "files.download":
      return downloadFile(api, args);
    case "files.sites":
      return sites(api, args);
    case "messaging.list_chats":
      return listChats(api, args);
    case "messaging.list_messages":
      return listChatMessages(api, args);
    case "messaging.send_message":
      return sendMessage(api, args);
    default:
      return unknownAction(action);
  }
}

export function call({ action, args, creds }: { action: string; args: Record<string, unknown>; creds: Creds }, ctx: Ctx) {
  return dispatch(apiOf(ctx, creds), action, args ?? {});
}

export function accounts({ creds }: { creds: Creds }, ctx: Ctx) {
  const me = get(apiOf(ctx, creds), "/me?$select=id,displayName,mail,userPrincipalName") as any;
  const id = String(me?.id ?? "");
  if (id === "") fail("auth", "Microsoft did not return the signed-in account");
  const mail = String(me?.mail ?? me?.userPrincipalName ?? "");
  const label = String(me?.displayName ?? mail ?? "Microsoft");
  return [{ id, label, settings: { shown: { account: mail } } }];
}
