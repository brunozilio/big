import release from "../../packages/config.ts";

export type EnvironmentOption = { value: string; label: string };

export function environmentOptions(): EnvironmentOption[] {
  const listed = Array.isArray(release?.environments) ? release.environments : [];
  return listed
    .map((env) => ({ value: String(env?.id ?? ""), label: String(env?.label ?? env?.id ?? "") }))
    .filter((env) => env.value !== "");
}

type Choice = { value?: unknown; label?: unknown };

const textOf = (choice: Choice | undefined) => (typeof choice?.value === "string" ? choice.value : "");

export function profileFor(profiles: unknown, key: string, environment: string | undefined): Record<string, Choice> | undefined {
  const listed = (Array.isArray(profiles) ? profiles : []).filter((profile) => profile && typeof profile === "object");
  if (environment === undefined) return listed[0];
  const wanted = environment.toLowerCase();
  return listed.find((profile) => [profile[key]?.value, profile[key]?.label].some((name) => typeof name === "string" && name.toLowerCase() === wanted));
}

export function chosenIn(profiles: unknown, key: string, field: string, environment: string | undefined): string | undefined {
  const found = textOf(profileFor(profiles, key, environment)?.[field]);
  return found === "" ? undefined : found;
}
