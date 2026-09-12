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
    CyberPuzzle, // skeleton: effect not implemented yet
}

// Which hand-labelled channel drives a scene's punch. Enum (project
// convention) so channels can be A/B-swapped for testing without code churn.
#[derive(Clone, Copy, PartialEq)]
enum SyncCh { Snare, Kick }

// Scene 1 punch source. SNARE is the default: the kick impulse is too short
// and staccato for the rotozoom (auditioned 2026-09-11 -- "kick looks ass in
// there"). Kick stays wired as an option purely for A/B tests via the env var
// PLASMA_ROTOZOOM_SYNC=snare|kick -- see rotozoom_channel().
const ROTOZOOM_SYNC: SyncCh = SyncCh::Snare;

fn rotozoom_channel() -> SyncCh {
    match std::env::var("PLASMA_ROTOZOOM_SYNC").ok().as_deref() {
        Some("snare") => SyncCh::Snare,
        Some("kick") => SyncCh::Kick,
        _ => ROTOZOOM_SYNC,
    }
}

// ---------------- Beat sync system (hand-labelled ground truth) ----------------
// Both channels are HAND-LABELLED in Audacity (label tracks, exported as plain
// text) and committed. No onset detection exists anywhere in this project --
// do not reintroduce it. If the sync is wrong, fix the labels, not the code.
//   beats.txt : 24 SNARE onsets   (meltdown_beat.ogg)
//   kicks.txt : 36 KICK onsets    (meltdown_beat.ogg)
// Grid the labels trace: 4/4 at ~103.4 BPM (bar = 2.321 s, beat = 0.580 s).
// Snare sits on beats 2 & 4; kick sits on beats 1 & 3 plus the "and" of 3, i.e.
// three kicks per bar. Labels keep the performance's natural micro-timing
// (spacing jitters +/- 15 ms); that jitter is authentic and is NOT quantised.
// punch(t)/punch_kick(t) return damped-sine impulses around the most recent
// labelled hit of their channel, wrapping cyclically at the song length.
// DESIGN NOTE (locked 2026-09-10, by user decree): keep the snare impulse's
// springy envelope -- its secondary lobe adds a second visual pulse between
// snares and tests as maxed vibes. DO NOT "fix" it to be snare-only.
struct BeatSync {
    beats: Vec<f32>,   // extracted snare timestamps (seconds) -- high band
    kicks: Vec<f32>,   // extracted kick timestamps (seconds) -- low band (150Hz)
    span: f32,         // TRUE audio file length -- must match the actual loop point
    amp: f32,          // punch strength
}
impl BeatSync {
    fn load(beats_path: &str, kicks_path: &str, amp: f32, song_len: f32) -> Self {
        let read = |p: &str| -> Vec<f32> {
            // one timestamp per line; '#' comment lines are skipped
            std::fs::read_to_string(p)
                .unwrap_or_else(|e| panic!("cannot read {p}: {e}"))
                .lines()
                .filter_map(|l| l.trim().parse().ok())
                .collect()
        };
        let beats = read(beats_path);
        let kicks = read(kicks_path);
        assert!(!beats.is_empty(), "beats.txt is empty");
        assert!(!kicks.is_empty(), "kicks.txt is empty");
        assert!(beats.windows(2).all(|w| w[1] > w[0]), "beats.txt must be sorted");
        assert!(kicks.windows(2).all(|w| w[1] > w[0]), "kicks.txt must be sorted");
        BeatSync { span: song_len, beats, kicks, amp }
    }
    /// time since the last hit of the given channel, cyclic over the song span
    #[inline]
    fn since(&self, t: f32, snare: bool) -> f32 {
        let tc = t % self.span;
        let v = if snare { &self.beats } else { &self.kicks };
        let mut best: Option<f32> = None;
        for &b in v { if b <= tc { best = Some(b); } else { break; } }
        match best {
            Some(b) => tc - b,
            None => match v.last() {
                Some(&b) => tc + self.span - b, // wrap-around gap
                None => 1e9,                    // empty channel: never fires
            },
        }
    }
    #[inline]
    fn punch(&self, t: f32) -> f32 {
        // SNARE impulse: sharp, bright spring. Decays before the next kick (~0.4s).
        // debug/A-B knob: PLASMA_NO_SNARE=1 silences this channel so a render
        // can be diffed against the normal one to ISOLATE the snare's effect.
        if std::env::var_os("PLASMA_NO_SNARE").is_some() { return 0.0; }
        let e = self.since(t, true);
        (-6.0 * e).exp() * (16.0 * e).sin() * self.amp
    }
    /// KICK impulse: subtle one-bounce thump that settles to a DEAD STILL by
    /// KICK_TAU = 0.40s, so the bg is motionless and ready for the next kick.
    ///   - decay KICK_DECAY=12.0 -> envelope is e^-4.8 = 0.008 of peak at 0.4s
    ///   - freq  KICK_FREQ=4*pi/0.4 -> the sine lands exactly on a zero crossing
    ///     at 0.4s, so there is no residual wobble, it simply stops.
    ///   - amp is scaled down (KICK_GAIN) to keep the move SUBTLE.
    #[inline]
    fn punch_kick(&self, t: f32) -> f32 {
        const KICK_TAU: f32 = 0.40;
        const KICK_DECAY: f32 = 12.0;          // e^-4.8 at KICK_TAU
        const KICK_FREQ: f32 = 31.4159;         // 4*pi/0.4: zero crossing at KICK_TAU
        const KICK_GAIN: f32 = 0.55;            // subtle
        // debug/A-B knob: PLASMA_NO_KICK=1 silences the channel so a render can
        // be diffed against the normal one to ISOLATE the kick's contribution.
        if std::env::var_os("PLASMA_NO_KICK").is_some() { return 0.0; }
        let e = self.since(t, false);
        if e >= KICK_TAU { return 0.0; }        // hard stop: dead still after 0.4s
        // ease the envelope to exactly zero at KICK_TAU so the hard stop is smooth
        let tail = 1.0 - (e / KICK_TAU).powi(3);
        (-KICK_DECAY * e).exp() * (KICK_FREQ * e).sin() * (self.amp * KICK_GAIN) * tail
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

// Ease-out exponential: maps elapsed fraction e in [0,1] -> assembly scaler s
// in [1,0]. s(0)=1 (at origin), s(1)=0 (at the cell), with a STEEP start and a
// shallow finish -- pieces launch fast and decelerate into place.
//   s = (e^{-k e} - e^{-k}) / (1 - e^{-k})
// Steepness k: larger = more front-loaded. Slope ratio start:end = e^k.
#[inline]
fn ease_out_expo(e: f32, k: f32) -> f32 {
    let ek = (-k).exp();
    (((-k * e).exp()) - ek) / (1.0 - ek)
}

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

// Draw one pyramid (the whole mesh, or an explosion shard). refract=false:
// opaque flat-shaded Solid faces. refract=true: transparent refracting glass
// with green emissive, additive blend.
#[allow(clippy::too_many_arguments)]
fn draw_pyramid(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, depth: &mut Vec<f32>,
                cx: f32, cy: f32, yaw: f32, pitch: f32, scale: f32,
                alpha: f32, punch: f32, refract: bool) {
    const PUNCH_MODE: MeshPunch = MeshPunch::SolidColor;
    let v0 = [(0.0f32, 1.0f32, 0.0f32),
              (1.0, -0.9, 0.0),
              (-0.5, -0.9, 0.8660),
              (-1.0, -0.9, -0.8660)];
    let (cp, sp) = (pitch.cos(), pitch.sin());
    let (cyw, syw) = (yaw.cos(), yaw.sin());
    let mut rv = [(0.0f32, 0.0f32, 0.0f32); 4];
    for k in 0..4 {
        let (x, y, z) = v0[k];
        let y2 = y * cp - z * sp;
        let z2 = y * sp + z * cp;
        let x3 = x * cyw + z2 * syw;
        let z3 = -x * syw + z2 * cyw;
        rv[k] = (x3, y2, z3);
    }
    let persp = 2.4f32;
    let scale = scale * (1.0 + punch);
    let proj = |p: (f32, f32, f32)| (cx + p.0 * scale / (persp - p.2), cy - p.1 * scale / (persp - p.2));
    let l = {
        let (x, y, z) = (0.5f32, -0.5f32, 0.75f32);
        let il = 1.0 / (x * x + y * y + z * z).sqrt();
        (x * il, y * il, z * il)
    };
    let faces = [(0usize, 1usize, 2usize), (0, 2, 3), (0, 3, 1), (1, 3, 2)];
    let base_col = [(0.15f32, 0.55f32, 1.0f32), (0.2, 0.85, 1.0), (0.1, 0.45, 0.95), (0.35, 0.95, 1.0)];
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
        let punch_tint = match PUNCH_MODE { MeshPunch::SolidColor => 1.0 + punch.abs() * 2.0, MeshPunch::Glass => 1.0 };
        let (br, bgc, bb) = base_col[(i0 + i1 + i2) as usize % 4];
        let base = (br * punch_tint, bgc * punch_tint, bb * punch_tint);
        let facing = (n.0 * view.0 + n.1 * view.1 + n.2 * view.2).abs();
        let shift = 18.0 * (1.0 - facing);
        let off = (n.0 * shift, n.1 * shift);
        let surface = if refract { Surface::Refract { strength: shift, chroma: 0.15 } }
                      else { Surface::Solid { base } };
        let p = [proj(rv[i0]), proj(rv[i1]), proj(rv[i2])];
        let mode = if refract { Blend::Add } else { Blend::Alpha };
        fill_tri_flat(img, depth, p, [rv[i0].2, rv[i1].2, rv[i2].2],
                      surface, lam, off, 0.15, alpha, mode);
    }
}

// ---------------- Scene: CyberPuzzle -- textured 3D quad pipeline ----------------
// Software 3D pipeline for TEXTURED quads: perspective camera, perspective-
// correct UV interpolation, depth buffer, flat per-face lambert lighting.
// Built in stages (wabunja's plan); STAGE 1 = the renderer itself, proven with
// a single rotating textured quad.

#[derive(Clone, Copy)]
struct V3 { x: f32, y: f32, z: f32 }
impl V3 {
    #[inline] fn new(x: f32, y: f32, z: f32) -> Self { V3 { x, y, z } }
    #[inline] fn sub(self, o: V3) -> V3 { V3::new(self.x - o.x, self.y - o.y, self.z - o.z) }
    #[inline] fn cross(self, o: V3) -> V3 {
        V3::new(self.y * o.z - self.z * o.y,
                self.z * o.x - self.x * o.z,
                self.x * o.y - self.y * o.x)
    }
    #[inline] fn dot(self, o: V3) -> f32 { self.x * o.x + self.y * o.y + self.z * o.z }
    #[inline] fn norm(self) -> V3 {
        let l = (self.x * self.x + self.y * self.y + self.z * self.z).sqrt();
        V3::new(self.x / l, self.y / l, self.z / l)
    }
}

