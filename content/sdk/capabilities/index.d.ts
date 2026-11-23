import type { Tracker } from "./tracker";
import type { Code } from "./code";
import type { Pipelines } from "./pipelines";
import type { Cloud } from "./cloud";
import type { Tests } from "./tests";

export type * from "./tracker";
export type * from "./code";
export type * from "./pipelines";
export type * from "./cloud";
export type * from "./tests";

export interface Capabilities {
  tracker: Tracker;
  code: Code;
  pipelines: Pipelines;
  cloud: Cloud;
  tests: Tests;
}

export type ActionContract = { args: unknown; result: unknown };

export type CapabilityName = keyof Capabilities;

export type ActionName<C extends CapabilityName> = keyof Capabilities[C] & string;

export type ActionArgs<C extends CapabilityName, A extends ActionName<C>> = Capabilities[C][A] extends ActionContract
  ? Capabilities[C][A]["args"]
  : never;

export type ActionResult<C extends CapabilityName, A extends ActionName<C>> = Capabilities[C][A] extends ActionContract
  ? Capabilities[C][A]["result"]
  : never;
