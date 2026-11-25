import { type Creds, type Language, decode, fetchJson, form, missing, optionalText, pick, setting, tokenAlternative, tokenField, unknownAction } from "sdk/connectors/index.ts";
import { binding, cloud, options } from "sdk/connectors/contracts.ts";
import { caller, keysOf, type Keys } from "./sts.ts";
import { isoSeconds } from "./login.ts";

export { signIn } from "./login.ts";
export { sso } from "./sso.ts";

const PAGES = 20;

const regionOf = (ctx, creds: Creds) => setting(creds, "region") ?? ctx.config.region;

export const portal = (region: string) => `https://portal.sso.${region}.amazonaws.com`;

export function spaced(account: string): string {
  if (!/^\d{12}$/.test(account)) return account;
  return `${account.slice(0, 4)}-${account.slice(4, 8)}-${account.slice(8)}`;
}

export function provider() {
  return {
    name: "AWS",
    logo: "logo.svg",
    capabilities: ["cloud"],
    auth: { kind: "loopback" },
    actions: [...cloud().filter((action) => action.name !== "cloud.last_deployment"), options("cloud")],
  };
}

export function presentation({ language }: { language: Language }) {
  return {
    description: pick(language, "Your AWS account", "Tu cuenta de AWS", "Sua conta AWS"),
    group: "cloud",
    button: pick(language, "Sign in with AWS", "Entrar con AWS", "Entrar com AWS"),
    site: pick(language, "AWS", "de AWS", "da AWS"),
    choose_title: pick(language, "Account and role", "Cuenta y rol", "Conta e papel"),
    hidden: false,
    bindings: [binding("cloud", "application", pick(language, "AWS account and role", "Cuenta y rol de AWS", "Conta e papel da AWS"))],
  };
}

function accessKeys(language: Language) {
  const id = tokenField("accessKeyId", "Access key ID", "", false);
  const help = pick(
    language,
    "In IAM, open the user, Security credentials, and create an access key.",
    "En IAM, abre el usuario, Security credentials, y crea una access key.",
    "No IAM, abra o usuário, Security credentials, e crie uma access key.",
  );
  const secret = tokenField("secretAccessKey", "Secret access key", help, true);
  const way = pick(language, "Use an access key", "Usar una access key", "Usar access key");
  return tokenAlternative(language, "access_keys", way, pick(language, "AWS", "de AWS", "da AWS"), [id, secret], "json");
}

export function alternatives({ language }: { language: Language }) {
  const sso = {
    key: "sso",
    label: pick(language, "Use company SSO", "Usar el SSO de la empresa", "Usar SSO da empresa"),
    button: pick(language, "Continue", "Continuar", "Continuar"),
    site: pick(language, "AWS IAM Identity Center", "de AWS IAM Identity Center", "do AWS IAM Identity Center"),
    auth: { kind: "steps", using: "sso" },
  };
  return [sso, accessKeys(language)];
}

function portalGet(ctx, creds: Creds, path: string, query: [string, string][]) {
  const pairs = form(query);
  const url = `${portal(regionOf(ctx, creds))}${path}${pairs === "" ? "" : `?${pairs}`}`;
  return fetchJson(ctx, { method: "GET", url, headers: { "x-amz-sso_bearer_token": creds.token } });
}

function paged(ctx, creds: Creds, path: string, query: [string, string][], field: string) {
  const found = [];
  let token: string | undefined;
  for (let page = 0; page < PAGES; page += 1) {
    const pairs: [string, string][] = [...query, ["max_result", "100"]];
    if (token !== undefined) pairs.push(["next_token", token]);
    const answer = portalGet(ctx, creds, path, pairs);
    found.push(...(Array.isArray(answer?.[field]) ? answer[field] : []));
    token = typeof answer?.nextToken === "string" && answer.nextToken !== "" ? answer.nextToken : undefined;
    if (token === undefined) break;
  }
  return found;
}

const textOf = (value, name: string) => (typeof value?.[name] === "string" ? value[name] : "");