// Rotate a point by yaw (about Y), then pitch (about X), then roll (about Z).
#[inline]
fn rot3(p: V3, yaw: f32, pitch: f32, roll: f32) -> V3 {
    let (cr, sr) = (roll.cos(), roll.sin());
    let (x1, y1) = (p.x * cr - p.y * sr, p.x * sr + p.y * cr);
    let (cp, sp) = (pitch.cos(), pitch.sin());
    let (y2, z2) = (y1 * cp - p.z * sp, y1 * sp + p.z * cp);
    let (cyw, syw) = (yaw.cos(), yaw.sin());
    let (x3, z3) = (x1 * cyw + z2 * syw, -x1 * syw + z2 * cyw);
    V3::new(x3, y2, z3)
}

// Rotate point p about unit axis a by angle (Rodrigues). Used for the
// per-piece tumble: a clean N-full-turn spin that returns to identity.
#[inline]
fn rot_axis(p: V3, a: V3, ang: f32) -> V3 {
    let (c, sn) = (ang.cos(), ang.sin());
    let term1 = V3::new(p.x * c, p.y * c, p.z * c);
    let term2 = a.cross(p);
    let term2 = V3::new(term2.x * sn, term2.y * sn, term2.z * sn);
    let ad = a.dot(p);
    let term3 = V3::new(a.x * ad * (1.0 - c), a.y * ad * (1.0 - c), a.z * ad * (1.0 - c));
    V3::new(term1.x + term2.x + term3.x,
            term1.y + term2.y + term3.y,
            term1.z + term2.z + term3.z)
}

// Stable per-cell pseudo-random in [0,1) from the cell index (no state).
#[inline]
fn hash01(i: usize, j: usize, salt: f32) -> f32 {
    let v = ((i as f32 * 12.9898 + j as f32 * 78.233 + salt).sin() * 43758.5453).fract();
    v.abs()
}

// Camera: eye at world origin looking down +Z. CAM_F is the focal length in
// PIXELS. Setting CAM_F == CAM_D makes 1 world unit == 1 pixel at the quad
// plane (Z == CAM_D) -- this is what lets the assembled quad reproduce a 2D
// blit exactly, and it is the whole trick behind the puzzle anchor.
const CAM_D: f32 = 1000.0;
const CAM_F: f32 = CAM_D;
const CX_PX: f32 = (W as f32) * 0.5;
const CY_PX: f32 = (H as f32) * 0.5;
// Global light direction == the camera's front direction (both +Z). An
// assembled (screen-parallel) quad therefore faces the light head-on and
// lights to exactly 1.0 (see draw_textured_quad). Two-sided lambert.
const LIGHT: V3 = V3 { x: 0.0, y: 0.0, z: 1.0 };

#[inline]
fn project(p: V3) -> (f32, f32, f32) { // -> screen x, screen y, 1/z
    let iz = 1.0 / p.z;
    (CX_PX + CAM_F * p.x * iz, CY_PX - CAM_F * p.y * iz, iz)
}

