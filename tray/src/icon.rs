//! Tray icons drawn in code (no image files to ship): a filled dot whose colour shows the state.
use crate::host::HostState;

const SIZE: u32 = 32;

fn colour(state: &HostState) -> [u8; 3] {
    match state {
        HostState::Running { .. } => [0x72, 0xd5, 0xb0],
        HostState::Stopped => [0x9c, 0x9c, 0xa4],
        HostState::NotSetUp => [0xe0, 0xa0, 0x30],
        HostState::Other(_) => [0xe5, 0x6b, 0x6b],
    }
}

/// RGBA pixels for a smooth-edged dot with a dark ring so it reads on light and dark taskbars.
pub fn rgba(state: &HostState) -> Vec<u8> {
    let [r, g, b] = colour(state);
    let centre = (SIZE as f32 - 1.0) / 2.0;
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let distance = ((x as f32 - centre).powi(2) + (y as f32 - centre).powi(2)).sqrt();
            let fill = (13.5 - distance).clamp(0.0, 1.0); // dot, 1px soft edge
            let outer = (15.5 - distance).clamp(0.0, 1.0); // ring edge
            let (red, green, blue) = (
                (r as f32 * fill + 30.0 * (1.0 - fill)) as u8,
                (g as f32 * fill + 30.0 * (1.0 - fill)) as u8,
                (b as f32 * fill + 36.0 * (1.0 - fill)) as u8,
            );
            pixels.extend_from_slice(&[red, green, blue, (outer * 255.0) as u8]);
        }
    }
    pixels
}

pub fn icon(state: &HostState) -> tray_icon::Icon {
    tray_icon::Icon::from_rgba(rgba(state), SIZE, SIZE).expect("valid icon size")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn each_state_has_a_distinct_opaque_centre_and_transparent_corners() {
        let states = [
            HostState::Running { pid: 1 },
            HostState::Stopped,
            HostState::NotSetUp,
            HostState::Other("x".into()),
        ];
        let centres: Vec<[u8; 4]> = states
            .iter()
            .map(|state| {
                let pixels = rgba(state);
                assert_eq!(pixels.len(), (SIZE * SIZE * 4) as usize);
                assert_eq!(pixels[3], 0, "corner is transparent");
                let at = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
                assert_eq!(pixels[at + 3], 255, "centre is opaque");
                [pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]]
            })
            .collect();
        for (i, a) in centres.iter().enumerate() {
            for b in &centres[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
