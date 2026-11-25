# Regras do projeto

- Todo comando cargo usa `CARGO_TARGET_DIR` explícito, por exemplo `CARGO_TARGET_DIR="$PWD/target/agents/<tarefa>"`. Nunca use o target padrão e nunca rode `cargo clean` nele.
- Ao terminar a tarefa (verificação rodada), apague o próprio target: `rm -rf "$PWD/target/agents/<tarefa>"`. Só o seu; nunca o de outra tarefa nem o `target/agents/big`. Reaproveite um target já existente quando a tarefa continuar a anterior.
- Sem comentários no código; o `xtask lint` reprova.
- Sem `unwrap` fora de testes.
- Funções com menos de 100 linhas.
- Trabalho direto na `main`.
- O nome do produto vem só do `config.toml`, pelo crate `brand`. Nenhum código, texto ou rota escreve o nome.
- Provedores (Jira, GitHub, AWS, IA) nunca são acoplados: tudo passa por interfaces comuns no crate `connectors`.
- Configuração fica no `config.toml`, não em variáveis de ambiente.
- Nunca inicie, pare ou reinicie o servidor do app (`big`) sem o usuário pedir.
- O plano completo está em `/Users/bruno/.claude/plans/esse-um-projeto-gentle-hummingbird-of-25226a84-897b-43cb-a1a4-ba0d2e6a2449.md`.

## Verificação

```
CARGO_TARGET_DIR="$PWD/target/agents/<tarefa>" cargo nextest run --workspace
CARGO_TARGET_DIR="$PWD/target/agents/<tarefa>" cargo clippy --workspace --all-targets -- -D warnings
CARGO_TARGET_DIR="$PWD/target/agents/<tarefa>" cargo run -p xtask -- lint
```

## Crates

```
cli (launcher) ──► server, app, platform, domain, brand
server ──► orchestrator, connectors, store, sync, app, page, domain, brand, platform
orchestrator ──► agent-engine, connectors, sync, script, content, store, domain, platform, brand
connectors ──► domain, brand, platform
store ──► domain
sync ──► domain, store, connectors, brand
app ──► agent-engine, llm-providers, platform, domain, brand, code-map
agent-engine ──► script, content, brand, domain, platform, language-servers, llm-providers
```