// Perspective-correct textured triangle (software rasterizer).
//
// Why the math looks like this: the projection is a divide-by-z, which does NOT
// preserve barycentric ratios. The weights below (w0,w1,w2) are SCREEN-space
// barycentrics. To recover a correct surface attribute we must first convert
// them to true 3D barycentrics a_i = (w_i/z_i) / (sum_j w_j/z_j); that lone 1/z_i
// is the only difference between the two. So every affine attribute is blended
// as "sum w_i * attr_i / z_i" and then divided by the interpolated "sum w_i / z_i".
// That same reciprocal depth (1/z) is what interpolates linearly in screen space,
// which is why it drives BOTH the depth buffer and the texture sampler.
//
// lam = flat per-face lighting (computed per quad upstream). Depth test on
// view-space z (smaller = closer).
#[allow(clippy::too_many_arguments)]
fn raster_tex_tri(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, depth: &mut [f32],
                  sx: [f32; 3], sy: [f32; 3],
                  u: [f32; 3], v: [f32; 3], invz: [f32; 3],
                  tex: &Tex, lam: f32) {
    // Twice the SIGNED area of the triangle in screen space (cross product of
    // two edges). Sign encodes winding, so it works for either direction; we
    // just need it nonzero. area ~ 0 => triangle collapsed to a line/point
    // (a quad seen exactly edge-on, e.g. mid-flip): nothing to draw, bail out.
    let area = (sx[1] - sx[0]) * (sy[2] - sy[0]) - (sy[1] - sy[0]) * (sx[2] - sx[0]);
    if area.abs() < 1e-9 { return; }
    // Hoisted out of the pixel loops: the normalizer that turns raw per-pixel
    // edge-function AREAS (units of px^2) into dimensionless barycentric
    // FRACTIONS summing to 1. Signed divide cancels the winding sign too.
    let inva = 1.0 / area;

    // Bounding box of the triangle, clamped to the framebuffer. Only these
    // pixels can possibly be inside, so only these are scanned.
    let minx = sx.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as i32;
    let maxx = sx.iter().cloned().fold(f32::MIN, f32::max).ceil().min((W - 1) as f32) as i32;
    let miny = sy.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as i32;
    let maxy = sy.iter().cloned().fold(f32::MIN, f32::max).ceil().min((H - 1) as f32) as i32;
    for py in miny..=maxy {
        for px in minx..=maxx {
            // Sample at the pixel CENTER (+0.5), not its top-left corner.
            let fxp = px as f32 + 0.5;
            let fyp = py as f32 + 0.5;

            // Screen-space barycentric weights of this pixel. edge(i,j,pixel)
            // is twice the signed area of triangle (i,j,pixel); * inva turns it
            // into the fraction of the whole triangle, i.e. w for the opposite
            // vertex. w2 comes free from w0+w1+w2 == 1 (sum-to-one property).
            let w0 = ((sx[1] - fxp) * (sy[2] - fyp) - (sy[1] - fyp) * (sx[2] - fxp)) * inva;
            let w1 = ((sx[2] - fxp) * (sy[0] - fyp) - (sy[2] - fyp) * (sx[0] - fxp)) * inva;
            let w2 = 1.0 - w0 - w1;
            // Inside test: all three weights >= 0 <=> pixel is inside the tri.
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 { continue; }

            // Interpolate reciprocal depth 1/z (the screen-linear quantity;
            // z itself is not). This same value is reused as the perspective
            // denominator for the UVs below.
            let iz = w0 * invz[0] + w1 * invz[1] + w2 * invz[2];
            if iz <= 0.0 { continue; }          // through/behind the eye: skip
            let z = 1.0 / iz;                    // back to real view-space depth

            let idx = py as usize * W + px as usize;
            if z < depth[idx] {                  // z-buffer: smaller z = closer
                depth[idx] = z;
                // Perspective-correct UV. Interpolate u/z and v/z (linear in
                // screen space), then divide by interpolated 1/z (= iz) to undo
                // the foreshortening: u = (sum w_i*u_i/z_i) / (sum w_i/z_i).
                // At a vertex this returns exactly that vertex's u,v.
                let uu = ((w0 * u[0] * invz[0] + w1 * u[1] * invz[1] + w2 * u[2] * invz[2]) / iz).clamp(0.0, 1.0);
                let vv = ((w0 * v[0] * invz[0] + w1 * v[1] * invz[1] + w2 * v[2] * invz[2]) / iz).clamp(0.0, 1.0);
                let c = tex.sample_clamp(uu, vv);
                // Flat per-face lighting: scale the texel by lam.
                img.put_pixel(px as u32, py as u32, Rgb([
                    (c[0] * lam).clamp(0.0, 255.0) as u8,
                    (c[1] * lam).clamp(0.0, 255.0) as u8,
                    (c[2] * lam).clamp(0.0, 255.0) as u8,
                ]));
            }
        }
    }
}

// Letterbox fit: the largest uniform scale that still shows the WHOLE texture
// (black bars on the remaining sides).
fn fit_letterbox(tw: f32, th: f32) -> (f32, f32) {
    let s = (W as f32 / tw).min(H as f32 / th);
    (tw * s, th * s)
}

// Draw one textured quad (two triangles). Corners in world space, order
// [TL, TR, BR, BL]; uvs likewise. Flat lambert per face, two-sided (see LIGHT).
//
// TWO-SIDED TEXTURING: `back == None` -> BOTH faces sample `front` (the old
// behaviour, used during the fly-in). `back == Some(t)` -> the face pointing
// AT the camera samples `front`, the face pointing AWAY samples `t` with u
// mirrored (u -> 1-u) so the reveal reads un-mirrored. The side is chosen PER
// QUAD from its CURRENT facing, every frame -- there is no temporal "swap"; a
// piece flipping 180 degrees crosses over by itself at the edge-on instant.
fn draw_textured_quad(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, depth: &mut [f32],
                      front: &Tex, back: Option<&Tex>, c: [V3; 4], uvs: [(f32, f32); 4]) {
    draw_textured_quad_tint(img, depth, front, back, c, uvs, 1.0)
}

fn draw_textured_quad_tint(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, depth: &mut [f32],
                           front: &Tex, back: Option<&Tex>, c: [V3; 4], uvs: [(f32, f32); 4],
                           tint: f32) {
    let n = c[1].sub(c[0]).cross(c[3].sub(c[0])).norm();
    // assembled quad -> |n.LIGHT| == 1 -> lam == 1.0 exactly (two-sided: abs)
    let lam = (0.25 + 0.75 * n.dot(LIGHT).abs()) * tint;
    // camera is the world origin, so centroid -> camera = -centroid. `n` is the
    // outward front normal; n.cen < 0 means the front is turned toward the camera.
    let cen = V3::new(
        (c[0].x + c[1].x + c[2].x + c[3].x) * 0.25,
        (c[0].y + c[1].y + c[2].y + c[3].y) * 0.25,
        (c[0].z + c[1].z + c[2].z + c[3].z) * 0.25);
    let front_facing = n.dot(cen) < 0.0;
    let (pick, flip_u) = match back {
        Some(b) if !front_facing => (b, true),
        _ => (front, false),
    };
    // Un-mirror the revealed back: a 180-degree Y flip reverses the left/right
    // corner order, so swap the u of each tile's left/right corner pair
    // (TL<->TR, BL<->BR). This mirrors WITHIN the tile -- a global (1-u) would
    // sample the wrong slice on off-centre tiles.
    let uvs: [(f32, f32); 4] = if flip_u {
        [(uvs[1].0, uvs[0].1), (uvs[0].0, uvs[1].1),
         (uvs[3].0, uvs[2].1), (uvs[2].0, uvs[3].1)]
    } else { uvs };
    if std::env::var_os("CYBERPUZZLE_LIGHTDBG").is_some() {
        let raw = 0.25 + 0.75 * n.dot(LIGHT).abs();
        eprintln!("LIGHTDBG n=({:.3},{:.3},{:.3}) n.L={:.4} lam_raw={:.6} front={} flip_u={}",
                  n.x, n.y, n.z, n.dot(LIGHT), raw, front_facing, flip_u);
    }
    let mut sx = [0.0f32; 4];
    let mut sy = [0.0f32; 4];
    let mut invz = [0.0f32; 4];
    for i in 0..4 {
        let (a, b, iz) = project(c[i]);
        sx[i] = a; sy[i] = b; invz[i] = iz;
    }
    for &(a, b, cc) in &[(0usize, 1usize, 2usize), (0, 2, 3)] {
        raster_tex_tri(img, depth,
                       [sx[a], sx[b], sx[cc]], [sy[a], sy[b], sy[cc]],
                       [uvs[a].0, uvs[b].0, uvs[cc].0],
                       [uvs[a].1, uvs[b].1, uvs[cc].1],
                       [invz[a], invz[b], invz[cc]], pick, lam);
    }
}

