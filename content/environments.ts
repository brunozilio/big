export const environments = [
  { id: "develop", label: "Develop", short: "Develop", icon: "branch", lane: true, gate: { kind: "none" }, rollback: true },
  {
    id: "hint",
    label: "Homologação interna",
    short: "HINT",
    icon: "sTest",
    gate: { kind: "merge", promotion: 0, title: "Promover {from} para {into}" },
    autoPromote: true,
    rollback: true,
  },
  { id: "hext", label: "Homologação externa", short: "HEXT", icon: "globe", gate: { kind: "change", environment: "HST" }, seals: true },
  { id: "prod", label: "Produção", short: "PROD", icon: "rocket", gate: { kind: "change", environment: "PRD" }, locksTasks: true },
];
