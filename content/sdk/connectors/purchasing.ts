import { type Action, alters, looks, object, read, write } from "./contracts.ts";

const party = () => object([["document", "string"], ["name", "string"]]);

const line = () =>
  object([["code", "string"], ["description", "string"], ["quantity", "number"], ["unit_price", "number"]], [["received", "number"], ["total", "number"]]);

function withLines(schema: Record<string, unknown>, holder: string): Record<string, unknown> {
  const properties = schema.properties as Record<string, unknown>;
  properties[holder] = party();
  properties.lines = { type: "array", items: line() };
  return schema;
}

export function purchasing(): Action[] {
  const invoice = withLines(
    object([["number", "string"], ["series", "string"], ["issued", "string"], ["total", "number"], ["lines", "array"]], [["order", "string"], ["key", "string"], ["status", "string"]]),
    "issuer",
  );
  const order = withLines(
    object([["number", "string"], ["status", "string"], ["total", "number"], ["lines", "array"]], [["buyer", "string"], ["tolerance", "number"]]),
    "supplier",
  );
  const outcome = object([["invoice", "string"], ["status", "string"]], [["order", "string"], ["due", "string"], ["ref", "string"]]);
  return [
    { ...read("purchasing.invoice", object([["invoice", "string"]]), invoice, looks(true, true)), description: "Read an invoice received from a supplier, with its lines and the order it cites." },
    { ...read("purchasing.order", object([["order", "string"]]), order, looks(false, true)), description: "Read a purchase order, with its lines, what was already received and the price tolerance." },
    {
      ...write("purchasing.match", object([["invoice", "string"], ["order", "string"]], [["notes", "string"]]), outcome, "lookup", alters("medium", "compensate", { freeText: ["notes"] })),
      description: "Record that an invoice matches its purchase order.",
    },
    {
      ...write("purchasing.release", object([["invoice", "string"]], [["due", "string"]]), outcome, "native", alters("high", "compensate", { effects: ["write", "money"] })),
      description: "Release a matched invoice for payment.",
    },
  ];
}
