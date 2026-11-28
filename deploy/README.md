# Servidor compartilhado (AWS)

O mesmo binário roda local (um usuário, login automático) ou compartilhado (vários usuários, login). O modo é escolhido na compilação: `deploy/shared.toml` é aplicado por cima do `config.toml` (`BRAND_OVERLAY`).

## Imagem

```
make image
```

Edite antes `deploy/shared.toml`: `public_url`, domínios, admins, SSO e SMTP.

## Topologia atual (uma task)

- **ECS Fargate**, 1 task, porta 7420, atrás de um **ALB** com HTTPS (ACM) no domínio do `public_url`.
- **EFS** montado em `/data`: banco SQLite, segredos selados, workspaces dos repositórios (`/data/.../workspaces/<usuário>/`), anexos.
- Chave dos segredos em `/run/secrets/secrets.key` (64 dígitos hex), montada de um volume separado do EFS de dados — por exemplo, um segundo EFS access point só leitura, preenchido a partir do Secrets Manager.
- **SES** pelo SMTP para o código por e-mail (`smtp_url = "smtps://USER:PASS@email-smtp.<região>.amazonaws.com:465"`).
- **SSO**: registre um app OIDC no provedor da empresa com redirect `https://<public_url>/api/auth/sso/callback`.
- Health check do ALB: `GET /api/auth/methods` (aberto, 200).
- Uma task só: sessões de login e desafios ficam no banco, mas fluxos de conexão em andamento, o feed ao vivo e o agendador ainda vivem na memória do processo.

## Segurança já aplicada no modo compartilhado

- Toda rota `/api` exige sessão; pedidos que alteram dados vindos de outra origem são recusados.
- Itens, sessões, conexões, aplicações, anexos e o feed ao vivo são separados por usuário.
- Branding, permissões, memórias, navegação de pastas do servidor e provedores de IA exigem admin (`[auth] admins`).
- Papéis em `[roles]` e `[access]`: `use` roda workflows e conversa com agentes, `author` edita a biblioteca (agentes, conectores, workflows), `admin` tudo. Quem entra recebe `[access] default_role`; os grupos do SSO viram papéis por `sso_groups` (`"grupo=papel, outro=papel"`, lidos da claim `sso_claim`).
- Conectores não chamam loopback nem link-local (`[network] blocked`), o que barra o endpoint de metadados da AWS.
- Repositórios são clonados pelo servidor só por `https://`, com credenciais pedidas ao conector `code` (`code.git_credentials`).

## O que ainda falta para escalar

- Postgres/RDS no lugar do SQLite (o `Store` ainda é SQLite direto).
- Várias tasks: estado de fluxos, feed ao vivo e agendador fora da memória.
- Sandbox por execução do agente (shell, git e rede do agente rodam na task do servidor).

## Comandos descobertos

O CLI acha comandos extras sozinho: toda pasta dentro de `[commands] dirs` (no `config.toml`, relativo à pasta onde o CLI roda) que tenha um `commands.json` vira subcomando, sem mexer no Rust.

```json
{ "install-x": { "about": "Cria tudo", "run": ["bun", "x.ts", "install"] } }
```

O programa roda dentro da pasta do manifesto, recebe os argumentos que vierem depois do nome e as variáveis `COMMAND_EXE` (o próprio CLI), `COMMAND_NAME` e `COMMAND_ROOT` (a pasta de onde o CLI foi chamado). Um nome igual a um subcomando embutido é ignorado.
