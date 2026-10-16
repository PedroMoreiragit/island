// Em release no Windows, abre só a janela do jogo (sem console).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod building;
mod combat;
mod crafting;
mod creatures;
mod daynight;
mod game;
mod gathering;
mod inventory;
mod items;
mod player;
mod progression;
mod render;
mod rng;
mod save;
mod taming;
mod ui;
mod vitals;
mod world;

use std::path::{Path, PathBuf};

use macroquad::prelude::*;

use game::Game;

const DEFAULT_SAVE: &str = "saves/save.json";

fn window_conf() -> Conf {
    Conf {
        window_title: "Island".to_owned(),
        window_width: 1280,
        window_height: 720,
        ..Default::default()
    }
}

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// Carrega o save se existir. Um save inválido é preservado como `.bak` antes de começar do zero.
fn load_or_new(path: &Path, seed: u64) -> Game {
    match save::read(path) {
        Ok(Some(data)) => Game::from_save(data),
        Ok(None) => Game::new(seed),
        Err(e) => {
            let backup = path.with_extension("json.bak");
            eprintln!("Save inválido ({e}); copiado para {}", backup.display());
            let _ = std::fs::copy(path, &backup);
            Game::new(seed)
        }
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let seed = env("ISLAND_SEED").and_then(|s| s.parse().ok()).unwrap_or(1337);

    // Smoke test: ISLAND_SHOT=caminho.png salva um screenshot e encerra (sem ler/gravar save).
    // ISLAND_SHOT_INVENTORY=1 abre o inventário com itens de debug; ISLAND_SHOT_DEAD=1 mostra a morte;
    // ISLAND_SHOT_BUILD=1 monta uma cabana de exemplo; ISLAND_SHOT_HOUR=22 ajusta o relógio;
    // ISLAND_SHOT_CREATURES=1 coloca uma criatura de cada espécie perto do jogador;
    // ISLAND_SHOT_FIRE=1 monta a cabana e abre o painel da fogueira;
    // ISLAND_SHOT_TAME=1 mostra um raptor desacordado com o painel de domesticação;
    // ISLAND_SHOT_RIDE=1 mostra o jogador montado num raptor, com um dodô domesticado e um dilo desacordado;
    // ISLAND_SHOT_PET=1 abre o painel do raptor domesticado;
    // ISLAND_SHOT_MOUNTAIN=1 leva à montanha, perto de metal, com uma forja acesa.
    let shot = env("ISLAND_SHOT");
    let mut game = if shot.is_some() {
        let mut game = Game::new(seed);
        if env("ISLAND_SHOT_INVENTORY").is_some() {
            game.give_debug_items();
            game.debug_xp(1500);
            game.open_inventory();
        }
        if env("ISLAND_SHOT_MOUNTAIN").is_some() {
            game.demo_mountain();
        }
        if env("ISLAND_SHOT_CREATURES").is_some() {
            game.spawn_demo_creatures();
        }
        if env("ISLAND_SHOT_BUILD").is_some() {
            game.build_demo_base();
        }
        if env("ISLAND_SHOT_TAME").is_some() {
            game.demo_taming();
        }
        if env("ISLAND_SHOT_RIDE").is_some() {
            game.demo_riding();
        }
        if env("ISLAND_SHOT_PET").is_some() {
            game.demo_pet_panel();
        }
        if env("ISLAND_SHOT_FIRE").is_some() {
            game.build_demo_base();
            game.open_first_campfire();
        }
        if let Some(hour) = env("ISLAND_SHOT_HOUR").and_then(|h| h.parse().ok()) {
            game.skip_to_hour(hour);
        }
        if env("ISLAND_SHOT_DEAD").is_some() {
            game.give_debug_items();
            game.kill();
        }
        game
    } else {
        let save_path = PathBuf::from(env("ISLAND_SAVE").unwrap_or_else(|| DEFAULT_SAVE.to_owned()));
        let mut game = if env("ISLAND_NEW").is_some() {
            Game::new(seed)
        } else {
            load_or_new(&save_path, seed)
        };
        game.set_save_path(save_path);
        // Intercepta o fechamento da janela para salvar antes de sair.
        prevent_quit();
        game
    };

    let mut frame = 0u32;
    loop {
        let dt = get_frame_time().min(1.0 / 20.0);
        game.update(dt);
        game.draw();

        frame += 1;
        if let Some(path) = &shot
            && frame == 20
        {
            get_screen_data().export_png(path);
            break;
        }
        if is_quit_requested() {
            game.save();
            break;
        }
        next_frame().await;
    }
}
