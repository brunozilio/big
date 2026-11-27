export const PORT = {{PORT}};
export const VERSION = "{{VERSION}}";
export const BUILD = "{{BUILD}}";
export const NAME = "{{NAME}}";
export const SOCKET = PORT ? `ws://127.0.0.1:${PORT}/api/browser/socket` : null;
export const RELAY_MARKER = `${NAME}:relay`;
