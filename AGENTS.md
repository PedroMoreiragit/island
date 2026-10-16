# AGENTS.md — Island

Survival 2D em Rust inspirado em ARK (coleta, crafting, futuramente domesticação de dinossauros) com elementos de MMORPG. Visão 3/4 top-down; o personagem anda em 8 direções mas o sprite só vira para **esquerda/direita**. Desktop, offline por enquanto.

## Stack
- Rust (edition 2024) + [macroquad 0.4](https://docs.rs/macroquad/0.4). Sem outras dependências.
- Arte provisória desenhada com primitivas em `render.rs`/`ui.rs` (sem assets).

## Comandos
```
cargo run                 # jogar (F1 em debug = itens de teste)
cargo test                # testes de lógica (inventário, crafting, coleta, mundo)
cargo fmt && cargo clippy -- -D warnings   # rustfmt.toml: max_width 120
ISLAND_SHOT=out.png cargo run                          # smoke test: salva screenshot e sai
ISLAND_SHOT=out.png ISLAND_SHOT_INVENTORY=1 cargo run  # idem, com inventário aberto
ISLAND_SEED=42 cargo run  # outra ilha
```
Se `target/debug/island.exe` estiver em uso (jogo aberto), use `CARGO_TARGET_DIR=target/verify`.

## Arquitetura (`src/`)
| Módulo | Papel |
|---|---|
| `game.rs` | Estado (`Game`), loop update/draw, câmera. Só orquestra. |
| `world.rs` | Tiles da ilha, geração procedural, nós de recurso, colisão. |
| `player.rs` | Movimento, facing esquerda/direita, timers de golpe. |
| `items.rs` | `ItemId`: nome, pilha máxima, tipo de ferramenta. |
| `inventory.rs` | Slots com empilhamento; slots `0..HOTBAR_SIZE` são a hotbar. |
| `crafting.rs` | Receitas (`RECIPES`) e craft atômico. |
| `gathering.rs` | Rendimento por golpe (nó × ferramenta). |
| `render.rs` | Desenho do mundo com y-sort. |
| `ui.rs` | HUD, inventário e crafting (modo imediato). |
| `rng.rs` | RNG/ruído determinísticos (mesma seed = mesma ilha). |

## Convenções
- Regras de jogo ficam em módulos puros (sem chamadas de desenho/input) e têm testes; `render.rs`/`ui.rs` só leem estado.
- Coordenadas de mundo em pixels, Y para baixo, `TILE = 32`. `pos` de entidade = ponto do pé (base), usado no y-sort e na colisão.
- Item novo: variante em `ItemId` + `name`/`max_stack`/`tool` + ícone em `ui::draw_item_icon` (o `match` exaustivo força isso).
- Código e identificadores em inglês; textos do jogo e docs em português (a fonte padrão suporta acentos).
- Balanceamento (hp de nós, rendimentos, receitas) é dado: mude os números nos módulos acima, não espalhe constantes.
- Antes de concluir: `cargo test`, `cargo clippy` sem warnings e um `ISLAND_SHOT` para conferir visual quando mexer em render/UI.

Planejamento: ver [ROADMAP.md](ROADMAP.md).
