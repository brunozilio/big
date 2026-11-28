export const spec = {
  name: "ListSessions",
  deferred: true,
  input: {
    "additionalProperties": false,
    "properties": {},
    "type": "object"
  },
  description: (ctx) => `List the user's other ${ctx.brand.NAME} sessions open on this computer: their names and folders. Send one a message with SendMessage (\`to\` its name); its answer reaches you as an event.`,
  profile: { effects: ["read"], risk: "low", reversible: true, untrusted: false, sensitive: false },
};

type Peer = { name: string; cwd: string; me: boolean };

export function effects(_input: unknown) {
  return [];
}

export function summary(_input: unknown) {
  return "";
}

export function execute(_input: unknown, ctx) {
  const sessions: Peer[] = ctx.sessions.peers();
  const me = sessions.find((peer) => peer.me)?.name ?? "unnamed";
  const others = sessions.filter((peer) => !peer.me).map((peer) => `${peer.name} (${peer.cwd})`);
  if (others.length === 0) {
    return `This session is ${me}; no other session is open.`;
  }
  return `This session is ${me}. Others:\n${others.join("\n")}`;
}
