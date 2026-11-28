import { decode, fail, fetchJson, regional } from "sdk/connectors/index.ts";
import { sign, stampOf } from "./sigv4.ts";
import type { Keys } from "./sts.ts";

const TASK_PATH = "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI";
const JSON_1_1 = "application/x-amz-json-1.1";
const GET_SECRET = "secretsmanager.GetSecretValue";

export function secretEnvironment(): string[] {
  return [TASK_PATH];
}

export function regionOfReference(reference: string): string | undefined {
  const region = reference.split(":")[3];
  return region === undefined || region === "" ? undefined : region;
}

function taskKeys(ctx, environment: Record<string, string>): Keys {
  const path = environment?.[TASK_PATH];
  if (typeof path !== "string" || path === "") return fail("auth", "no task credentials: this is not running as an ECS task");
  const answer = fetchJson(ctx, { method: "GET", url: ctx.config.task_credentials_url.replace("{path}", path) });
  if (typeof answer?.AccessKeyId !== "string" || typeof answer?.SecretAccessKey !== "string") return decode("the task credentials have no key");
  return {
    accessKeyId: answer.AccessKeyId,
    secretAccessKey: answer.SecretAccessKey,
    sessionToken: typeof answer.Token === "string" ? answer.Token : "",
    accountId: "",
    expiresAt: typeof answer.Expiration === "string" ? answer.Expiration : "",
  };
}

export function secret({ reference, environment }: { reference: string; environment: Record<string, string> }, ctx): string {
  const region = regionOfReference(reference) ?? fail("permanent", `${reference}: not a secret ARN`);
  const keys = taskKeys(ctx, environment);
  const url = regional(ctx.config.secrets_url, region);
  const body = JSON.stringify({ SecretId: reference });
  const headers: [string, string][] = [["content-type", JSON_1_1], ["x-amz-target", GET_SECRET]];
  const signing = { method: "POST", url, headers, body, region, service: "secretsmanager", stamp: stampOf(new Date()) };
  const all: Record<string, string> = Object.fromEntries(headers);
  for (const [name, value] of sign(ctx, signing, keys)) all[name] = value;
  const answer = fetchJson(ctx, { method: "POST", url, headers: all, body });
  return typeof answer?.SecretString === "string" ? answer.SecretString : decode("the secret has no text");
}
