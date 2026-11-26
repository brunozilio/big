import { spec as flow } from "./flows/gitflow.ts";

const [promotion] = flow.promote;

export const environments = [
  { id: "develop", label: "Develop", short: "Develop", icon: "branch", branch: flow.base, gate: { kind: "none" }, rollback: true },
  {
    id: "hi",
    label: "Homologação interna",
    short: "HINT",
    icon: "sTest",
    gate: { kind: "merge", promotion: 0, from: promotion.from, into: promotion.into, method: promotion.method, title: "Promover {from} para {into}" },
    autoPromote: true,
    rollback: true,
  },
  { id: "hext", label: "Homologação externa", short: "HEXT", icon: "globe", gate: { kind: "change", environment: "HST" }, seals: true },
  { id: "prd", label: "Produção", short: "PROD", icon: "rocket", gate: { kind: "change", environment: "PRD" }, locksTasks: true },
];