// Grid resolution for the CyberPuzzle quad. Adjustable at runtime via env so
// Grid resolution for the CyberPuzzle quad. Adjustable at runtime via env so
// no single value is hard-coded (default 4x4); see cyberpuzzle_grid().
const CYBERPUZZLE_COLS: usize = 4;
const CYBERPUZZLE_ROWS: usize = 4;
// MAXIMUM full rotations a piece makes about its own (hashed) axis during the
// fly; the actual per-piece amount is hashed between MIN and this.
const CYBERPUZZLE_MAX_TURNS: f32 = 2.0;
const CYBERPUZZLE_MIN_TURNS: f32 = 0.4;
// Where pieces launch from: just outside the camera at the LOWER-LEFT corner
// (screen px, negative = off-screen left, >H = off-screen bottom), plus hashed
// jitter so they don't stack. The stream flows from here toward the upper right.
const CYBERPUZZLE_ORIGIN_X: f32 = -260.0;
const CYBERPUZZLE_ORIGIN_Y: f32 = (H as f32) + 200.0;
const CYBERPUZZLE_ORIGIN_JITTER: f32 = 240.0;
// Launch DEPTH: pieces start this far BEHIND the quad plane (further from the
// camera) and fly forward to it. In perspective, pushing the off-screen
// lower-left origin deeper drags its projection up toward the vanishing point,
// so the stream reads as dropping in from ABOVE instead of sliding along the
// plane. The depth spread also keeps overlapping pieces on separate z during
// flight, so the z-buffer resolves them instead of the surfaces fighting.
// Signed depth offset of the launch point, world units. POSITIVE = pieces start
// BEHIND the quad plane (they grow into place); NEGATIVE = start IN FRONT of it
// (they recede into place). Flip the sign to switch which side they fly from.
const CYBERPUZZLE_ORIGIN_Z: f32 = -450.0;       // NEGATIVE => launch IN FRONT of the plane
const CYBERPUZZLE_ORIGIN_Z_JITTER: f32 = 300.0; // hashed extra depth per piece
// Per-piece flight duration. The launch window (stagger) is NOT a const: it is
// DERIVED so that the last piece lands exactly on a snare (see the timing block
// in frame_cyberpuzzle). Everything downstream is snare-synced to the music.
const CYBERPUZZLE_FLY_DUR: f32 = 1.7;
// Front-loading of the flight curve: 0 = linear, larger = pieces fly faster off
// the origin and coast into place. Slope ratio (start:end) is e^k.
const CYBERPUZZLE_EASE_K: f32 = 3.5;
// Choreography anchor: the assembly ENDS on the first SNARE at/after this
// nominal scene time. From there, the next snare starts the x-jiggle, and the
// SECOND snare after that (one snare is spent jiggling) triggers the 180-deg
// Y flip that reveals the back texture. See frame_cyberpuzzle.
const CYBERPUZZLE_ASSEMBLY_TARGET: f32 = 3.0;
const CYBERPUZZLE_FLIP_DUR: f32 = 1.2;
// SHARED-VERTEX SHAPE DISPLACEMENT (wabunja's scheme):
// The grid is ONE watertight mesh of shared vertices -- vertex (gi,gj) is owned
// jointly by every piece touching it, so a boundary vertex moved for one piece
// is moved for its neighbours by definition (they read the same point).
// Each INTERIOR vertex carries a deterministic IN-PLANE offset (z untouched),
// RADIUS-bounded to a fraction of the cell size (< half a cell) so no triangle
// can invert or overlap and the planar map stays one-to-one. Image-edge
// vertices are pinned -- moving one would move the silhouette.
// Each piece's UVs are the PLANAR PROJECTION of the vertex rest position, so
// displacing vertices re-meshes the SAME plane without changing the picture:
// at s=0 (assembled) the deformed grid still reproduces the exact texture.
// RULE: offset radius < 0.5 * cell. We use AMP * cell (AMP clamped < 0.49).
const CYBERPUZZLE_SHAPE_AMP: f32 = 0.35; // fraction of min cell size; 0 = off

// HIHAT-DERIVED SWING. Measured from the hand-labelled sync data: snare-to-snare
// spacing fits 1.1611 s (== 2 beats @ 103.35 BPM), and snare-to-snare spans
// exactly 4 hihats, so the hihat interval is 1.1611/4. (Cross-checked against a
// 6-14 kHz onset detection on the audio: median spacing 0.285 s.) Hard-coded for
// now, per wabunja.
const HIHAT_DT: f32 = 0.2903;
// DANCE: every piece sways on screen-X with sin(), one full -1..1 excursion per
// hihat interval => period = 2*HIHAT_DT. Amplitude is a FRACTION of the square
// puzzle piece (cell) size, in world units (1 world unit == 1 px at the quad
// plane). The SAME offset is applied to every piece, so shared vertices stay
// welded and the whole mosaic swings as one rigid body -- no seams can open.
// The swing is OFF during the fly-in and BEGINS on the first KICK sync at/after
// assembly_end, phase-anchored on that snare.
const CYBERPUZZLE_DANCE_FRAC: f32 = 0.0625; // 1/16 of the square piece
const CYBERPUZZLE_DANCE_AMP: f32 = -1.0; // absolute px override; <0 => use FRAC*cell

fn cyberpuzzle_grid() -> (usize, usize) {
    let n = |k: &str, d: usize| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    (n("CYBERPUZZLE_COLS", CYBERPUZZLE_COLS).max(1),
     n("CYBERPUZZLE_ROWS", CYBERPUZZLE_ROWS).max(1))
}

fn cyberpuzzle_shape_amp() -> f32 {
    std::env::var("CYBERPUZZLE_SHAPE_AMP").ok().and_then(|v| v.parse().ok())
        .unwrap_or(CYBERPUZZLE_SHAPE_AMP).clamp(0.0, 0.49)
}

// Build the shared-vertex grid of a qw x qh quad: (cols+1)*(rows+1) vertices,
// row-major. Interior vertices get a hashed in-plane offset (radius <= amp*cell
// < half a cell). Returns (positions, uvs); each uv is the planar projection of
// the offset rest position -- this is what keeps the assembled image exact.
// Index: gj*(cols+1)+gi.
fn cyberpuzzle_vertices(cols: usize, rows: usize, qw: f32, qh: f32, amp: f32)
    -> (Vec<V3>, Vec<(f32, f32)>) {
    let cell = (qw / cols as f32).min(qh / rows as f32);
    let maxd = cell * amp;
    let nv = (cols + 1) * (rows + 1);
    let mut pos = Vec::with_capacity(nv);
    let mut uv = Vec::with_capacity(nv);
    for gj in 0..=rows {
        for gi in 0..=cols {
            let x = -qw * 0.5 + qw * (gi as f32) / (cols as f32);
            let y =  qh * 0.5 - qh * (gj as f32) / (rows as f32);
            let interior = gi > 0 && gi < cols && gj > 0 && gj < rows;
            let (dx, dy) = if interior && maxd > 0.0 {
                let ang = hash01(gi, gj, 21.0) * std::f32::consts::TAU;
                let rad = hash01(gi, gj, 22.0) * maxd;        // bounded RADIUS
                (rad * ang.cos(), rad * ang.sin())
            } else { (0.0, 0.0) };
            let (px, py) = (x + dx, y + dy);
            pos.push(V3::new(px, py, 0.0));
            // planar projection of the (displaced) rest position, NOT a frozen
            // grid uv -- so the plane re-meshes without warping the picture.
            uv.push(((px + qw * 0.5) / qw, (qh * 0.5 - py) / qh));
        }
    }
    (pos, uv)
}

