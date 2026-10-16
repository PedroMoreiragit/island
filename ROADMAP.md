# Roadmap — Island (sugestão)

Cada marco é jogável por si só. A ordem prioriza o loop central de survival antes de dinossauros, dinossauros antes de rede,
e rede antes do polimento (arte, áudio e conteúdo novo são feitos já sobre a arquitetura multiplayer).

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

## ✅ v0.3 — Construção
- Estruturas em grid com prévia verde/vermelha: fundação, parede e porta de madeira (paredes em 3/4)
- Fogueira que queima madeira e ilumina a noite
- Caixa de armazenamento (18 slots, Shift+clique transfere)
- Bancada: receitas de construção exigem estar perto dela
- Fila de crafting com tempo por receita (cancelável, devolve materiais)
- Ciclo dia/noite (12 min reais) com iluminação suave e entardecer alaranjado
- Demolir estruturas (X) devolve o item

## ✅ v0.4 — Fauna e combate
- Criaturas passivas (dodô, trilobita) e agressivas (dilofossauro, raptor) com IA simples (vagar, fugir, perseguir, atacar)
- Combate corpo a corpo com dano por arma (lança: mais dano e alcance) e durabilidade
- Carcaças coletáveis: machado rende carne, picareta rende couro
- Fogueira com inventário: queima madeira/palha e cozinha carne (carne crua faz mal)
- Spawn por bioma (praia, campo, floresta), limite de população e zonas seguras (spawn e bases)
- Ainda não: estragar comida e armaduras

## ✅ v0.5 — Domesticação ⭐
- Torpor e desmaio: clava (corpo a corpo) e estilingue (pedras, mira no cursor); torpor some com o tempo
- Narcóticos no inventário do dino desacordado o mantêm dormindo
- Domesticação alimentando o dino desacordado (herbívoros: frutinhas; carnívoros: carne crua > cozida),
  com efetividade que cai a cada golpe e define os níveis bônus
- Dinos domesticados: seguir/ficar, inventário de carga, regeneração de vida
- Montaria: raptor com Sela de Raptor (feita de couro na bancada); a montaria morde e absorve o dano
- Níveis 1–30 na natureza, escalando vida, torpor e dano
- Ainda não: domesticados lutando sozinhos, fome/ração de domesticados, XP de criaturas

## ✅ v0.6 — Progressão RPG
- XP e nível do jogador (até 30): coleta, crafting, abates e domesticação dão XP
- Pontos de atributo: vida, stamina, comida, água e dano corpo a corpo
- Engramas: receitas têm nível mínimo e custo em pontos; clicar na receita com cadeado aprende
- Bioma de montanha no interior da ilha, com rochas de metal (picareta rende minério)
- Tier de metal: forja (funde 2 minérios em 1 lingote), picareta, machado (rendem o dobro) e espada de metal
- Saves antigos mantêm as receitas que já existiam liberadas
- Movido para a v0.8: biomas de selva e pântano, cavernas

## v0.7 — Multiplayer
- Separar simulação de apresentação: estado do mundo num servidor autoritativo (headless) e cliente que só desenha e envia input
- Rede (ex.: `renet`/`quinn`), sincronização de entidades com interpolação e predição do próprio jogador
- Co-op online para poucos jogadores: entrar/sair, nomes, chat
- Tribos: dinos domesticados, estruturas e baús pertencem à tribo
- Save do servidor (mundo + todos os jogadores), PvE com PvP opcional
- Avaliar migração para ECS (ex.: Bevy) se a contagem de entidades e a rede exigirem

## v0.8 — Conteúdo e polimento
- Sprites pixel art substituindo as primitivas (spritesheets + animações)
- Áudio (macroquad já suporta), menus, opções, rebind de teclas
- Biomas de selva e pântano, cavernas
- Mapa/minimapa, notas exploráveis, chefes simples

## v1.0+ — MMO
- Servidores persistentes com banco de dados e muitos jogadores
- Mundos maiores em chunks, várias ilhas
- Economia entre jogadores, PvE/PvP por servidor

## Decisões técnicas a revisitar
- **Engine:** macroquad é simples e compila rápido; se o projeto crescer muito (ECS, rede, editor), migrar para Bevy na v0.7, junto com a separação servidor/cliente. Manter a lógica em módulos puros facilita essa troca.
- **Mapa:** hoje é uma ilha 160×160 tiles em memória; mundos maiores pedem chunks.
- **Dados:** itens/receitas em código agora; mover para arquivos (RON) quando houver muitos.
