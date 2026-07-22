# CLAUDE.md

Contexto permanente para o Claude Code neste repositório. Leia antes de qualquer alteração.

## O que é

`mirante` — perfilador de CSV que roda inteiramente no navegador. O núcleo é Rust
compilado para WebAssembly; o arquivo do usuário nunca sai da máquina dele.

Um backend FastAPI opcional faz o mesmo perfilamento com pandas, apenas para servir
de comparação de desempenho documentada no README.

Projeto de portfólio. Prioridade: estar no ar, funcionando, com README honesto.

## Escopo

**Dia 1 (obrigatório)**
- Núcleo Rust: parsing de CSV, inferência de tipo, agregação por coluna
- Casca Wasm e worker
- UI: arrastar arquivo, tabela de perfil, histogramas em SVG
- Deploy estático funcionando

**Dia 2**
- Backend FastAPI com pandas fazendo o mesmo perfil
- Botão de comparação lado a lado
- README com metodologia e resultados

**Não-objetivos.** Não implemente nada disto sem pedido explícito:
Parquet, autenticação, banco de dados, upload para servidor, contas de usuário,
i18n, PWA, modo offline, gráficos além do histograma.

## Layout

```
crates/core/     lógica pura de perfilamento, sem nada de Wasm — é aqui que mora o valor
crates/wasm/     casca fina #[wasm_bindgen] em volta do core
web/             index.html, main.ts, worker.ts, pkg/ (gerado)
api/             FastAPI de comparação, gerenciado por uv
```

A separação `core` / `wasm` é obrigatória. O `core` precisa rodar sob `cargo test`
nativo e, no futuro, ser exposto via PyO3 sem reescrita. Nenhum tipo de
`wasm-bindgen` pode vazar para dentro do `core`.

## Comandos

```bash
# Rust
cargo test -p mirante-core
cargo clippy --all-targets -- -D warnings
cargo fmt

# Build Wasm (sempre --release; debug é ordens de grandeza mais lento)
wasm-pack build crates/wasm --target web --release --out-dir ../../web/pkg

# Front
cd web && python3 -m http.server 8080

# API
cd api && uv run uvicorn app.main:app --reload
cd api && uv run ruff check . && uv run ruff format .
cd api && uv run pytest
```

Use `cargo add` e `uv add` para incluir dependências. Não escreva números de versão
à mão nos manifestos.

## Restrições do alvo wasm32-unknown-unknown

Não há sistema operacional embaixo. Violar qualquer um destes gera erro em runtime,
não em compilação:

- `std::time::Instant` não funciona. Toda cronometragem acontece no TypeScript.
- Sem threads. Não adicione `rayon`.
- Sem sistema de arquivos. O core recebe `&[u8]`, nunca um caminho.
- Aleatoriedade exige a feature `js` da crate `getrandom`.
- `println!` não aparece em lugar nenhum. Use `web_sys::console` ou devolva dados.

## Rust

- `console_error_panic_hook` instalado num `#[wasm_bindgen(start)]`. Sem isso, panic
  vira `unreachable executed` e o debug fica impossível.
- Fronteira Wasm: use `serde-wasm-bindgen`. `JsValue::from_serde` está depreciado.
- Nenhum `unwrap()` ou `expect()` em código que processa entrada do usuário. CSV
  malformado é caso esperado, não bug — devolva `Result`.
- O `core` não faz alocação por célula. Perfilar 200 MB não pode significar alocar
  uma `String` por campo.
- Perfil de release no `Cargo.toml` raiz: `opt-level = 3`, `lto = true`,
  `codegen-units = 1`. `opt-level = "z"` é proibido: mede-se velocidade aqui.

## CSV do mundo real

Os arquivos de teste incluem casos brasileiros. O parser deve tratar:

- separador `;` além de `,` (detecte pela frequência na primeira linha)
- decimal com vírgula
- encoding windows-1252 além de UTF-8
- BOM no início do arquivo
- linhas com contagem de campos divergente do cabeçalho

## Python

- `uv` para tudo. Nunca invoque `pip` ou `python` direto; use `uv run`.
- `ruff` para lint e formatação.
- FastAPI com `python-multipart` instalado, senão o upload falha silenciosamente.
- O endpoint de comparação recebe o mesmo arquivo e devolve o mesmo formato de
  perfil que o Wasm. Divergência de formato invalida a comparação.
- Type hints em todas as assinaturas públicas.

## Benchmark: regras de honestidade

O valor deste projeto está na credibilidade dos números. Portanto:

- Sempre comparar build de release do Wasm contra pandas, nunca debug.
- Reportar mediana de no mínimo 5 execuções, não a melhor.
- Separar explicitamente tempo de rede do tempo de processamento na comparação com
  a API. O Wasm ganha em parte por não ter rede — isso vai escrito no README.
- Declarar máquina, navegador, versões e tamanho do dataset.
- Nunca escrever um número no README que não tenha saído de uma execução real.

## Proibido sem eu pedir

- Framework de front em Rust (Yew, Leptos, Dioxus)
- React, Vue ou qualquer bundler
- Biblioteca de gráficos — histogramas são SVG escritos à mão
- `rayon` ou qualquer tentativa de threads
- Refatorar o `core` para depender de `wasm-bindgen`
- Adicionar dependência "só pra facilitar" sem me consultar

## Idioma

Código, comentários, mensagens de commit, README e docstrings em inglês.
Conversa comigo em português.

## Pronto quando

**Dia 1:** link público abre, arrasta um CSV de 100 MB, a aba não congela, a tabela
de perfil e os histogramas aparecem corretos.

**Dia 2:** README com tabela de benchmark, metodologia declarada e a ressalva sobre
rede escrita de forma visível.
