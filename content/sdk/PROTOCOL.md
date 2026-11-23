# Protocolo host ↔ JS do SDK (versão 1)

Este documento descreve o contrato entre o runtime JS do SDK (`sdk/index.js`) e o host Rust (o kernel do orquestrador). O runtime é código puro: não faz I/O, não lê relógio nem gera aleatoriedade fora do `ctx`. Tudo o que toca o mundo vira um **comando** que o host executa e devolve.

## 1. Modelo

- Um item roda um handler durável: `workflow.run(ctx, item)`, ou o executor padrão de `steps: [...]`.
- O handler é **reexecutado do zero a cada ativação** (replay). O host guarda um **journal** com o resultado de cada comando já resolvido; no replay, o runtime devolve esses resultados sem pedir de novo.
- Cada chamada ao `ctx` que precisa do host recebe um número **`seq`**: um contador único, global à execução, que começa em 0 e cresce na ordem em que o handler faz as chamadas. Sinais (seção 4) também consomem `seq`.
- O runtime entrega os resultados do journal **um por vez, na ordem do journal**, drenando a fila de microtarefas entre cada entrega. Assim a ordem de intercalação entre ramos paralelos é a mesma da execução original e os `seq` são estáveis, mesmo com `ctx.all`, `ctx.race` e `ctx.any`.
- Drenar significa rodar a fila de microtarefas até ficar 1000 voltas seguidas sem nova atividade do `ctx` (limite total de 1.000.000 voltas). A regra é a mesma em toda ativação, então uma ativação é sempre prefixo da seguinte.

## 2. Carga do módulo

- O host avalia o módulo do workflow do cliente, que importa `sdk`. `workflow(...)` e `step(...)` registram as definições num registro do módulo `sdk`, por nome.
- O host resolve os imports `sdk` e `sdk/*` para os arquivos embutidos (ou sobrescritos em `<DIR>/sdk/`), e tira os tipos do TS antes de avaliar.
- Exportações para o host, em `host` (`export const host`):
  - `host.protocol`: número da versão deste protocolo (`1`).
  - `host.describe()`: os workflows registrados (`name`, `statuses`, `triggers`, `reconcile`, `steps`, `code`) e os nomes dos tipos de etapa. Serve para registrar gatilhos e montar a UI.
  - `host.activate(activation)`: roda uma ativação (seção 3) e devolve uma `Promise` do resultado. O host roda a fila de jobs do QuickJS até ela resolver.
  - `host.view({ step, state })`: chama a `view(state)` do tipo de etapa e devolve a lista de blocos (dados puros).

## 3. Ativação

Entrada (`Activation`):

```json
{
  "target": { "workflow": "entrega" },
  "item": { "id": "…", "workflow": "entrega", "title": "…", "labels": [], "branch": null, "issue": null, "input": null },
  "journal": [ { "seq": 0, "kind": "agent", "ok": { "text": "…" }, "at": 1732280000000 } ],
  "start": 1732279000000,
  "seed": "<id do item>"
}
```

- `target`: `{ "workflow": nome }` roda o workflow; `{ "step": tipo, "id"?: id, "options"?: {...} }` roda só um tipo de etapa. Sem `target`, roda o primeiro workflow registrado.
- `journal`: as resoluções na **ordem em que aconteceram**. Cada entrada:
  - `seq`: o comando resolvido;
  - `kind`: o tipo do comando quando foi pedido (checagem de determinismo);
  - exatamente um de `ok` (o valor JSON), `error` (`{ kind, message?, data? }`) ou `cancelled: true`;
  - `at`: o instante da resolução em ms desde a época; vira o valor de `ctx.now()` daí em diante.
- `start`: instante de início do item; é o `ctx.now()` antes de qualquer resolução.
- `seed`: semente do gerador de `ctx.random()` (determinístico, igual em todo replay).

Saída (`ActivationResult`):

```json
{
  "state": "running",
  "pending": [ { "seq": 3, "kind": "exec", "branch": "main/1.0", "step": "checks", "attempt": 1, "args": { "command": "bun test" }, "timeout": 600000 } ],
  "cancel": [ 5 ],
  "signals": [ { "seq": 1, "kind": "fork", "branch": "main", "step": null, "attempt": 0, "mode": "all", "branches": ["main/1.0", "main/1.1"] } ],
  "seq": 6
}
```