fn frame_cyberpuzzle(tex: &Tex, back_tex: &Tex, st: f32, _gt: f32, beat: &BeatSync) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let mut img = ImageBuffer::new(W as u32, H as u32);
    let mut depth = vec![f32::INFINITY; W * H];
    let (cols, rows) = cyberpuzzle_grid();
    let (qw, qh) = fit_letterbox(tex.w as f32, tex.h as f32);
    let amp_base = cyberpuzzle_shape_amp();
    let fly_dur = CYBERPUZZLE_FLY_DUR;
    // ---- SNARE-SYNCED CHOREOGRAPHY ----------------------------------------
    // Scene-local snare schedule (same mapping the other scenes use).
    let mut ls: Vec<f32> = beat.beats.iter().map(|&b| (b - 32.0).rem_euclid(beat.span)).collect();
    ls.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let snare_at_or_after = |t: f32| ls.iter().cloned().find(|&b| b >= t - 1e-4);
    let snare_after = |t: f32| ls.iter().cloned().find(|&b| b > t + 1e-4);
    // (1) The picture unifies EXACTLY on a snare: pick the first snare at/after
    // the nominal target, then draw the launch window so the last piece lands
    // precisely there (last launch == stagger, +fly_dur == assembly_end).
    let assembly_end = snare_at_or_after(CYBERPUZZLE_ASSEMBLY_TARGET)
        .unwrap_or(CYBERPUZZLE_ASSEMBLY_TARGET);
    let stagger = (assembly_end - fly_dur).max(0.2);
    // (2) The NEXT snare after unify begins the x-axis jiggle.
    let jiggle_start = snare_after(assembly_end).unwrap_or(assembly_end + 0.5);
    // (3) Flip on the SECOND snare after the jiggle starts -- one snare is spent
    // jiggling in between, then the next one triggers the 180-deg reveal.
    let after_jiggle: Vec<f32> = ls.iter().cloned().filter(|&b| b > jiggle_start + 1e-4).collect();
    let flip_start = after_jiggle.get(1).copied()
        .or_else(|| after_jiggle.first().copied())
        .unwrap_or(jiggle_start + 1.0);
    if std::env::var_os("CYBERPUZZLE_TIMING").is_some() {
        eprintln!("[cyber] assembly_end={:.4} jiggle_start={:.4} flip_start={:.4} flip_end={:.4} stagger={:.4}",
                  assembly_end, jiggle_start, flip_start, flip_start + CYBERPUZZLE_FLIP_DUR, stagger);
    }
    // Morph the irregular pieces back to perfect squares across the hold. This is
    // INVISIBLE: the assembled planar image is invariant to in-plane interior
    // -vertex displacement, and by assembly_end every piece has landed (s==0).
    // Why bother: after morphing, each piece is a square centred on its cell, so
    // the 180-deg Y flip maps it onto itself -> the reveal retiles with NO
    // diagonal cracks (irregular shapes cannot flip onto themselves).
    let morph = if st <= assembly_end { 0.0 }
                else { smooth(((st - assembly_end) / (flip_start - assembly_end)).clamp(0.0, 1.0)) };
    let amp = amp_base * (1.0 - morph);
    // --- hihat swing: rigid screen-X sway of the WHOLE mosaic (same offset for
    // every piece -> shared vertices stay welded, no seams). OFF during the
    // fly-in; it BEGINS exactly on the first KICK sync at/after assembly_end and
    // runs from there. Phase is anchored on that kick (sin 0 there) so the sway
    // starts from a standstill with no jump. Amplitude is a fraction of the
    // square piece (cell) unless an absolute override is set.
    let cell = (qw / cols as f32).min(qh / rows as f32);
    let dance_amp: f32 = std::env::var("CYBERPUZZLE_DANCE_AMP").ok()
        .and_then(|v| v.parse().ok()).filter(|&v| v >= 0.0)
        .unwrap_or(CYBERPUZZLE_DANCE_FRAC * cell);
    let dance_x = if st < jiggle_start { 0.0 }
        else { dance_amp * (std::f32::consts::PI * (st - jiggle_start) / HIHAT_DT).sin() };
    // ONE shared mesh; pieces are index windows into it, so shared boundary
    // vertices (and their uvs) are literally the same points -> watertight.
    let (vpos, vuv) = cyberpuzzle_vertices(cols, rows, qw, qh, amp);
    let vidx = |gi: usize, gj: usize| gj * (cols + 1) + gi;
    let center = V3::new(0.0, 0.0, CAM_D);
    let force_s: Option<f32> = std::env::var("CYBERPUZZLE_FORCE_S").ok().and_then(|v| v.parse().ok());
    let origin_z: f32 = std::env::var("CYBERPUZZLE_ORIGIN_Z").ok().and_then(|v| v.parse().ok()).unwrap_or(CYBERPUZZLE_ORIGIN_Z);
    let tint_on = std::env::var_os("CYBERPUZZLE_TINT").is_some();
    let spin_rate: f32 = std::env::var("CYBERPUZZLE_SPIN").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let spin = spin_rate * (st - (stagger + fly_dur)).max(0.0);
    // --- post-assembly choreography: the 180-degree Y flip. It pivots on the
    // SAME per-piece centroid as the tumble and lands coplanar again (a half-
    // turn about Y keeps z and mirrors x). `reveal` enables two-sided texturing
    // the instant the flip starts, so the back never peeks during the fly-in.
    let flip_t = ((st - flip_start) / CYBERPUZZLE_FLIP_DUR).clamp(0.0, 1.0);
    let yaw = std::f32::consts::PI * smooth(flip_t);
    let reveal = st >= flip_start;
    for j in 0..rows {
        for i in 0..cols {
            // the piece's four SHARED corners, in [TL, TR, BR, BL] order
            let ci = [vidx(i, j), vidx(i + 1, j), vidx(i + 1, j + 1), vidx(i, j + 1)];
            let base = [vpos[ci[0]], vpos[ci[1]], vpos[ci[2]], vpos[ci[3]]];
            let uvs = [vuv[ci[0]], vuv[ci[1]], vuv[ci[2]], vuv[ci[3]]];
            // per-piece launch time: stream order runs from the lower-left cell
            // (first to launch) to the upper-right cell (last to launch), so the
            // pieces cascade from the corner toward the upper right.
            let kx = if cols > 1 { i as f32 / (cols - 1) as f32 } else { 0.0 };
            let ky = if rows > 1 { (rows - 1 - j) as f32 / (rows - 1) as f32 } else { 0.0 };
            let key = (kx + ky) * 0.5;                 // 0 lower-left .. 1 upper-right
            let launch = key * stagger;
            let e = ((st - launch) / fly_dur).clamp(0.0, 1.0); // elapsed fraction
            // ONE unified scaler drives BOTH position and spin, so a piece
            // stops rotating the instant it reaches its cell (s==0). `turns`
            // still independently sets HOW MANY rotations happen during flight.
            let mut s = ease_out_expo(e, CYBERPUZZLE_EASE_K);  // 1 -> 0
            if let Some(f) = force_s { s = f; }
            // pivot = centroid of the (displaced) piece
            let cc = V3::new(
                (base[0].x + base[1].x + base[2].x + base[3].x) * 0.25,
                (base[0].y + base[1].y + base[2].y + base[3].y) * 0.25,
                0.0);
            // shared fly-in origin: just off the lower-left corner, jittered per
            // piece. Offset = (origin - cell); scaled by s it moves the piece
            // from the origin to its assembled cell.
            let ox_px = CYBERPUZZLE_ORIGIN_X + (hash01(i, j, 7.0) - 0.5) * CYBERPUZZLE_ORIGIN_JITTER;
            let oy_px = CYBERPUZZLE_ORIGIN_Y + (hash01(i, j, 8.0) - 0.5) * CYBERPUZZLE_ORIGIN_JITTER;
            // screen px -> world at the quad plane (1 unit == 1 px at z=CAM_D)
            let ox_w = (ox_px - CX_PX) * CAM_D / CAM_F;
            let oy_w = (CY_PX - oy_px) * CAM_D / CAM_F;
            // launch depth: CYBERPUZZLE_ORIGIN_Z carries the SIGN, so a negative
            // value pulls the whole launch IN FRONT of the quad plane. Clamp the
            // resulting launch z so a piece never reaches or crosses the eye
            // plane (z<=0), where the projection blows up and the piece would
            // vanish / invert. 1 world unit == 1 px at z==CAM_D.
            let dz_want = origin_z + hash01(i, j, 9.0) * CYBERPUZZLE_ORIGIN_Z_JITTER;
            let launch_z = (CAM_D + dz_want).clamp(140.0, 1.0e6);
            let dz = launch_z - CAM_D;
            let dx = ox_w - cc.x;
            let dy = oy_w - cc.y;
            // per-piece tumble about a hashed axis + hashed turn count (< MAX),
            // so any integer-ish amount still returns to identity at landing.
            let ax_raw = V3::new(hash01(i, j, 3.0) - 0.5,
                                 hash01(i, j, 4.0) - 0.5,
                                 hash01(i, j, 5.0) - 0.5);
            let axis = if ax_raw.dot(ax_raw) < 1e-6 { V3::new(1.0, 0.0, 0.0) } else { ax_raw.norm() };
            let tilt = (hash01(i, j, 6.0) - 0.5) * 2.0;          // radians
            let turns = CYBERPUZZLE_MIN_TURNS
                + (CYBERPUZZLE_MAX_TURNS - CYBERPUZZLE_MIN_TURNS) * hash01(i, j, 10.0);
            let ang = (turns * std::f32::consts::TAU + tilt) * s;
            let mut corners = [V3::new(0.0, 0.0, 0.0); 4];
            for k in 0..4 {
                let rel = base[k].sub(cc);                       // about piece centroid
                let r = rot_axis(rel, axis, ang);                // tumble
                // 180-degree Y flip about the piece centroid (identity until flip_start).
                let r = if yaw != 0.0 { rot3(r, yaw, 0.0, 0.0) } else { r };
                let pos = V3::new(cc.x + r.x + dx * s,
                                  cc.y + r.y + dy * s,
                                  cc.z + r.z + dz * s);
                let mut pos = if spin != 0.0 { rot3(pos, spin, 0.0, 0.0) } else { pos };
                // hihat swing: same offset for every piece -> shared vertices move
                // together, so the mosaic sways as one body with no cracks.
                pos.x += dance_x;
                corners[k] = V3::new(center.x + pos.x, center.y + pos.y, center.z + pos.z);
            }
            // two-sided only once the flip begins; None keeps front tex on both faces.
            let back = if reveal { Some(back_tex) } else { None };
            // DEBUG: tint alternating cells so the grid is visible. TINT=1.
            if tint_on {
                let t = if (i + j) % 2 == 0 { 1.0 } else { 0.5 };
                draw_textured_quad_tint(&mut img, &mut depth, tex, back, corners, uvs, t);
            } else {
                draw_textured_quad(&mut img, &mut depth, tex, back, corners, uvs);
            }
        }
    }
    img
}

