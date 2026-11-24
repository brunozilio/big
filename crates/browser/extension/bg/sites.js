import { refuse } from "./results.js";

export function hostOf(url) {
  try {
    const parsed = new URL(url);
    return ["http:", "https:"].includes(parsed.protocol) ? parsed.hostname : "";
  } catch {
    return "";
  }
}

function covered(sites, host) {
  return (sites || []).some((site) => (site.startsWith("*.") ? host.endsWith(site.slice(1)) : site === "*" || site === host));
}

export function ensure(url, sites) {
  const host = hostOf(url);
  if (!host || covered(sites, host)) return;
  refuse(`The page is on ${host}, which the user has not approved for this call. Call the tool again so the user is asked about ${host}.`);
}
