type Party = { document: string; name: string };
type Line = { code: string; description: string; quantity: number; unit_price: number; received?: number };
export type Invoice = { number: string; series: string; key: string; issued: string; issuer: Party; order: string; lines: Line[] };
export type Order = { number: string; status: string; buyer: string; tolerance: number; supplier: Party; lines: Line[] };

const ANDRADE: Party = { document: "12.345.678/0001-90", name: "Metalúrgica Andrade Ltda" };
const OTHER: Party = { document: "98.765.432/0001-10", name: "Andrade Comércio de Metais ME" };
const VALE: Party = { document: "45.987.321/0001-55", name: "Embalagens Vale do Sul S.A." };

const BOLT = { code: "PAR-M8", description: "Parafuso sextavado M8 x 30 zincado" };
const WASHER = { code: "ARR-M8", description: "Arruela lisa M8 zincada" };
const SHEET = { code: "CHP-3", description: "Chapa de aço SAE 1020 3 mm 1200 x 3000" };
const BOX = { code: "CX-40", description: "Caixa de papelão ondulado 40 x 30 x 20" };

export const ORDERS: Record<string, Order> = {
  "4500012345": {
    number: "4500012345",
    status: "approved",
    buyer: "Compras Indiretas",
    tolerance: 0.02,
    supplier: ANDRADE,
    lines: [
      { ...BOLT, quantity: 2000, unit_price: 0.42, received: 2000 },
      { ...WASHER, quantity: 2000, unit_price: 0.08, received: 2000 },
      { ...SHEET, quantity: 10, unit_price: 389.9, received: 10 },
    ],
  },
  "4500012399": {
    number: "4500012399",
    status: "awaiting_approval",
    buyer: "Compras Indiretas",
    tolerance: 0.02,
    supplier: VALE,
    lines: [{ ...BOX, quantity: 500, unit_price: 3.15, received: 0 }],
  },
};

const key = (number: string) => `4326111234567800019055001${number.padStart(9, "0")}1${number.padStart(9, "0")}`;

export const INVOICES: Record<string, Invoice> = {
  "4521": {
    number: "4521",
    series: "1",
    key: key("4521"),
    issued: "2026-11-24",
    issuer: ANDRADE,
    order: "4500012345",
    lines: [
      { ...BOLT, quantity: 2000, unit_price: 0.42 },
      { ...WASHER, quantity: 2000, unit_price: 0.08 },
      { ...SHEET, quantity: 10, unit_price: 389.9 },
    ],
  },
  "4533": {
    number: "4533",
    series: "1",
    key: key("4533"),
    issued: "2026-11-25",
    issuer: ANDRADE,
    order: "4500012345",
    lines: [
      { ...BOLT, quantity: 2000, unit_price: 0.42 },
      { ...SHEET, quantity: 10, unit_price: 412.5 },
    ],
  },
  "4540": {
    number: "4540",
    series: "2",
    key: key("4540"),
    issued: "2026-11-26",
    issuer: OTHER,
    order: "4500012345",
    lines: [{ ...WASHER, quantity: 2500, unit_price: 0.08 }],
  },
  "4547": {
    number: "4547",
    series: "1",
    key: key("4547"),
    issued: "2026-11-27",
    issuer: VALE,
    order: "4500012399",
    lines: [{ ...BOX, quantity: 500, unit_price: 3.15 }],
  },
};

const cents = (value: number) => Math.round(value * 100) / 100;

export const totalOf = (lines: Line[]) => cents(lines.reduce((sum, line) => sum + line.quantity * line.unit_price, 0));

export const priced = (lines: Line[]) => lines.map((line) => ({ ...line, total: cents(line.quantity * line.unit_price) }));