fn frame_tri(ts: &Tex, tb: &Tex, st: f32, gt: f32, punch: f32, kick: f32, beat: &BeatSync) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    // scene-local snare schedule: global beats mapped into scene time (mod song span)
    let start: f32 = 16.0; // scene 2 begins at demo t=16s
    let mut ls: Vec<f32> = beat.beats.iter().map(|&b| (b - start).rem_euclid(beat.span)).collect();
    ls.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pumped: usize = ls.iter().filter(|&&b| b <= st).count();
    let drop_t = ls[2]; // mesh drops ON the 3rd snare
    // explosion schedule: once transparent, the mesh syncs exactly 4 more
    // times; on the 4th it explodes into shards.
    let fade_done = (ls[6] - drop_t) + 0.5;
    let tsyncs: Vec<f32> = ls.iter().map(|&b| b - drop_t)
        .filter(|&b| b > fade_done).take(4).collect();
    let explode_t = tsyncs.get(3).copied().unwrap_or(9.0);
    let t_x = st - (drop_t + explode_t);
    const SHARD_T: f32 = 1.6;
    let t_wave = t_x - SHARD_T;
    let wave_amp = if t_wave > 0.0 { smooth((t_wave / 0.5).min(1.0)) } else { 0.0 };
    let fade_k = if t_wave > 0.0 { 1.0 - smooth((t_wave / 1.2).min(1.0)) } else { 1.0 };
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
    // bg sync channel: KICK throughout the scene. The intro (before the mesh
    // lands) keeps the snare-driven intro pumps; after the drop the bg rides
    // kicks only, so the mesh owns the snares.
    // FOCUS MODE: kick-only calibration. Snare syncs on bg temporarily OFF
    // (incl. the intro pumps) so we can dial the kick channel in cleanly.
    let bg_punch = if st >= 1.5 { kick } else { 0.0 }; // kick impulse already carries KICK_GAIN
    let _ = pumped;
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
            let bz = 1.0 + bg_punch * 1.0; // snap zoom, tuned down (was 1.8, too drastic)
            let bx = sx - dw * 0.5;
            let by = sy - dh * 0.5;
            let mut u = (bx / bz + dw * 0.5) / dw + wamp * (gt * 0.3 + y as f32 * 0.02).sin();
            let mut v = (by / bz + dh * 0.5) / dh + wamp * (gt * 0.24 + x as f32 * 0.019).cos();
            u += wave_amp * 0.07 * (sy * 0.085 + gt * 5.0).sin();
            v += wave_amp * 0.07 * (sx * 0.075 - gt * 4.2).cos();
            u = u.clamp(0.0, 1.0);
            v = v.clamp(0.0, 1.0);
            // blend sharp <-> blurred sample with the ramp
            let cs = ts.sample_clamp(u, v);
            let cb = tb.sample_clamp(u, v);
            let c = [cs[0] + (cb[0] - cs[0]) * blurk,
                     cs[1] + (cb[1] - cs[1]) * blurk,
                     cs[2] + (cb[2] - cs[2]) * blurk];
            // brightness flash rides the punch envelope -> the shared snare READS even on blurred bg
            let flash = (1.0 + bg_punch.abs() * 1.5).min(1.3);
            img.put_pixel(x as u32, y as u32,
                Rgb([(c[0] * dim * flash).min(255.0) as u8, (c[1] * dim * flash).min(255.0) as u8, (c[2] * dim * flash).min(255.0) as u8]));
        }
    }
    let (cx0, cy0) = (W as f32 * 0.5, H as f32 * 0.52);
    let mut depth = vec![f32::INFINITY; W * H];
    const SHARDS: usize = 9;
    // whole mesh: drop -> 4 opaque syncs -> transparent refract glass
    if st >= drop_t && t_x < 0.0 {
        let u = st - drop_t;
        let start_off = -(cy0 + H as f32 * 0.65);
        let cy = cy0 + start_off * (-4.0 * u).exp() * (9.0 * u).cos();
        let yaw = gt * 0.9;
        let pitch = 0.45 + 0.2 * (gt * 0.5).sin();
        let n_sync = ls.iter().skip(3).filter(|&&b| b <= st).count();
        let mesh_alpha = if n_sync < 4 { 1.0 }
                         else { 0.5 + 0.5 * (1.0 - (u - (ls[6] - drop_t)) / 0.5).clamp(0.0, 1.0) };
        let refract = mesh_alpha < 1.0;
        // mesh rides the SNARE channel (punch): scale pop + (while opaque) the
        // SolidColor tint flash. bg keeps the kick channel, so the two layers
        // now have independent rhythms again -- snare = mesh, kick = bg.
        draw_pyramid(&mut img, &mut depth, cx0, cy, yaw, pitch,
                     H as f32 * (0.55 + 0.06 * (gt * 0.8).sin()), mesh_alpha, punch, refract);
    } else if t_x >= 0.0 {
        // explosion: shards = small transparent refracting pyramids flying
        // out on parabolic (gravity) paths, down off the bottom of the screen
        for i in 0..SHARDS {
            let fi = i as f32;
            let fr1 = ((fi * 12.9898).sin() * 43758.5453).fract();
            let fr2 = ((fi * 78.233).sin() * 12543.123).fract();
            let vx = (fr1 - 0.5) * 460.0;
            let vy0 = -(80.0 + 260.0 * fr2);
            let g = 820.0;
            let x = cx0 + vx * t_x;
            let y = cy0 + vy0 * t_x + 0.5 * g * t_x * t_x;
            if y < H as f32 + 150.0 && x > -150.0 && x < W as f32 + 150.0 {
                let yaw = gt * 1.7 + fi * 1.3;
                let pitch = 0.45 + 0.3 * (gt * 0.7 + fi).sin();
                let s = H as f32 * 0.55 * (0.13 + 0.05 * fr2) * (1.0 + punch);
                draw_pyramid(&mut img, &mut depth, x, y, yaw, pitch, s, 0.5, punch, true);
            }
        }
    }
    if fade_k < 1.0 { fade(&mut img, fade_k); }
    img
}

