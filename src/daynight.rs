//! Ciclo dia/noite derivado do tempo de jogo (segundos desde o início).

/// Um dia completo (24h no jogo) dura 12 minutos reais.
pub const DAY_LENGTH: f32 = 720.0;
/// O jogo começa às 8h do dia 1.
const START_HOUR: f32 = 8.0;
/// Escuridão máxima no meio da noite (1 = preto total).
const MAX_DARKNESS: f32 = 0.85;

fn total_hours(time: f32) -> f32 {
    START_HOUR + time / DAY_LENGTH * 24.0
}

/// Hora do dia em [0, 24).
pub fn hour(time: f32) -> f32 {
    total_hours(time).rem_euclid(24.0)
}

/// Dia atual, começando em 1.
pub fn day(time: f32) -> u32 {
    (total_hours(time) / 24.0).floor() as u32 + 1
}

/// Segundos de jogo a somar para chegar à próxima ocorrência de `target_hour`.
pub fn secs_until(time: f32, target_hour: f32) -> f32 {
    (target_hour - hour(time)).rem_euclid(24.0) / 24.0 * DAY_LENGTH
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 0 = dia claro, `MAX_DARKNESS` = meia-noite. Amanhece 5h–7h, anoitece 18h–20h.
pub fn darkness(hour: f32) -> f32 {
    let dawn = 1.0 - smoothstep(5.0, 7.0, hour);
    let dusk = smoothstep(18.0, 20.0, hour);
    dawn.max(dusk) * MAX_DARKNESS
}

/// Intensidade (0..1) do tom alaranjado do nascer/pôr do sol.
pub fn golden_hour(hour: f32) -> f32 {
    let bump = |center: f32| (1.0 - ((hour - center).abs() / 1.2)).max(0.0);
    bump(6.0).max(bump(18.6))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_starts_in_the_morning_and_wraps() {
        assert_eq!(hour(0.0), 8.0);
        assert_eq!(day(0.0), 1);
        let one_day = DAY_LENGTH;
        assert!((hour(one_day) - 8.0).abs() < 1e-3);
        assert_eq!(day(one_day), 2);
        assert_eq!(day(secs_until(0.0, 0.0)), 2, "meia-noite vira o dia");
    }

    #[test]
    fn noon_is_bright_and_midnight_is_dark() {
        assert_eq!(darkness(12.0), 0.0);
        assert_eq!(darkness(0.0), MAX_DARKNESS);
        assert!(darkness(19.0) > 0.0 && darkness(19.0) < MAX_DARKNESS);
    }

    #[test]
    fn secs_until_reaches_target() {
        let t = 123.0;
        let dt = secs_until(t, 22.0);
        assert!((hour(t + dt) - 22.0).abs() < 1e-3);
    }
}
