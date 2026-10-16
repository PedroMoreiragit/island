# Island

> ⚠️ **Projeto experimental.** Island é um experimento pessoal de desenvolvimento de jogos em Rust, feito em boa parte com assistência de IA. Não é um produto acabado: a arte é provisória (desenhada com formas geométricas), o balanceamento é chute, saves podem quebrar entre versões e tudo pode mudar a qualquer momento.

Survival 2D inspirado em **ARK** com elementos de MMORPG: colete recursos, fabrique ferramentas, sobreviva numa ilha e, no futuro, domestique dinossauros. Visão 3/4 de cima, com o personagem virando só para a esquerda ou para a direita. Roda no desktop, offline.

## O que já tem (v0.3)

- Ilha procedural com praia, água rasa e água funda
- Coleta de madeira, palha, pedra, sílex e frutinhas
- Inventário com hotbar, pilhas e ferramentas com durabilidade
- Crafting com fila e tempo: ferramentas, fogueira, bancada, baú, fundação, parede e porta
- Construção em grid, baú de armazenamento e fogueira que ilumina a noite
- Ciclo dia/noite
- Vida, comida, água e stamina; morte com bolsa de itens e respawn
- Save automático em `saves/save.json`, relativo à pasta de onde o jogo é executado

Os próximos passos estão no [ROADMAP.md](ROADMAP.md).

## Jogar

Baixe o pacote do seu sistema na página de [Releases](https://github.com/diegogit03/island/releases), extraia e execute `island` (ou `island.exe`).

- **macOS:** o binário não é assinado. Na primeira vez, clique com o botão direito no arquivo e escolha *Abrir*, ou rode `xattr -d com.apple.quarantine island`.
- **Linux:** requer OpenGL, X11 e ALSA, presentes na maioria das distros com interface gráfica.

### Controles

| Tecla | Ação |
|---|---|
| WASD / setas | Mover |
| Shift | Correr |
| E / clique esquerdo | Coletar; na beira da água, beber |
| R / clique direito | Comer o item selecionado |
| Clique (estrutura na mão) | Posicionar estrutura |
| X | Demolir a estrutura sob o cursor |
| 1–6 / roda do mouse | Selecionar slot da hotbar |
| Tab / I | Inventário e crafting |
| Q / Ctrl+Q | Largar 1 item / a pilha |
| F | Interagir: pegar itens, abrir porta/baú, usar bancada, pôr lenha na fogueira |
| F5 | Salvar |
| H | Mostrar/esconder ajuda |

## Rodar a partir do código

Requer [Rust](https://rustup.rs) estável.

```bash
cargo run
```

No Linux, instale antes: `libasound2-dev libx11-dev libxi-dev libgl1-mesa-dev`.

Comandos de desenvolvimento, arquitetura e convenções estão no [AGENTS.md](AGENTS.md).

## Releases

Ao publicar uma release no GitHub, o workflow [`release.yml`](.github/workflows/release.yml) compila o jogo para Windows (x64), Linux (x64) e macOS (Apple Silicon) e anexa os pacotes na própria release. O [`ci.yml`](.github/workflows/ci.yml) roda formatação, clippy e testes a cada push e pull request.
