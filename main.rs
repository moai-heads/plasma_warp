// plasma_warp: scene-based demo engine
// Scenes: Rotozoom (plasma warp + ripple + rotozoom), TriangleDance (pyramid + blurred bg)
// Rust, direct rustc, image vault per POLICY. Renders PNG frames for ffmpeg.
use std::path::Path;
use image::{ImageBuffer, Rgb};

const W: usize = 640;
const H: usize = 360;
const FPS: u32 = 30;
const SEG_SECS: f32 = 8.0;
const FADE_SECS: f32 = 1.5;   // fade-to-black / fade-in duration

#[derive(Clone, Copy, PartialEq, Debug)]
enum Scene {
    Rotozoom,
    TriangleDance,
}

// ---------------- Beat sync system (waveform-extracted) ----------------
// Snare timestamps are EXTRACTED from the audio waveform offline:
//   1. decode ogg -> mono f32 PCM (ffmpeg)
//   2. high-frequency energy envelope (snares are bright; kicks are low)
//   3. comb-fit period + phase over the envelope -> grid of hit times
//   4. grid written to beats.txt; renderer reads it at startup.
// punch(t) returns a damped-sine impulse around each real snare timestamp.
// DESIGN NOTE (locked 2026-09-10, by user decree): the punch envelope's
// secondary lobe lands ~half a snare period later, which coincides with the
// track's KICK transients (beat is kick-snare at double rate). This makes the
// visuals pulse dominantly on the KICKS — an accidental discovery that tested
// as maxed vibes. DO NOT "fix" the envelope to be snare-only; the kick
// dominance is intended behavior.
struct BeatSync {
    beats: Vec<f32>, // extracted snare timestamps (seconds)
    span: f32,       // TRUE audio file length -- must match the actual loop point
    amp: f32,        // punch strength
}
impl BeatSync {
    fn load(path: &str, amp: f32, song_len: f32) -> Self {
        let txt = std::fs::read_to_string(path).expect("beats.txt");
        let beats: Vec<f32> = txt.lines().filter_map(|l| l.trim().parse().ok()).collect();
        assert!(!beats.is_empty(), "no beats");
        BeatSync { span: song_len, beats, amp }
    }
    #[inline]
    fn punch(&self, t: f32) -> f32 {
        // wrap at the TRUE song length so renderer loop == audio loop, no drift.
        // beat search is CYCLIC: before the first snare of a cycle, the "previous
        // hit" is the last snare of the previous cycle (t - span), which handles
        // the shortened gap across the loop boundary.
        let tc = t % self.span;
        let mut best: Option<f32> = None;
        for &b in &self.beats { if b <= tc { best = Some(b); } else { break; } }
        let e = match best {
            Some(b) => tc - b,
            None => tc + self.span - *self.beats.last().unwrap(), // wrap-around gap
        };
        (-6.0 * e).exp() * (16.0 * e).sin() * self.amp
    }
}

