# Roadmap — Island (sugestão)

Cada marco é jogável por si só. A ordem prioriza o loop central de survival antes de dinossauros, e dinossauros antes de rede.

## ✅ v0.1 — Fundação
- Personagem com movimento 8 direções e sprite esquerda/direita
- Ilha procedural com praia, água rasa e funda
- Coleta: árvores (madeira/palha), rochas (pedra/sílex), pedrinhas no chão
- Inventário de 30 slots com hotbar, empilhamento, arrastar/dividir pilhas
- Crafting: picareta, machado, lança, fogueira
- Ferramentas alteram o rendimento (estilo ARK)

## ✅ v0.2 — Sobrevivência básica
- Atributos: vida, comida, água, stamina (correr com Shift, exaustão)
- Arbustos de frutinhas (comer com R / botão direito), beber água com E na beira
- Morte e respawn na praia, com bolsa dos itens (e feixe de luz) no local da morte
- Durabilidade de ferramentas (barra no slot, quebram ao zerar)
- Largar itens (Q / Ctrl+Q / soltar fora do inventário) e pegar de volta (F)
- Save/load em JSON (F5, autosave a cada 60 s, ao fechar a janela)

## v0.3 — Construção
- Colocar estruturas em grid: fogueira (luz, cozinhar carne), fundação, paredes, porta de madeira/palha
- Baús de armazenamento
- Ciclo dia/noite com iluminação (fogueira importa à noite)
- Bancada / estações de crafting e fila de crafting com tempo

## v0.4 — Fauna e combate
- Criaturas passivas (dodô, trilobita) e agressivas (dilofossauro, raptor) com IA simples (vagar, fugir, perseguir)
- Combate corpo a corpo e lança; carne crua/cozida, couro
- Spawn por bioma e limites de população

## v0.5 — Domesticação ⭐
- Nocaute (torpor) com estilingue/clava + narcóticos
- Barra de domesticação alimentando o dino desacordado (efetividade como no ARK)
- Dinos domesticados: seguir/ficar, montar (selas craftáveis), carregar inventário
- Níveis e atributos de criaturas

## v0.6 — Progressão RPG
- Nível e XP do jogador, pontos de atributo
- Engramas: desbloqueio de receitas por nível
- Novos tiers: metal, forja, ferramentas de metal
- Biomas novos na ilha (selva, montanha, pântano) e cavernas

## v0.7 — Conteúdo e polimento
- Sprites pixel art substituindo as primitivas (spritesheets + animações)
- Áudio (macroquad já suporta), menus, opções, rebind de teclas
- Mapa/minimapa, notas exploráveis, chefes simples

## v1.0+ — Multiplayer / MMO
- Separar simulação de apresentação (servidor autoritativo headless)
- Rede (ex.: `renet`/`quinn`), sincronização de entidades, interpolação
- Tribos, chat, PvE/PvP configurável, persistência em banco
- Avaliar migração para ECS (ex.: Bevy) se a contagem de entidades e a rede exigirem

## Decisões técnicas a revisitar
- **Engine:** macroquad é simples e compila rápido; se o projeto crescer muito (ECS, rede, editor), migrar para Bevy entre v0.4 e v1.0. Manter a lógica em módulos puros facilita essa troca.
- **Mapa:** hoje é uma ilha 160×160 tiles em memória; mundos maiores pedem chunks.
- **Dados:** itens/receitas em código agora; mover para arquivos (RON) quando houver muitos.
