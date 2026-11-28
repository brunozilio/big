import { workflow } from "sdk";
import { wait, act, verify, review, deliver } from "sdk/steps/general.js";

type Party = { document: string; name: string };
type Line = { code: string; description: string; quantity: number; unit_price: number; received?: number };
type Invoice = { number: string; series: string; issuer: Party; order: string; total: number; lines: Line[] };
type Order = { number: string; status: string; tolerance: number; supplier: Party; total: number; lines: Line[] };
type Item = { title?: string; description?: string; input?: { invoice?: string } };

const digits = (text: string) => String(text ?? "").replace(/\D/g, "");

const money = (value: number) => `R$ ${Number(value ?? 0).toFixed(2)}`;

function invoiceOf(item: Item): string {
  if (item.input?.invoice) return String(item.input.invoice);
  const found = /\b(\d{3,9})\b/.exec(`${item.title ?? ""} ${item.description ?? ""}`);
  return found ? found[1] : "";
}

function documents(ctx): { invoice: Invoice; order: Order } {
  return ctx.output("documents").result;
}

function lineIn(order: Order, line: Line): Line | undefined {
  return order.lines.find((wanted) => wanted.code === line.code);
}

const approved = (_item: Item, ctx) => {
  const { order } = documents(ctx);
  return { met: order.status === "approved", reason: `pedido ${order.number}: ${order.status}` };
};

const sameSupplier = (_item: Item, ctx) => {
  const { invoice, order } = documents(ctx);
  const met = digits(invoice.issuer.document) === digits(order.supplier.document);
  return { met, reason: met ? invoice.issuer.name : `NF de ${invoice.issuer.document}, pedido de ${order.supplier.document}` };
};

const ordered = (_item: Item, ctx) => {
  const { invoice, order } = documents(ctx);
  const extra = invoice.lines.filter((line) => !lineIn(order, line)).map((line) => line.code);
  return { met: extra.length === 0, reason: extra.length === 0 ? `${invoice.lines.length} itens` : `fora do pedido: ${extra.join(", ")}` };
};

const received = (_item: Item, ctx) => {
  const { invoice, order } = documents(ctx);
  const over = invoice.lines.filter((line) => line.quantity > (lineIn(order, line)?.received ?? 0));
  return { met: over.length === 0, reason: over.map((line) => `${line.code}: faturado ${line.quantity}, recebido ${lineIn(order, line)?.received ?? 0}`).join("; ") };
};

const priced = (_item: Item, ctx) => {
  const { invoice, order } = documents(ctx);
  const off = invoice.lines.filter((line) => {
    const wanted = lineIn(order, line);
    return wanted !== undefined && Math.abs(line.unit_price - wanted.unit_price) > wanted.unit_price * order.tolerance;
  });
  const shown = off.map((line) => `${line.code}: ${money(line.unit_price)} na NF, ${money(lineIn(order, line)?.unit_price ?? 0)} no pedido`);
  return { met: off.length === 0, reason: off.length === 0 ? `tolerância de ${order.tolerance * 100}%` : shown.join("; ") };
};

export default workflow({
  schema_version: 1,
  name: "nf-pedido",
  accepts: [],
  triggers: [],
  steps: [
    wait({ id: "hold", label: "Correção", onReturn: true, human: "Corrija o pedido ou a nota no sistema de compras.", continueLabel: "Conferir de novo" }),
    act({
      id: "documents",
      label: "Documentos",
      run: async (item: Item, ctx) => {
        const invoice: Invoice = await ctx.call("purchasing", "invoice", { invoice: invoiceOf(item) });
        const order: Order = await ctx.call("purchasing", "order", { order: invoice.order });
        return { invoice, order };
      },
      summary: ({ invoice, order }) => `NF ${invoice.number}-${invoice.series} · ${money(invoice.total)}\n\nPedido ${order.number} · ${money(order.total)}`,
    }),
    verify({
      id: "match",
      label: "Conferência",
      criterion: [
        { name: "Pedido aprovado", check: approved },
        { name: "Mesmo fornecedor", check: sameSupplier },
        { name: "Itens do pedido", check: ordered },
        { name: "Quantidade recebida", check: received },
        { name: "Preço", check: priced },
      ],
      onFail: "hold",
      retries: 2,
    }),
    review({ id: "approval", label: "Aprovação", of: "match", onChanges: "hold", approveLabel: "Aprovar pagamento", changesLabel: "Devolver" }),
    act({
      id: "record",
      label: "Registro",
      capability: "purchasing",
      action: "match",
      args: (item: Item, ctx) => ({ invoice: documents(ctx).invoice.number, order: documents(ctx).order.number, notes: `aprovado por ${ctx.output("approval").by ?? ""}` }),
    }),
    deliver({ label: "Liberação", capability: "purchasing", action: "release", args: (_item: Item, ctx) => ({ invoice: documents(ctx).invoice.number }) }),
  ],
});