// ---------------- Scene dispatch ----------------

fn frame_for(scene: Scene, texs: &[&Tex; 5], blurs: &[&Tex; 5],
             t0: usize, t1: usize, st: f32, gt: f32, mix: f32, punch: f32, beat: &BeatSync)
    -> ImageBuffer<Rgb<u8>, Vec<u8>>
{
    match scene {
        Scene::Rotozoom => {
            let p = match rotozoom_channel() {
                SyncCh::Snare => punch,                // snare impulse
                SyncCh::Kick => beat.punch_kick(gt),   // kick impulse
            };
            frame_rotozoom(texs[t0], blurs[t0], texs[t1], blurs[t1], gt, mix, p)
        }
        Scene::TriangleDance => frame_tri(texs[2], blurs[2], st, gt, punch, beat.punch_kick(gt), beat), // blue bg; mesh=snare, bg=kick
        Scene::CyberPuzzle => frame_cyberpuzzle(texs[3], texs[4], st, gt, beat), // front=tex_scene3, back=tex_scene4
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
        "cyberpuzzle" | "puzzle" => Some(Scene::CyberPuzzle),
        _ => None,
    }
}

// Render one timeline entry: `start` = demo-timeline seconds where this scene
// sits, `dur` = its length. Flags control the handoff fades:
//   demo_fade_in : global black fade-in (only when this entry opens the demo)
//   fade_out     : fade to black at end (handoff to the next scene)
//   wrap         : dissolve back into the first scene at the very end (loop)
// ---------------- Asset / output paths ----------------
// Relative to the CWD so the packaged project runs anywhere.
// Override with PLASMA_ASSET_DIR / PLASMA_FRAMES_DIR.
fn asset_dir() -> String {
    std::env::var("PLASMA_ASSET_DIR").unwrap_or_else(|_| ".".to_string())
}
fn frames_dir() -> String {
    std::env::var("PLASMA_FRAMES_DIR").unwrap_or_else(|_| "frames".to_string())
}
fn asset(name: &str) -> String {
    format!("{}/{}", asset_dir().trim_end_matches('/'), name)
}

// ---------------- Scene data: textures + sync + timeline ----------------
struct SceneData {
    texs: Vec<Tex>,
    blurs: Vec<Tex>,
    beat: BeatSync,
    timeline: Vec<(Scene, f32, f32)>,
}
impl SceneData {
    fn load() -> Self {
        let names = ["tex_purple.png", "tex_green.png", "tex_blue.png",
                     "tex_scene3.png", "tex_scene4.png"];
        let texs: Vec<Tex> = names.iter().map(|n| Tex::load(&asset(n))).collect();
        let blurs: Vec<Tex> = texs.iter().map(|t| t.blur()).collect();
        let beat = BeatSync::load(&asset("beats.txt"), &asset("kicks.txt"), 0.22, SONG_LEN);
        // TIMELINE: (scene, demo_start_sec, duration_sec) -- the sync contract.
        let timeline: Vec<(Scene, f32, f32)> = vec![
            (Scene::Rotozoom, 0.0, 16.0),
            (Scene::TriangleDance, 16.0, 16.0),
            (Scene::CyberPuzzle, 32.0, 8.2), // unify-on-snare + jiggle + snare-triggered flip reveal
        ];
        SceneData { texs, blurs, beat, timeline }
    }
    fn tex_refs(&self) -> [&Tex; 5] {
        [&self.texs[0], &self.texs[1], &self.texs[2], &self.texs[3], &self.texs[4]]
    }
    fn blur_refs(&self) -> [&Tex; 5] {
        [&self.blurs[0], &self.blurs[1], &self.blurs[2], &self.blurs[3], &self.blurs[4]]
    }
}

// Which texture slot pair a scene cross-fades between.
fn scene_segs(scene: Scene) -> [usize; 2] {
    match scene {
        Scene::Rotozoom => [0, 1],
        Scene::TriangleDance => [2, 1],
        Scene::CyberPuzzle => [3, 3],
    }
}

// Render one frame of the FULL timeline at demo-local time `t` (looping).
// The realtime player's equivalent of render_range's inner loop: identical
// scene math and handoff fades, but driven by wall-clock time instead of a
// frame counter.
#[allow(dead_code)]
fn timeline_frame(sd: &SceneData, texs: &[&Tex; 5], blurs: &[&Tex; 5], t: f32)
    -> ImageBuffer<Rgb<u8>, Vec<u8>>
{
    let total: f32 = sd.timeline.last().map(|(_, s, d)| s + d).unwrap_or(SEG_SECS).max(0.001);
    let t = t.rem_euclid(total);
    let (scene, start, dur) = *sd.timeline.iter()
        .find(|(_, s, d)| t >= *s && t < *s + *d)
        .unwrap_or_else(|| sd.timeline.last().unwrap());
    let st = t - start;      // scene-local time
    let gt = t;              // demo-timeline time == song position
    let sj = ((st / SEG_SECS) as usize).min(1);
    let fl = st - sj as f32 * SEG_SECS;
    let tex_mix = if sj == 0 && fl >= SEG_SECS - FADE_SECS {
        smooth((fl - (SEG_SECS - FADE_SECS)) / FADE_SECS)
    } else { 0.0 };
    let segs = scene_segs(scene);
    let (t0, t1) = (segs[sj], segs[1 - sj]);
    let punch = sd.beat.punch(gt);
    let mut frame = frame_for(scene, texs, blurs, t0, t1, st, gt, tex_mix, punch, &sd.beat);
    if gt < FADE_SECS { fade(&mut frame, smooth(gt / FADE_SECS)); }
    match scene {
        Scene::Rotozoom => {
            if st > dur - FADE_SECS {
                fade(&mut frame, 1.0 - smooth((st - (dur - FADE_SECS)) / FADE_SECS));
            }
        }
        Scene::TriangleDance => {
            if st < FADE_SECS { fade(&mut frame, smooth(st / FADE_SECS)); }
        }
        Scene::CyberPuzzle => {}
    }
    frame
}