struct Tex {
    w: u32,
    h: u32,
    px: Vec<u8>,          // packed RGB, 3 bytes/px
    alpha: Option<Vec<u8>>, // per-px alpha, only if source had transparency
}
impl Tex {
    fn load(p: &str) -> Self {
        let img = image::open(Path::new(p)).expect("open tex").to_rgba8();
        let (w, h) = img.dimensions();
        let raw = img.into_raw();
        let mut px = Vec::with_capacity((w * h) as usize * 3);
        let mut alpha = Vec::with_capacity((w * h) as usize);
        for c in raw.chunks_exact(4) {
            px.extend_from_slice(&c[0..3]);
            alpha.push(c[3]);
        }
        // store alpha only if the image actually uses transparency
        let alpha = if alpha.iter().any(|&a| a != 255) { Some(alpha) } else { None };
        Tex { w, h, px, alpha }
    }
    /// sample RGB + alpha (0..1), clamped coords — for overlay-style drawing
    #[inline]
    fn sample_clamp_rgba(&self, u: f32, v: f32) -> ([f32; 3], f32) {
        let x = ((u * (self.w as f32 - 1.0)) as usize).min(self.w as usize - 1);
        let y = ((v * (self.h as f32 - 1.0)) as usize).min(self.h as usize - 1);
        let i = y * self.w as usize + x;
        let a = self.alpha.as_ref().map(|v| v[i] as f32 / 255.0).unwrap_or(1.0);
        let j = i * 3;
        ([self.px[j] as f32, self.px[j + 1] as f32, self.px[j + 2] as f32], a)
    }
    #[inline]
    fn sample(&self, u: f32, v: f32) -> [f32; 3] {
        let u = u - u.floor();
        let v = v - v.floor();
        let x = (u * self.w as f32) as isize;
        let y = (v * self.h as f32) as isize;
        let xi = (x.rem_euclid(self.w as isize)) as usize % self.w as usize;
        let yi = (y.rem_euclid(self.h as isize)) as usize % self.h as usize;
        let i = (yi * self.w as usize + xi) * 3;
        [self.px[i] as f32, self.px[i + 1] as f32, self.px[i + 2] as f32]
    }
    #[inline]
    fn sample_clamp(&self, u: f32, v: f32) -> [f32; 3] {
        let x = (u * (self.w as f32 - 1.0)) as usize;
        let y = (v * (self.h as f32 - 1.0)) as usize;
        let i = (y * self.w as usize + x) * 3;
        [self.px[i] as f32, self.px[i + 1] as f32, self.px[i + 2] as f32]
    }
    fn blur(&self) -> Tex {
        let small = image::imageops::resize(
            &ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(self.w, self.h, self.px.clone()).unwrap(),
            self.w / 8, self.h / 8, image::imageops::FilterType::Triangle);
        let up = image::imageops::resize(&small, self.w, self.h, image::imageops::FilterType::Triangle);
        let alpha = self.alpha.as_ref().map(|al| {
            let small = image::imageops::resize(
                &ImageBuffer::<image::Luma<u8>, Vec<u8>>::from_raw(self.w, self.h, al.clone()).unwrap(),
                self.w / 8, self.h / 8, image::imageops::FilterType::Triangle);
            let up = image::imageops::resize(&small, self.w, self.h, image::imageops::FilterType::Triangle);
            up.into_raw()
        });
        Tex { w: up.width(), h: up.height(), px: up.into_raw(), alpha }
    }
}

#[inline]
fn smooth(k: f32) -> f32 { k * k * (3.0 - 2.0 * k) }

// ---------------- Scene: Rotozoom ----------------

#[inline]
fn rotozoom_uv(gx: f32, gy: f32, t: f32, punch: f32) -> (f32, f32) {
    let x = gx / W as f32 - 0.5;
    let y = gy / H as f32 - 0.5;
    let r = (x * x + y * y).sqrt();
    let ang = y.atan2(x);
    let env2 = (0.5 + 0.5 * (t * 0.55 + 0.7).sin()).powf(1.5);
    let p = (x * 9.0 + t * 1.3).sin()
        + (y * 7.0 - t * 1.1).sin()
        + ((x * x + y * y) * 14.0 - t * 2.0).sin()
        + (r * 8.0 - t * 2.4).sin() * 0.5;
    let swirl = 0.9 * p * 0.25 + t * 0.35 * (r * 2.0 + 0.3).sin();
    let a2 = ang + swirl * 0.7 * (0.15 + 0.85 * env2);
    let rr = r * (1.0 + 0.22 * env2 * (t * 1.7 + r * 10.0).sin());
    let sx = rr * a2.cos();
    let sy = rr * a2.sin();
    let env = (0.5 + 0.5 * (t * 0.55).sin()).powf(1.5) * (0.6 + 0.4 * (t * 0.9 + 1.3).cos());
    let rip = env * (0.035 * (r * 34.0 - t * 5.0).sin()
        + 0.025 * ((sx * 18.0 + t * 3.0).sin() * (sy * 14.0 - t * 2.2).cos()));
    let rz = (0.5 + 0.5 * (t * 0.55).sin()).powf(1.2);
    let ang_r = t * 0.9 * rz + rz * rz * 0.4;
    let (ca, sa) = (ang_r.cos(), ang_r.sin());
    let zoom = (1.0 + 0.6 * rz) * (1.0 + punch); // snare punch: quick zoom-in, spring back
    let pan_x = t * 0.45 * rz;
    let pan_y = 0.25 * (t * 0.3).sin() * rz;
    let (sx2, sy2) = (sx * ca - sy * sa, sx * sa + sy * ca);
    let u = (sx2 * 1.6 + pan_x) / zoom + 0.5 + rip * sx;
    let v = (sy2 * 1.6 + pan_y) / zoom + 0.5 + rip * sy;
    (u, v)
}

