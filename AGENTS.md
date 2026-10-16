# AGENTS.md — Island

Survival 2D em Rust inspirado em ARK (coleta, crafting, futuramente domesticação de dinossauros) com elementos de MMORPG. Visão 3/4 top-down; o personagem anda em 8 direções mas o sprite só vira para **esquerda/direita**. Desktop, offline por enquanto.

## Stack
- Rust (edition 2024) + [macroquad 0.4](https://docs.rs/macroquad/0.4) + serde/serde_json (save). Evite novas dependências sem motivo.
- Arte provisória desenhada com primitivas em `render.rs`/`ui.rs` (sem assets).

## Comandos
```
cargo run                 # jogar (debug: F1 = itens de teste, F2 = -25 de vida, F3 = +3 horas)
cargo test                # testes de lógica (inventário, crafting, coleta, mundo)
cargo fmt && cargo clippy -- -D warnings   # rustfmt.toml: max_width 120
ISLAND_SHOT=out.png cargo run                          # smoke test: salva screenshot e sai
ISLAND_SHOT=out.png ISLAND_SHOT_INVENTORY=1 cargo run  # idem, com inventário aberto
ISLAND_SHOT=out.png ISLAND_SHOT_DEAD=1 cargo run       # idem, tela de morte
ISLAND_SHOT=out.png ISLAND_SHOT_BUILD=1 ISLAND_SHOT_HOUR=22 cargo run  # cabana de exemplo, à noite
ISLAND_NEW=1 ISLAND_SEED=42 cargo run  # ignora o save e gera outra ilha
ISLAND_SAVE=outro.json cargo run       # caminho do save (padrão: saves/save.json)
```
Se `target/debug/island.exe` estiver em uso (jogo aberto), use `CARGO_TARGET_DIR=target/verify`.
O modo `ISLAND_SHOT` nunca lê nem grava save. O jogo salva com F5, a cada 60 s e ao fechar a janela.

## Arquitetura (`src/`)
| Módulo | Papel |
|---|---|
| `game.rs` | Estado (`Game`), loop update/draw, câmera. Só orquestra. |
| `world.rs` | Tiles da ilha, geração procedural, nós de recurso, colisão. |
| `player.rs` | Movimento, corrida, facing esquerda/direita, timers de golpe. |
| `vitals.rs` | Vida, comida, água, stamina: drenagem, dano por fome/sede, regeneração. |
| `items.rs` | `ItemId`: nome, pilha máxima, ferramenta, durabilidade, efeito de comida. |
| `inventory.rs` | Slots com empilhamento e durabilidade; slots `0..HOTBAR_SIZE` são a hotbar. |
| `crafting.rs` | Receitas (`RECIPES`, tempo, estação) e fila de crafting (`CraftQueue`). |
| `building.rs` | Estruturas no grid (piso/objeto), colisão, combustível, constantes de alcance. |
| `daynight.rs` | Relógio do jogo e curvas de escuridão/entardecer. |
| `gathering.rs` | Rendimento por golpe (nó × ferramenta). |
| `render.rs` | Desenho do mundo com y-sort, estruturas e iluminação (malha com cor por vértice). |
| `ui.rs` | HUD, inventário, crafting e baú (modo imediato). |
| `save.rs` | DTOs serde + leitura/escrita atômica do save JSON (`SAVE_VERSION`). |
| `rng.rs` | RNG/ruído determinísticos (mesma seed = mesma ilha). |

## Convenções
- Regras de jogo ficam em módulos puros (sem chamadas de desenho/input) e têm testes; `render.rs`/`ui.rs` só leem estado.
- Coordenadas de mundo em pixels, Y para baixo, `TILE = 32`. `pos` de entidade = ponto do pé (base), usado no y-sort e na colisão.
- Item novo: variante em `ItemId` + seus `match` em `items.rs` + ícone em `ui::draw_item_icon` (o `match` exaustivo força isso).
- Save: o mundo é regenerado pela seed e só o estado mutável vai para `SaveData`. Campo novo no save leva `#[serde(default)]`; mudar a geração do mundo ou quebrar o formato exige incrementar `SAVE_VERSION`.
- Estrutura nova: variante em `StructureKind` + item correspondente + desenho em `render::draw_structure` + rótulo em `render::structure_label` se for interativa.
- Código e identificadores em inglês; textos do jogo e docs em português (a fonte padrão suporta acentos).
- Balanceamento (hp de nós, rendimentos, receitas) é dado: mude os números nos módulos acima, não espalhe constantes.
- Antes de concluir: `cargo test`, `cargo clippy` sem warnings e um `ISLAND_SHOT` para conferir visual quando mexer em render/UI.
- CI (`.github/workflows/ci.yml`) roda `fmt --check`, `clippy -D warnings` e testes. Publicar uma release no GitHub dispara `release.yml`, que anexa builds de Windows/Linux/macOS.

Planejamento: ver [ROADMAP.md](ROADMAP.md).
