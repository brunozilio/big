import { type Window, epoch, instant, invalid, monthly, number, pick, titled, window } from "sdk/providers/index.ts";

function moment(ctx, value): string | null {
  if (typeof value === "string") return instant(ctx, value);
  if (typeof value === "number") return epoch(ctx, Math.floor(value));
  return null;
}

const amount = (entry, key: string) => number(entry?.[`${key}WithPrecision`]) ?? number(entry?.[key]);

function shown(value: number): string {
  const rounded = Math.round(value * 100) / 100;
  return Number.isInteger(rounded) ? rounded.toFixed(0) : String(rounded);
}

function metered(id: string, name: string, entry, resets: string | null, language: string): Window | undefined {
  const used = amount(entry, "currentUsage");
  const limit = amount(entry, "usageLimit");
  if (used === undefined || limit === undefined || limit <= 0) return undefined;
  const [spent, most] = [shown(used), shown(limit)];
  const label = pick(language, `${name} · ${spent} of ${most}`, `${name} · ${spent} de ${most}`, `${name} · ${spent} de ${most}`);
  return window(id, label, (used / limit) * 100, resets);
}

function extra(ctx, kind: string, name: string, at: number, bonus, language: string): Window | undefined {
  if (typeof bonus?.status === "string" && bonus.status !== "ACTIVE") return undefined;
  const title = typeof bonus?.displayName === "string" ? bonus.displayName : pick(language, `${name} bonus`, `${name} bono`, `${name} bônus`);
  return metered(`${kind}_bonus_${at}`, title, bonus, moment(ctx, bonus?.expiresAt), language);
}

function breakdown(ctx, entry, reset: string | null, language: string): Window[] {
  const kind = (typeof entry?.resourceType === "string" ? entry.resourceType : "usage").toLowerCase();
  const name = typeof entry?.displayNamePlural === "string" ? entry.displayNamePlural : typeof entry?.displayName === "string" ? entry.displayName : pick(language, "Credits", "Créditos", "Créditos");
  const resets = moment(ctx, entry?.nextDateReset) ?? reset;
  const out: Window[] = [];
  const main = metered(kind, name, entry, resets, language);
  if (main) out.push(monthly(ctx, main));
  const trial = entry?.freeTrialInfo;
  if (trial?.freeTrialStatus === "ACTIVE") {
    const label = pick(language, `${name} (free trial)`, `${name} (prueba gratuita)`, `${name} (teste gratuito)`);
    const found = metered(`${kind}_trial`, label, trial, moment(ctx, trial.freeTrialExpiry), language);
    if (found) out.push(found);
  }
  const bonuses = Array.isArray(entry?.bonuses) ? entry.bonuses : [];
  bonuses.forEach((bonus, at) => {
    const found = extra(ctx, kind, name, at, bonus, language);
    if (found) out.push(found);
  });
  return out;
}

function plan(ctx, info): string | null {
  const title = typeof info?.subscriptionTitle === "string" ? info.subscriptionTitle.trim() : "";
  return title !== "" ? title : titled(ctx, info?.type);
}

export function windows(ctx, value, language: string) {
  const list = value?.usageBreakdownList;
  if (!Array.isArray(list)) invalid("Kiro usage has no usageBreakdownList");
  const reset = moment(ctx, value.nextDateReset);
  return { plan: plan(ctx, value.subscriptionInfo), windows: list.flatMap((entry) => breakdown(ctx, entry, reset, language)) };
}