fn frame_rotozoom(t: &Tex, tb: &Tex, nt: &Tex, nb: &Tex, gt: f32, mix: f32, punch: f32)
    -> ImageBuffer<Rgb<u8>, Vec<u8>>
{
    let mut img = ImageBuffer::new(W as u32, H as u32);
    let glow = 0.6 + 0.4 * (gt * 0.8).sin();
    for y in 0..H {
        for x in 0..W {
            let (u, v) = rotozoom_uv(x as f32, y as f32, gt, punch);
            let c1 = t.sample(u, v);
            let base = if mix > 0.0 {
                let c2 = nt.sample(u, v);
                [c1[0] + (c2[0] - c1[0]) * mix,
                 c1[1] + (c2[1] - c1[1]) * mix,
                 c1[2] + (c2[2] - c1[2]) * mix]
            } else { c1 };
            let out = if mix > 0.05 && mix < 0.95 {
                let g1 = tb.sample(u * 0.9 + gt * 0.05, v * 0.9 - gt * 0.04);
                let g2 = nb.sample(u * 0.9 + gt * 0.05, v * 0.9 - gt * 0.04);
                let k = (mix * 2.0).min(1.0) * (1.0 - mix) * 2.0;
                [base[0] + (g1[0] * (1.0 - mix) + g2[0] * mix - base[0]) * k * glow,
                 base[1] + (g1[1] * (1.0 - mix) + g2[1] * mix - base[1]) * k * glow,
                 base[2] + (g1[2] * (1.0 - mix) + g2[2] * mix - base[2]) * k * glow]
            } else { base };
            img.put_pixel(x as u32, y as u32,
                Rgb([out[0].clamp(0.0, 255.0) as u8, out[1].clamp(0.0, 255.0) as u8, out[2].clamp(0.0, 255.0) as u8]));
        }
    }
    img
}

// ---------------- Scene: TriangleDance ----------------

#[inline]
fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (px - ax) * (by - ay) - (py - ay) * (bx - ax)
}

// Shader = where a fragment's color comes from. Blend = how it lands. Orthogonal.
#[derive(Clone, Copy, Debug)]
enum Surface {
    Solid { base: (f32, f32, f32) },          // flat shaded color (current look)
    Refract { strength: f32, chroma: f32 },   // sample bg pixels with a normal-driven offset
}

// Which surface shader the mesh renders with (toggle to test).
#[derive(Clone, Copy, Debug, PartialEq)]
enum SurfaceKind {
    Solid,   // flat shaded colors (classic)
    Refract, // bg pixels sampled through the faces with normal-driven offset
}

// Mesh punch mode: does the snare punch tint the mesh (solid) or leave it pure glass?
#[derive(Clone, Copy, Debug, PartialEq)]
enum MeshPunch {
    SolidColor, // punch scales AND brightens the mesh (current behavior)
    Glass,      // punch scales only; mesh shows bg through refraction, untouched by flash
}

#[derive(Clone, Copy, Debug)]
enum Blend {
    Alpha,   // src-over: normal transparency
    Add,     // additive: colors accumulate (glow / energy look)
    Screen,  // screen blend: 1-(1-a)(1-b), soft light-accumulation
}

// Sample the image at a pixel offset (clamped to the frame). Used by the
// Refract shader: reads the already-composited background through the mesh.
fn sample_bg(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, x: i32, y: i32, dx: f32, dy: f32) -> [f32; 3] {
    let sx = (x as f32 + dx).clamp(0.0, W as f32 - 1.0) as u32;
    let sy = (y as f32 + dy).clamp(0.0, H as f32 - 1.0) as u32;
    let p = img.get_pixel(sx, sy).0;
    [p[0] as f32, p[1] as f32, p[2] as f32]
}

