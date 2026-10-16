mod crafting;
mod game;
mod gathering;
mod inventory;
mod items;
mod player;
mod render;
mod rng;
mod ui;
mod world;

use macroquad::prelude::*;

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

#[macroquad::main(window_conf)]
async fn main() {
    let seed = env("ISLAND_SEED").and_then(|s| s.parse().ok()).unwrap_or(1337);
    let mut game = game::Game::new(seed);

    // Smoke test: ISLAND_SHOT=caminho.png salva um screenshot e encerra.
    // ISLAND_SHOT_INVENTORY=1 abre o inventário com itens de debug antes da captura.
    let shot = env("ISLAND_SHOT");
    if shot.is_some() && env("ISLAND_SHOT_INVENTORY").is_some() {
        game.give_debug_items();
        game.open_inventory();
    }

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
        next_frame().await;
    }
}
