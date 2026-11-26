function pick(language: string | undefined, english: string, spanish: string, portuguese: string): string {
  if (language === "en") return english;
  if (language === "es") return spanish;
  return portuguese;
}

export function kinds({ language }: { language?: string } = {}) {
  return [
    { key: "application", label: pick(language, "Application", "Aplicación", "Aplicação") },
    { key: "library", label: pick(language, "Library", "Biblioteca", "Biblioteca") },
    { key: "configuration", label: pick(language, "Configuration", "Configuración", "Configuração") },
    { key: "infrastructure", label: pick(language, "Infrastructure", "Infraestructura", "Infraestrutura") },
    { key: "tests", label: pick(language, "Tests", "Pruebas", "Testes") },
  ];
}