// offset: per-face refraction shift in pixels (0.0 for Solid). chroma spreads R/G/B.
#[allow(clippy::too_many_arguments)]
fn fill_tri_flat(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, depth: &mut Vec<f32>,
                 p: [(f32, f32); 3], dz: [f32; 3],
                 surface: Surface, lam: f32,
                 off: (f32, f32), chroma: f32,
                 alpha: f32, mode: Blend) {
    let col: (f32, f32, f32) = match surface {
        Surface::Solid { base } => (base.0 * lam * 255.0, base.1 * lam * 255.0, base.2 * lam * 255.0),
        Surface::Refract { .. } => (0.0, 0.0, 0.0), // computed per fragment below
    };
    let refr = match surface { Surface::Refract { strength, chroma: ch } => Some((strength, ch)), _ => None };
    // alpha: face opacity 0..1. bg shows through via src-over blend; the
    // depth buffer still gates writes so transparency never re-draws behind.

    let area = edge(p[0].0, p[0].1, p[1].0, p[1].1, p[2].0, p[2].1);
    if area.abs() < 1e-6 { return; }
    let sign = area > 0.0;
    let minx = p.iter().fold(f32::MAX, |m, q| m.min(q.0)).max(0.0) as i32;
    let maxx = p.iter().fold(f32::MIN, |m, q| m.max(q.0)).min(W as f32 - 1.0) as i32;
    let miny = p.iter().fold(f32::MAX, |m, q| m.min(q.1)).max(0.0) as i32;
    let maxy = p.iter().fold(f32::MIN, |m, q| m.max(q.1)).min(H as f32 - 1.0) as i32;
    for y in miny..=maxy {
        for x in minx..=maxx {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let mut w0 = edge(p[1].0, p[1].1, p[2].0, p[2].1, px, py);
            let mut w1 = edge(p[2].0, p[2].1, p[0].0, p[0].1, px, py);
            let mut w2 = edge(p[0].0, p[0].1, p[1].0, p[1].1, px, py);
            if !sign { w0 = -w0; w1 = -w1; w2 = -w2; }
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let a = area.abs();
                let z = (w0 * dz[0] + w1 * dz[1] + w2 * dz[2]) / a;
                let idx = y as usize * W + x as usize;
                if z < depth[idx] {
                    depth[idx] = z;
                    let old = img.get_pixel(x as u32, y as u32).0;
                    let cc: [f32; 3] = if let Some((_strength, _ch)) = refr {
                        // refract: sample the composed bg (img pre-write at this frag)
                        // at the fragment pos shifted by the normal-driven offset.
                        // chroma: R/G/B sampled at slightly different strengths.
                        let sam = |k: f32| sample_bg(img, x, y, off.0 * k, off.1 * k);
                        let (mr, mg, mb) = (1.0 - chroma, 1.0, 1.0 + chroma);
                        let s0 = sam(mr); let s1 = sam(mg); let s2 = sam(mb);
                        // green emissive: keeps the glass luminous instead of dark
                        let g = lam * 110.0;
                        [s0[0] * lam + g * 0.12, s1[1] * lam + g, s2[2] * lam + g * 0.45]
                    } else { [col.0, col.1, col.2] };
                    let out: [u8; 3] = (0..3).map(|c| {
                        let o = old[c] as f32;
                        let s = cc[c].clamp(0.0, 255.0);
                        let v = match mode {
                            Blend::Alpha  => o + (s - o) * alpha,
                            Blend::Add    => o + s * alpha,
                            Blend::Screen => 255.0 - (255.0 - o) * (255.0 - s * alpha) / 255.0,
                        };
                        v.clamp(0.0, 255.0) as u8
                    }).collect::<Vec<u8>>().try_into().unwrap();
                    img.put_pixel(x as u32, y as u32, Rgb(out));
                }
            }
        }
    }
}