// Render one timeline entry (headless frame dumper): `start` = demo-timeline
// seconds where this scene sits, `dur` = its length. Flags control the handoff
// fades:
//   demo_fade_in : global black fade-in (only when this entry opens the demo)
//   fade_out     : fade to black at end (handoff to the next scene)
//   wrap         : dissolve back into the first scene at the very end (loop)
fn render_range(scene: Scene, start: f32, dur: f32,
                texs: &[&Tex; 5], blurs: &[&Tex; 5], beat: &BeatSync,
                demo_fade_in: bool, fade_out: bool, wrap: bool, idx0: usize)
    -> usize
{
    let segs = scene_segs(scene);
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
        let t0 = segs[sj];
        let t1 = segs[1 - sj];
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
            Scene::CyberPuzzle => {}
        }
        frame.save(format!("{}/f{:05}.png", frames_dir(), idx)).unwrap();
        idx += 1;
        f += 1;
    }
    idx
}

// ---------------- Entry points ----------------
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg1 = args.get(1).map(|s| s.as_str()).unwrap_or("");
    let headless_cmd = matches!(arg1, "demo" | "dump" | "dev");

    // Default (no args) = realtime SDL3 window + music, when built with the
    // `realtime` feature. `demo` / `dump` / `dev` always use the headless
    // frame dumper (cargo run --no-default-features ... or cargo run -- demo).
    #[cfg(feature = "realtime")]
    {
        if !headless_cmd {
            let sd = SceneData::load();
            realtime::run(&sd);
            return;
        }
    }
    #[cfg(not(feature = "realtime"))]
    {
        if !headless_cmd {
            eprintln!("(built without the `realtime` feature -> headless frame dumper)");
        }
    }

    // ---- headless frame dumper ----
    let sd = SceneData::load();
    let texs = sd.tex_refs();
    let blurs = sd.blur_refs();
    std::fs::create_dir_all(frames_dir()).expect("create frames dir");
    let out = frames_dir();
    let mode = if arg1.is_empty() { "demo" } else { arg1 };

    // hidden: render ONE frame with the realtime path's timeline math (verification)
    if mode == "tframe" {
        let t: f32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let frame = timeline_frame(&sd, &texs, &blurs, t);
        frame.save(format!("{}/tframe.png", frames_dir())).unwrap();
        eprintln!("TFRAME t={:.4}s -> {}/tframe.png", t, out);
        return;
    }

    if mode == "dev" {
        // dev mode: single scene, exits when done. Music starts at start % SONG_LEN.
        let name = args.get(2).map(|s| s.as_str()).unwrap_or("");
        let sc = parse_scene(name)
            .unwrap_or_else(|| panic!("usage: plasma_warp dev <rotozoom|triangledance|cyberpuzzle>"));
        let (scene, start, dur) = *sd.timeline.iter().find(|(s, _, _)| *s == sc)
            .unwrap_or_else(|| panic!("scene not in timeline"));
        eprintln!("DEV MODE: {:?} | demo-timeline start {:.3}s, dur {:.1}s", scene, start, dur);
        let n = render_range(scene, start, dur, &texs, &blurs, &sd.beat, start == 0.0, false, false, 0);
        eprintln!("ALL FRAMES DONE ({}) -> {}/", n, out);
        eprintln!("AUDIO_OFFSET={:.3}", start % SONG_LEN);
    } else {
        // demo mode: full timeline in order, with handoffs and loop wrap
        let mut idx = 0usize;
        for (i, &(scene, start, dur)) in sd.timeline.iter().enumerate() {
            let last = i == sd.timeline.len() - 1;
            eprintln!("DEMO: scene {:?} @ {:.1}s", scene, start);
            idx = render_range(scene, start, dur, &texs, &blurs, &sd.beat,
                               start == 0.0, !last, last, idx);
        }
        eprintln!("ALL FRAMES DONE ({}) -> {}/", idx, out);
    }
}

// ---------------- Realtime SDL3 player (Cargo feature "realtime") ----------------
// Renders the same frames the headless dumper would write, but straight into an
// SDL3 window at 30 fps, with meltdown_beat.ogg playing underneath via
// SDL3_mixer (looping). Pointer-driven, no gamepad.
#[cfg(feature = "realtime")]
mod realtime {
    use super::*;
    use sdl3::event::Event;
    use sdl3::keyboard::Keycode;
    use sdl3::mixer;
    use sdl3::pixels::{Color, PixelFormat};
    use sdl3::render::{ScaleMode, TextureAccess};
    use std::time::{Duration, Instant};

    pub fn run(sd: &SceneData) {
        let texs = sd.tex_refs();
        let blurs = sd.blur_refs();

        let sdl = sdl3::init().expect("SDL_Init failed");

        // ---- window + renderer + streaming texture ----
        let video = sdl.video().expect("SDL video subsystem");
        let window = video
            .window("plasma_warp \u{2014} music-synced demo", (W * 2) as u32, (H * 2) as u32)
            .position_centered()
            .resizable()
            .build()
            .expect("create window");
        let mut canvas = window.into_canvas();
        // logical resolution = render size; SDL scales + letterboxes it into the window
        let _ = canvas.set_logical_size(
            W as u32, H as u32,
            sdl3::sys::render::SDL_RendererLogicalPresentation::LETTERBOX);
        let creator = canvas.texture_creator();
        let mut tex = creator
            .create_texture(PixelFormat::RGB24, TextureAccess::Streaming, W as u32, H as u32)
            .expect("create streaming texture");
        tex.set_scale_mode(ScaleMode::Linear);

        // ---- audio: SDL3_mixer (init must follow SDL_Init; needs the audio subsystem) ----
        let _audio_subsys = sdl.audio().expect("SDL audio subsystem");
        let _mix_ctx = mixer::init().expect("MIX_Init failed");
        let mixer = mixer::Mixer::open_device(None).expect("open default mixer device");
        let music = mixer
            .load_audio(asset("meltdown_beat.ogg"), true)
            .expect("load meltdown_beat.ogg");
        let track = mixer.create_track().expect("create mixer track");
        track.set_audio(&music).expect("assign audio to track");
        track.set_loops(-1).expect("loop forever");     // -1 == play forever
        track.play().expect("start playback");
        eprintln!("realtime: SDL3_mixer {} decoders, music looping",
                  mixer::get_num_audio_decoders());

        let max_frames: u64 = std::env::var("PLASMA_MAX_FRAMES")
            .ok().and_then(|v| v.parse().ok()).unwrap_or(0);

        let mut events = sdl.event_pump().expect("event pump");
        let start = Instant::now();
        let frame_dt = Duration::from_secs_f64(1.0 / FPS as f64);
        let mut next_frame = start;
        let mut frames: u64 = 0;

        'running: loop {
            for ev in events.poll_iter() {
                match ev {
                    Event::Quit { .. } => break 'running,
                    Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break 'running,
                    _ => {}
                }
            }

            let t = start.elapsed().as_secs_f32();
            let frame = timeline_frame(sd, &texs, &blurs, t);
            let _ = tex.update(None, frame.as_raw(), (W * 3) as usize);

            canvas.set_draw_color(Color::RGB(0, 0, 0));
            canvas.clear();
            let _ = canvas.copy(&tex, None, None);
            canvas.present();

            frames += 1;
            if max_frames > 0 && frames >= max_frames { break 'running; }

            next_frame += frame_dt;
            let now = Instant::now();
            if next_frame > now { std::thread::sleep(next_frame - now); } else { next_frame = now; }
        }
        eprintln!("realtime: {} frames rendered", frames);
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
