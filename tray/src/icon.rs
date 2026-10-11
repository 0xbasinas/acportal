//! Tray icons drawn in code (no image files to ship): the ACP Portal mark (a ring with a prompt
//! chevron, see assets/logo) in one colour that shows the host state. A single colour with no tile
//! stays readable on light and dark taskbars.
use crate::host::HostState;

const SIZE: u32 = 64;
const SUPERSAMPLE: u32 = 4;

fn colour(state: &HostState) -> [u8; 3] {
    match state {
        HostState::Running { .. } => [0x2f, 0xbf, 0x8f],
        HostState::Stopped => [0x8a, 0x8a, 0x93],
        HostState::NotSetUp => [0xe0, 0xa0, 0x30],
        HostState::NotInstalled => [0x4c, 0x8d, 0xf6],
        HostState::Other(_) => [0xe5, 0x53, 0x4b],
    }
}

/// Distance from point `p` to the segment `a`-`b`.
fn segment_distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (ab, ap) = ((b.0 - a.0, b.1 - a.1), (p.0 - a.0, p.1 - a.1));
    let t = ((ap.0 * ab.0 + ap.1 * ab.1) / (ab.0 * ab.0 + ab.1 * ab.1)).clamp(0.0, 1.0);
    ((p.0 - (a.0 + t * ab.0)).powi(2) + (p.1 - (a.1 + t * ab.1)).powi(2)).sqrt()
}

/// Whether the point (in the 512-unit logo grid) is on the mark: the ring (radius 152, stroke 36) or
/// the chevron (stroke 36 with round ends and a round tip).
fn on_mark(x: f32, y: f32) -> bool {
    let ring = (((x - 256.0).powi(2) + (y - 256.0).powi(2)).sqrt() - 152.0).abs() <= 18.0;
    let (top, tip, bottom) = ((222.0, 188.0), (296.0, 256.0), (222.0, 324.0));
    let chevron =
        segment_distance((x, y), top, tip) <= 18.0 || segment_distance((x, y), tip, bottom) <= 18.0;
    ring || chevron
}

/// RGBA pixels, anti-aliased by supersampling.
pub fn rgba(state: &HostState) -> Vec<u8> {
    let [r, g, b] = colour(state);
    // The logo's outer ring edge is 170 units from the centre of a 512 grid; scale it to fill the
    // icon (with a 1 px margin) so the mark is as large as it can be.
    let scale = 512.0 * (SIZE as f32 / 2.0 - 1.0) / 170.0 / SIZE as f32;
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for py in 0..SIZE {
        for px in 0..SIZE {
            let mut hits = 0;
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let fx = px as f32 + (sx as f32 + 0.5) / SUPERSAMPLE as f32;
                    let fy = py as f32 + (sy as f32 + 0.5) / SUPERSAMPLE as f32;
                    // Pixel to logo units, centred on the icon.
                    let x = 256.0 + (fx - SIZE as f32 / 2.0) * 512.0 / SIZE as f32 / scale;
                    let y = 256.0 + (fy - SIZE as f32 / 2.0) * 512.0 / SIZE as f32 / scale;
                    if on_mark(x, y) {
                        hits += 1;
                    }
                }
            }
            let alpha = (hits * 255 / (SUPERSAMPLE * SUPERSAMPLE)) as u8;
            pixels.extend_from_slice(&[r, g, b, alpha]);
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
    fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }
    #[test]
    fn mark_has_a_ring_a_chevron_an_empty_gap_and_transparent_corners() {
        let pixels = rgba(&HostState::Running { pid: 1 });
        assert_eq!(pixels.len(), (SIZE * SIZE * 4) as usize);
        assert_eq!(at(&pixels, 0, 0)[3], 0, "corner is transparent");
        let middle = SIZE / 2;
        // On the ring, to the right of the centre (ring radius ~ 152/512 of the grid, scaled).
        let ring_x = middle + (SIZE as f32 * 0.4) as u32;
        assert_eq!(at(&pixels, ring_x, middle)[3], 255, "ring is opaque");
        // Between the chevron tip and the ring there is a gap.
        let gap_x = middle + (SIZE as f32 * 0.2) as u32;
        assert_eq!(
            at(&pixels, gap_x, middle)[3],
            0,
            "gap inside the ring is empty"
        );
        // The chevron tip sits just right of centre.
        let tip_x = middle + (SIZE as f32 * 0.1) as u32;
        assert_eq!(at(&pixels, tip_x, middle)[3], 255, "chevron is opaque");
    }
    #[test]
    fn each_state_has_its_own_colour() {
        let states = [
            HostState::Running { pid: 1 },
            HostState::Stopped,
            HostState::NotSetUp,
            HostState::NotInstalled,
            HostState::Other("x".into()),
        ];
        let ring_x = SIZE / 2 + (SIZE as f32 * 0.4) as u32;
        let colours: Vec<[u8; 3]> = states
            .iter()
            .map(|state| {
                let p = at(&rgba(state), ring_x, SIZE / 2);
                [p[0], p[1], p[2]]
            })
            .collect();
        for (i, a) in colours.iter().enumerate() {
            for b in &colours[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