fn frame_tri(ts: &Tex, tb: &Tex, st: f32, gt: f32, punch: f32, beat: &BeatSync) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    // scene-local snare schedule: global beats mapped into scene time (mod song span)
    let start: f32 = 16.0; // scene 2 begins at demo t=16s
    let mut ls: Vec<f32> = beat.beats.iter().map(|&b| (b - start).rem_euclid(beat.span)).collect();
    ls.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pumped: usize = ls.iter().filter(|&&b| b <= st).count();
    let drop_t = ls[2]; // mesh drops ON the 3rd snare
    let mut img = ImageBuffer::new(W as u32, H as u32);
    // background: blurred blue texture drawn ONCE, centered (cover-fit), no wrap.
    // outside its rect = black. subtle wobble on the sampling uv (clamped).
    let ar_t = tb.w as f32 / tb.h as f32;
    let ar_s = W as f32 / H as f32;
    let (dw, dh) = if ar_t > ar_s { (W as f32, W as f32 / ar_t) } else { (H as f32 * ar_t, H as f32) };
    let ox = (W as f32 - dw) * 0.5;
    let oy = (H as f32 - dh) * 0.5;
    // blur/brightness state: first 3s the bg is fully SHARP, then blurs to
    // full blur level over 1.5s (arrives blurred right as the mesh lands).
    let blurk = smooth((((st - drop_t) + 0.35) / 1.5).clamp(0.0, 1.0)); // blur starts AT drop, lands 0.35s late-shifted
    // brightness: sharp phase is brighter, eases down to 0.25 as blur arrives
    let dim = 0.42 - 0.17 * blurk;
    // wobble: a touch stronger than before, grows further once blurred
    let wamp = 0.014 + 0.012 * blurk;
    // intro slide: bg is split DIAGONALLY into 2 pieces; during the fade-in
    // (0..1.5s) the left piece slides in from off-screen left and the right
    // piece from off-screen right; they meet at center as the fade completes.
    let slide = 1.0 - smooth((st / 1.5).clamp(0.0, 1.0));
    let off = slide * (W as f32 * 0.75); // fully off-screen at start
    // diagonal seam through the bg rect center (slope k in screen space)
    let k = 0.45f32;
    let bg_punch = if pumped <= 3 && st >= 1.5 { punch } else { 0.0 }; // pumps 1-3 incl. the shared drop beat
    for y in 0..H {
        for x in 0..W {
            // which piece? left of the diagonal seam = left piece
            let dx0 = x as f32 - (ox + dw * 0.5);
            let dy0 = y as f32 - (oy + dh * 0.5);
            let left = dx0 < k * dy0;
            // left piece content shifted left by `off` (comes from left),
            // right piece shifted right by `off` (comes from right)
            let sx = x as f32 - ox + if left { off } else { -off };
            let sy = y as f32 - oy;
            if sx < 0.0 || sy < 0.0 || sx >= dw || sy >= dh {
                img.put_pixel(x as u32, y as u32, Rgb([0, 0, 0]));
                continue;
            }
            // snare punch: bg zooms in a touch around its center, springs back
            let bz = 1.0 + bg_punch * 1.8; // strong zoom for the shared drop beat
            let bx = sx - dw * 0.5;
            let by = sy - dh * 0.5;
            let mut u = (bx / bz + dw * 0.5) / dw + wamp * (gt * 0.3 + y as f32 * 0.02).sin();
            let mut v = (by / bz + dh * 0.5) / dh + wamp * (gt * 0.24 + x as f32 * 0.019).cos();
            u = u.clamp(0.0, 1.0);
            v = v.clamp(0.0, 1.0);
            // blend sharp <-> blurred sample with the ramp
            let cs = ts.sample_clamp(u, v);
            let cb = tb.sample_clamp(u, v);
            let c = [cs[0] + (cb[0] - cs[0]) * blurk,
                     cs[1] + (cb[1] - cs[1]) * blurk,
                     cs[2] + (cb[2] - cs[2]) * blurk];
            // brightness flash rides the punch envelope -> the shared snare READS even on blurred bg
            let flash = (1.0 + bg_punch.abs() * 3.0).min(1.6);
            img.put_pixel(x as u32, y as u32,
                Rgb([(c[0] * dim * flash).min(255.0) as u8, (c[1] * dim * flash).min(255.0) as u8, (c[2] * dim * flash).min(255.0) as u8]));
        }
    }
    // pyramid drops in at st = DROP_T with spring overshoot, otherwise not drawn
    if st < drop_t {
        return img;
    }
    let (cx0, cy0) = (W as f32 * 0.5, H as f32 * 0.52);
    let u = st - drop_t;
    // damped spring from above-screen to center: starts at top with zero velocity,
    // falls with weight, overshoots past center, springs back. e^{-4u} decay, 9 rad/s.
    let start_off = -(cy0 + H as f32 * 0.65); // displacement at drop start (above screen)
    let cy = cy0 + start_off * (-4.0 * u).exp() * (9.0 * u).cos();
    let yaw = gt * 0.9;
    let pitch = 0.45 + 0.2 * (gt * 0.5).sin();
    let scale = H as f32 * (0.55 + 0.06 * (gt * 0.8).sin()) * (1.0 + punch); // snare punch: scale pop, spring back
    // transparency: solid until the mesh has synced with 4 snares since the
    // drop, then a quick 0.5s fade to fully transparent (bg shines through).
    let since: f32 = st - drop_t;
    let n_sync = ls.iter().skip(3).filter(|&&b| b <= st).count(); // snares landed after drop (drop itself = #3)
    // fades to a 50% floor (test build): bg shines through but mesh stays visible
    let mesh_alpha: f32 = if n_sync < 4 { 1.0 }
                          else { (0.5 + 0.5 * (1.0 - (since - (ls[6] - drop_t)) / 0.5).clamp(0.0, 1.0)) };
    let mut v = [(0.0f32, 0.0f32, 0.0f32); 4];
    v[0] = (0.0, 1.0, 0.0); // apex
    for k in 0..3 {
        let a = k as f32 * 2.0944;
        v[k + 1] = (a.cos(), -0.9, a.sin());
    }
    let (cp, sp) = (pitch.cos(), pitch.sin());
    let (cyw, syw) = (yaw.cos(), yaw.sin());
    let mut rv = [(0.0f32, 0.0f32, 0.0f32); 4];
    for k in 0..4 {
        let (x, y, z) = v[k];
        let y2 = y * cp - z * sp;
        let z2 = y * sp + z * cp;
        let x3 = x * cyw + z2 * syw;
        let z3 = -x * syw + z2 * cyw;
        rv[k] = (x3, y2, z3);
    }
    let persp = 2.4;
    let proj = |p: (f32, f32, f32)| (cx0 + p.0 * scale / (persp - p.2), cy - p.1 * scale / (persp - p.2));
    // GLOBAL light vector (fixed): direction of light travel, from a sun
    // positioned above the camera and to the left, aimed into the scene.
    // 45 deg downward: |y| == horizontal magnitude. Constant for all faces/frames.
    let l = {
        let (x, y, z) = (0.5f32, -0.5f32, 0.75f32); // rightward, downward, into scene
        let il = 1.0 / (x * x + y * y + z * z).sqrt();
        (x * il, y * il, z * il)
    };
    const PUNCH_MODE: MeshPunch = MeshPunch::SolidColor;
    let faces = [(0usize, 1usize, 2usize), (0, 2, 3), (0, 3, 1), (1, 3, 2)];
    let base_col = [(0.15f32, 0.55f32, 1.0f32), (0.2, 0.85, 1.0), (0.1, 0.45, 0.95), (0.35, 0.95, 1.0)];
    let mut depth = vec![f32::INFINITY; W * H];
    for &(i0, i1, i2) in &faces {
        let (ax, ay, az) = rv[i0]; let (bx, by, bz) = rv[i1]; let (cxx, cyy, czz) = rv[i2];
        let e1 = (bx - ax, by - ay, bz - az);
        let e2 = (cxx - ax, cyy - ay, czz - az);
        let mut n = (e1.1 * e2.2 - e1.2 * e2.1, e1.2 * e2.0 - e1.0 * e2.2, e1.0 * e2.1 - e1.1 * e2.0);
        let nl = (n.0 * n.0 + n.1 * n.1 + n.2 * n.2).sqrt();
        if nl < 1e-6 { continue; }
        n = (n.0 / nl, n.1 / nl, n.2 / nl);
        let ctr = ((ax + bx + cxx) / 3.0, (ay + by + cyy) / 3.0, (az + bz + czz) / 3.0);
        let view = (0.0 - ctr.0, 0.0 - ctr.1, persp - ctr.2);
        let vl = (view.0 * view.0 + view.1 * view.1 + view.2 * view.2).sqrt();
        let view = (view.0 / vl, view.1 / vl, view.2 / vl);
        let mut ndl = n.0 * l.0 + n.1 * l.1 + n.2 * l.2;
        if n.0 * view.0 + n.1 * view.1 + n.2 * view.2 < 0.0 { ndl = -ndl; }
        let lam = 0.25 + 0.75 * ndl.max(0.0);
        // punch tint only in SolidColor mode; Glass stays pure bg-through-refraction
        let punch_tint = match PUNCH_MODE { MeshPunch::SolidColor => 1.0 + punch.abs() * 2.0, MeshPunch::Glass => 1.0 };
        let (br, bgc, bb) = base_col[(i0 + i1 + i2) as usize % 4];
        let base = (br * punch_tint, bgc * punch_tint, bb * punch_tint);
        // refraction offset: faces angled away from the camera shift bg more.
        // normal.xy drives the direction; strength scales the pixel shift.
        let facing = (n.0 * view.0 + n.1 * view.1 + n.2 * view.2).abs();
        let shift = 18.0 * (1.0 - facing);
        let off = (n.0 * shift, n.1 * shift);
        // phase-driven: flat Solid while opaque; Refract+green glow only once transparent
        let surface = if mesh_alpha < 1.0 {
            Surface::Refract { strength: shift, chroma: 0.15 }
        } else {
            Surface::Solid { base }
        };
        let p = [proj(rv[i0]), proj(rv[i1]), proj(rv[i2])];
        let mode = if mesh_alpha < 1.0 { Blend::Add } else { Blend::Alpha };
        fill_tri_flat(&mut img, &mut depth, p, [rv[i0].2, rv[i1].2, rv[i2].2],
                      surface, lam, off, 0.15, mesh_alpha, mode);
    }
    img
}