- `state`:
  - `running`: o handler espera pelos comandos em `pending`;
  - `completed`: terminou; `result` traz o valor devolvido;
  - `cancelled`: terminou com `ctx.cancel(reason)` (ou `Cancelled` lançado); `reason` traz o motivo;
  - `failed`: o handler lançou um erro não tratado; `error` traz `{ kind, message, data }`;
  - `nondeterministic`: o código mudou e o replay não bate com o journal (um `seq` do journal não foi pedido, foi pedido com outro `kind` ou foi resolvido duas vezes). O host **não** marca o item como falho: para o item e mostra o erro.
- `pending`: todos os comandos pedidos e ainda sem resolução, em qualquer ativação. O host deduplica por `seq` (um comando já em andamento não é executado de novo).
- `cancel`: `seq` de comandos que o handler não espera mais (ramos perdedores de `race`/`any`, ramos irmãos de um `all` que falhou, ou tudo o que sobrou quando o workflow terminou). O host cancela o efeito. Pode registrar `{ seq, cancelled: true }` no journal ou não: o runtime ignora resultados de comandos cancelados.
- `signals`: marcas de progresso que não precisam de resposta (seção 4). O host deduplica por `seq` e as grava no log de eventos.
- `seq`: o próximo `seq` livre (útil para diagnóstico).

Laço do host:

1. Monta a ativação com o journal atual e chama `host.activate`.
2. Grava os `signals` novos e, para cada comando novo de `pending`, grava `EffectRequested` com a chave de idempotência e começa a executá-lo.
3. Cancela os `seq` de `cancel` que ainda estiverem em andamento.
4. Quando um efeito termina, anexa `EffectResolved` ao journal e volta ao passo 1. Várias resoluções podem ser anexadas antes da próxima ativação.

## 4. Comandos e sinais

Campos comuns de um comando: `seq`, `kind`, `branch`, `step` (id da etapa ou `null`), `attempt` (tentativa da etapa, 0 fora de etapa), `args` e, quando dados, `connection`, `retry`, `timeout` e `optional`. Um `call` com `optional: true` e sem `connection` resolve com `null` quando nenhuma conexão oferece a capacidade, em vez de bloquear o item.

| `ctx` | `kind` | `args` | resolve com |
|---|---|---|---|
| `agent(name, input, opts?)` | `agent` | `{ name, input: { prompt, … }, model?, mode?, worktree? }` | `{ text, session, output }` |
| `exec(cmd, opts?)` | `exec` | `{ command, cwd?, env?, input? }` | `{ code, output, stdout, stderr }` |
| `call(capability, action, args, opts?)` | `call` | `{ capability, action, args }` | o resultado da ação |
| `waitHuman(view)` | `wait.human` | `{ view: Block[] }` | `{ action, values, by }` |
| `poll(call, every, opts?)` | `wait.poll` | `{ call: { capability, action, args }, every, until }` | o primeiro resultado que casa com `until` |
| `event(kind, filter, opts?)` | `event` | `{ kind, filter }` | o `ExternalEvent` normalizado |
| `sleep(ms)` | `sleep` | `{ ms }` | `null` |
| `child(workflow, input)` | `child` | `{ workflow, input }` | o `result` do item filho (`ChildFinished`) |
| `status(name)` | `status.set` | `{ name }` | `null` |
| `impediment(on, reason?)` | `impediment.set` | `{ on, reason }` | `null` |

- `until` do `poll` é um padrão JSON: o resultado casa quando contém todos os campos do padrão com os mesmos valores (comparação parcial e recursiva; um array no padrão significa "qualquer um destes"). Sem `until`, o primeiro resultado com sucesso resolve.
- `timeout` vale para qualquer comando; ao estourar, o host resolve com `error: { kind: "timeout" }`.
- `retry` (`{ max, backoff: "exponential", initial, maxInterval, jitter }`) é o retry **técnico** do efeito e é aplicado pelo host, sem nova ativação entre as tentativas. O padrão vem do `config.toml` (`[retry]`).
- Erros: `error.kind` é `transient`, `rate_limited`, `auth`, `conflict`, `permanent`, `timeout` ou `cancelled`. No JS, o `await` rejeita com `EffectError { kind, message, data, seq }`. `auth` sem refresh possível vira espera humana do lado do host ("Reconectar") e não chega ao JS.
- Não são comandos: `pass`, `fail`, `goto` e `cancel` devolvem um `Outcome` (dado puro) que a etapa ou o workflow retorna; `now()` e `random()` são síncronos e determinísticos (seção 3).

Sinais (sem resposta):