function ssoAccounts(ctx, creds: Creds) {
  return paged(ctx, creds, "/assignment/accounts", [], "accountList").map((account) => {
    const id = textOf(account, "accountId");
    const roles = paged(ctx, creds, "/assignment/roles", [["account_id", id]], "roleList").map((role) => textOf(role, "roleName"));
    return { id, name: textOf(account, "accountName"), email: textOf(account, "emailAddress"), roles };
  });
}

function accountsAndRoles(ctx, creds: Creds) {
  const keys = keysOf(creds.token);
  if (keys) {
    const found = caller(ctx, keys, regionOf(ctx, creds));
    return { accounts: [{ id: found.account, name: found.arn, email: "", roles: [] }] };
  }
  return { accounts: ssoAccounts(ctx, creds) };
}

function consoleIdentity(ctx, creds: Creds, keys: Keys) {
  const place = regionOf(ctx, creds);
  const found = caller(ctx, keys, place);
  return {
    user: found.arn,
    arn: found.arn,
    account: found.account,
    role: null,
    region: place,
    access_key_id: keys.accessKeyId,
    secret_access_key: keys.secretAccessKey,
    session_token: keys.sessionToken,
    expires_at: keys.expiresAt,
  };
}

export function boundOf(value: string | undefined): { account?: string; role?: string } {
  if (value === undefined || value === "") return {};
  const at = value.indexOf("/");
  return at < 0 ? { account: value } : { account: value.slice(0, at), role: value.slice(at + 1) || undefined };
}

export function cloudOptions(listed: { id: string; name: string; roles: string[] }[]) {
  return listed.flatMap((account) => {
    const name = account.name || spaced(account.id);
    if (account.roles.length === 0) return [{ value: account.id, label: name }];
    return account.roles.map((role) => ({ value: `${account.id}/${role}`, label: `${name} · ${role}` }));
  });
}

function identity(ctx, creds: Creds, args: Record<string, unknown>) {
  const keys = keysOf(creds.token);
  if (keys) return consoleIdentity(ctx, creds, keys);
  const bound = boundOf(optionalText(args, "binding"));
  const account = optionalText(args, "account") ?? bound.account ?? setting(creds, "account") ?? missing("account");
  const role = optionalText(args, "role") ?? bound.role ?? setting(creds, "role") ?? missing("role");
  const answer = portalGet(ctx, creds, "/federation/credentials", [["account_id", account], ["role_name", role]]);
  const held = answer?.roleCredentials ?? decode("no roleCredentials");
  const expiration = Number.isInteger(held.expiration) ? held.expiration : undefined;
  return {
    user: role,
    account,
    role,
    region: regionOf(ctx, creds),
    access_key_id: textOf(held, "accessKeyId"),
    secret_access_key: textOf(held, "secretAccessKey"),
    session_token: textOf(held, "sessionToken"),
    expires_at: expiration === undefined ? null : isoSeconds(expiration / 1000),
  };
}

export function call({ action, args, creds }: { action: string; args: Record<string, unknown>; creds: Creds }, ctx) {
  if (action === "cloud.accounts_and_roles") return accountsAndRoles(ctx, creds);
  if (action === "cloud.identity") return identity(ctx, creds, args ?? {});
  if (action === "cloud.options") return cloudOptions(accountsAndRoles(ctx, creds).accounts);
  return unknownAction(action);
}

function ssoOptions(listed, place: string) {
  const options = [];
  for (const account of listed) {
    const label = `${account.name} · ${spaced(account.id)}`;
    for (const role of account.roles) {
      options.push({ id: `${account.id}/${role}`, label, settings: { account: account.id, role, region: place, detail: role } });
    }
  }
  return options;
}

export function accounts({ creds }: { creds: Creds }, ctx) {
  const place = regionOf(ctx, creds);
  const keys = keysOf(creds.token);
  if (!keys) return ssoOptions(ssoAccounts(ctx, creds), place);
  const found = caller(ctx, keys, place);
  const name = found.arn.split(/[:/]/).pop() ?? "";
  return [
    {
      id: found.account,
      label: `${name} · ${spaced(found.account)}`,
      settings: { account: found.account, arn: found.arn, region: place, detail: found.arn },
    },
  ];
}