// ---------------- Scene dispatch ----------------

fn frame_for(scene: Scene, texs: &[&Tex; 3], blurs: &[&Tex; 3],
             t0: usize, t1: usize, st: f32, gt: f32, mix: f32, punch: f32, beat: &BeatSync)
    -> ImageBuffer<Rgb<u8>, Vec<u8>>
{
    match scene {
        Scene::Rotozoom => frame_rotozoom(texs[t0], blurs[t0], texs[t1], blurs[t1], gt, mix, punch),
        Scene::TriangleDance => frame_tri(texs[2], blurs[2], st, gt, punch, beat), // blue bg, pyramid w/ drop
    }
}

// ---------------- Dev / Demo modes ----------------
// TIMELINE defines each scene's demo-mode start time and duration. Dev mode
// renders ONLY one scene's frames; the music offset for its video is
// (demo_start % song_len) so the audio lands exactly as it does in demo mode
// (song loops via -stream_loop in ffmpeg if the scene outlives the track).
const SONG_LEN: f32 = 27.8395; // ffprobe duration of meltdown_beat.ogg // meltdown_beat.ogg duration, for dev-mode audio math

fn parse_scene(s: &str) -> Option<Scene> {
    match s {
        "rotozoom" => Some(Scene::Rotozoom),
        "triangledance" | "triangles" | "tri" => Some(Scene::TriangleDance),
        _ => None,
    }
}