| `kind` | quando | campos extras |
|---|---|---|
| `step.started` | uma etapa começa (lista ou chamada direta `etapa(ctx, opts)`) | `type`, `options` |
| `step.finished` | a etapa termina | `outcome` (`passed`, `failed`, `goto`, `cancelled`), `data`, `to` |
| `fork` | `all`, `race` ou `any` começa | `mode`, `branches` (ids dos ramos) |
| `branch.settled` | `race`/`any` decidiu, ou um `all` falhou | `winner`, `index`, `state` |

## 5. Ramos e chaves de idempotência

- O ramo raiz é `main`. Um `ctx.all/race/any` chamado no ramo `B` consome um `seq` `S` (o do sinal `fork`), e o ramo `i` ganha o id **`B/S.i`**. Ids não colidem mesmo com vários `all` no mesmo ramo e não dependem do tempo.
- Um ramo passado como função, `(ctx) => …`, recebe um `ctx` do próprio ramo; tudo o que ele pede leva esse `branch`. Um ramo passado como `Promise` já criada continua no ramo de quem a criou; se for um comando único do `ctx`, o `race` cancela exatamente aquele `seq` quando perde.
- `race` e `any` resolvem com `{ index, branch, kind, value }`; `kind` é o tipo do comando quando o ramo é um comando único (por exemplo `"event"`) ou `"value"`.
- A chave de idempotência do efeito é `item/step/branch/seq/attempt`, com o prefixo vindo de `brand`. O host repete um efeito interrompido com a mesma chave.

## 6. Etapas

- `step({ name, view, run })` registra um tipo de etapa e devolve uma função com duas formas:
  - `etapa(opts)` devolve um `StepCall` (`{ type: "step-call", step, id, options }`) para a lista `steps` do workflow;
  - `etapa(ctx, opts)` roda a etapa na hora e devolve o `Outcome`; `failed` vira `StepFailed` e `cancelled` vira `Cancelled`.
- O id da etapa é `options.id` ou o nome do tipo. Cada nova execução do mesmo id no mesmo ramo aumenta `attempt`.
- `options.status`, quando presente, vira `ctx.status(status)` antes do `run`; `options.doneStatus` vira `ctx.status(doneStatus)` quando a etapa passa.
- Dentro da etapa, `ctx.item` é o item da ativação (somente leitura), `ctx.output(id)` devolve os dados do último `pass`/`fail` da etapa `id` nesta execução (ou `null`) e `ctx.from` diz por que o executor entrou nela: `{ step, outcome: "failed" | "goto", data }` quando veio de um `onFail` ou de um `goto`, `null` no caminho normal. Tudo sai do replay, sem comando novo.
- Executor de `steps: [...]`: `passed` segue para a próxima; `goto(id)` pula para a etapa; `failed` volta para `options.onFail` até `options.retries` vezes; esgotado, liga o impedimento e pede ao humano `retry`, `skip` ou `cancel`.
- O `state` passado a `view(state)` é montado pelo host: os dados do último `pass`/`fail` espalhados no objeto, mais `id`, `state` (`pending`, `running`, `waiting_human`, `passed`, `failed`, `skipped`), `attempt`, `passed` e `failed`.

## 7. Biblioteca padrão

- `sdk/steps` resolve para `sdk/steps/index.js`, que reexporta `plan`, `ticket`, `develop`, `checks`, `pr`, `ci`, `review`, `merge` e `release`; os imports relativos (`./context.js`) são resolvidos a partir do arquivo que importa. Os tipos ficam nos `.d.ts` ao lado.
- As etapas usam só a API pública: o cliente troca qualquer uma criando `<DIR>/sdk/steps/<nome>.js` ou escreve as suas.
- Argumentos que a biblioteca passa ao host além da tabela da seção 4: `agent` recebe `askTimeout` (ms até uma pergunta sem resposta virar impedimento; sem ele, vale o `config.toml`) e `worktree` (a branch do item; o host cria ou reaproveita o worktree dela).

## 8. Regras para quem escreve workflows

- I/O só pelo `ctx`. Relógio e aleatoriedade só por `ctx.now()` e `ctx.random()`.
- Concorrência com novos comandos depois de um `await` vai por `ctx.all/race/any` com ramos-função. `Promise.all` direto sobre comandos do `ctx` é seguro só quando as continuações não pedem novos comandos.
- Mudar a ordem ou o tipo das chamadas ao `ctx` de um workflow com itens em andamento torna esses itens `nondeterministic`.