// Render one timeline entry: `start` = demo-timeline seconds where this scene
// sits, `dur` = its length. Flags control the handoff fades:
//   demo_fade_in : global black fade-in (only when this entry opens the demo)
//   fade_out     : fade to black at end (handoff to the next scene)
//   wrap         : dissolve back into the first scene at the very end (loop)
fn render_range(scene: Scene, start: f32, dur: f32,
                texs: &[&Tex; 3], blurs: &[&Tex; 3], beat: &BeatSync,
                demo_fade_in: bool, fade_out: bool, wrap: bool, idx0: usize)
    -> usize
{
    let scene_segs = match scene { Scene::Rotozoom => [0usize, 1], Scene::TriangleDance => [2, 1] };
    let show = (SEG_SECS * FPS as f32) as usize;
    let d = (FADE_SECS * FPS as f32) as usize;
    let total = (dur * FPS as f32) as usize;
    let mut idx = idx0;
    let mut f = 0usize;
    while f < total {
        let sj = if show > 0 { (f / show).min(1) } else { 0 };
        let fl = f % show;
        let st = (sj * show + fl) as f32 / FPS as f32;   // scene-local time
        let gt = start + f as f32 / FPS as f32;          // demo-timeline time == song position
        let tex_mix = if sj == 0 && fl >= show - d {
            smooth((fl - (show - d)) as f32 / d as f32)
        } else { 0.0 };
        let t0 = scene_segs[sj];
        let t1 = scene_segs[1 - sj];
        let punch = beat.punch(gt);
        let mut frame = frame_for(scene, texs, blurs, t0, t1, st, gt, tex_mix, punch, beat);

        // global demo fade-in from black (entry that opens the demo)
        if demo_fade_in && gt < FADE_SECS {
            fade(&mut frame, smooth(gt / FADE_SECS));
        }
        match scene {
            Scene::Rotozoom => {
                if fade_out && f >= total - d {
                    let k = smooth((f - (total - d)) as f32 / d as f32);
                    fade(&mut frame, 1.0 - k);
                }
            }
            Scene::TriangleDance => {
                // scene always starts black (after handoff in demo, clean in dev):
                // fade in over first FADE_SECS, identical ramp to the demo fade-in
                if st < FADE_SECS {
                    fade(&mut frame, smooth(st / FADE_SECS));
                }
                if wrap && f >= total - d {
                    let k = smooth((f - (total - d)) as f32 / d as f32);
                    let nf = frame_for(Scene::Rotozoom, texs, blurs, 0, 1, st, gt, 0.0, beat.punch(gt), beat);
                    mix_frames(&mut frame, &nf, k);
                }
            }
        }
        frame.save(format!("/root/plasma_warp/frames/f{:05}.png", idx)).unwrap();
        idx += 1;
        f += 1;
    }
    idx
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let a = Tex::load("/root/plasma_warp/tex_purple.png");
    let b = Tex::load("/root/plasma_warp/tex_green.png");
    let c = Tex::load("/root/plasma_warp/tex_blue.png");
    let d = Tex::load("/root/plasma_warp/tex_scene3.png"); // reserved: next scene (not used yet)
    let e = Tex::load("/root/plasma_warp/tex_scene4.png"); // reserved: RGBA overlay sprite w/ transparent px (not used yet)
    let texs = [&a, &b, &c];
    let blur_a = a.blur();
    let blur_b = b.blur();
    let blur_c = c.blur();
    let blurs = [&blur_a, &blur_b, &blur_c];
    let beat = BeatSync::load("/root/plasma_warp/beats.txt", 0.22, SONG_LEN);

    // TIMELINE: (scene, demo_start_sec, duration_sec) -- the sync contract.
    let timeline: Vec<(Scene, f32, f32)> = vec![
        (Scene::Rotozoom, 0.0, 16.0),
        (Scene::TriangleDance, 16.0, 16.0),
    ];

    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("demo");
    if mode == "dev" {
        // dev mode: single scene, exits when done. Music starts at start % SONG_LEN.
        let name = args.get(2).map(|s| s.as_str()).unwrap_or("");
        let sc = parse_scene(name).unwrap_or_else(|| panic!("usage: app dev <rotozoom|triangledance>"));
        let (scene, start, dur) = *timeline.iter().find(|(s, _, _)| *s == sc)
            .unwrap_or_else(|| panic!("scene not in timeline"));
        eprintln!("DEV MODE: {:?} | demo-timeline start {:.3}s, dur {:.1}s", scene, start, dur);
        let n = render_range(scene, start, dur, &texs, &blurs, &beat, start == 0.0, false, false, 0);
        eprintln!("ALL FRAMES DONE ({})", n);
        eprintln!("AUDIO_OFFSET={:.3}", (start % SONG_LEN) + 0.0);
    } else {
        // demo mode: full timeline in order, with handoffs and loop wrap
        let mut idx = 0usize;
        for (i, &(scene, start, dur)) in timeline.iter().enumerate() {
            let last = i == timeline.len() - 1;
            eprintln!("DEMO: scene {:?} @ {:.1}s", scene, start);
            idx = render_range(scene, start, dur, &texs, &blurs, &beat,
                               start == 0.0, !last, last, idx);
        }
        eprintln!("ALL FRAMES DONE ({})", idx);
    }
}

fn fade(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, k: f32) {
    for p in img.pixels_mut() {
        *p = Rgb([(p[0] as f32 * k) as u8, (p[1] as f32 * k) as u8, (p[2] as f32 * k) as u8]);
    }
}

fn mix_frames(a: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, b: &ImageBuffer<Rgb<u8>, Vec<u8>>, k: f32) {
    for (pa, pb) in a.pixels_mut().zip(b.pixels()) {
        *pa = Rgb([
            (pa[0] as f32 + (pb[0] as f32 - pa[0] as f32) * k) as u8,
            (pa[1] as f32 + (pb[1] as f32 - pa[1] as f32) * k) as u8,
            (pa[2] as f32 + (pb[2] as f32 - pa[2] as f32) * k) as u8,
        ]);
    }
}
