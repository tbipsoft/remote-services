#![allow(clippy::too_many_arguments)]
//! Renderizador 2D por software, sem SDL.
//!
//! O visual novo (cantos arredondados, brilhos, tracejados, gradiente conico e
//! texto com Manrope/JetBrains Mono) nao existe no `Canvas` do SDL2, e depender de
//! SDL2_gfx/SDL2_ttf exigiria pacotes extras no aparelho. Entao desenhamos num
//! `Pixmap` (BGRA pre-multiplicado, igual ao ARGB8888 do SDL em little-endian) com
//! anti-aliasing por distancia assinada e mandamos o quadro pronto para uma textura.
//! Como nao depende do SDL, tambem roda nos testes e gera capturas de tela.

use ab_glyph::{point, Font, FontRef, GlyphId, PxScale, ScaleFont};
use std::{f32::consts::PI, io::Write, path::Path};

// ---------------------------------------------------------------------------
// Cor
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }

    /// `0xRRGGBB`.
    pub const fn hex(v: u32) -> Color {
        Color::rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    /// Multiplica o alfa por `a` (0..1).
    pub fn alpha(self, a: f32) -> Color {
        Color {
            a: (self.a as f32 * a.clamp(0.0, 1.0)).round() as u8,
            ..self
        }
    }

    pub fn lerp(self, o: Color, t: f32) -> Color {
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Color {
            r: m(self.r, o.r),
            g: m(self.g, o.g),
            b: m(self.b, o.b),
            a: m(self.a, o.a),
        }
    }

    /// `hsl(h, s, l)` com h em graus e s, l em 0..1.
    pub fn hsl(h: f32, s: f32, l: f32) -> Color {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let hp = (h.rem_euclid(360.0)) / 60.0;
        let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
        let (r, g, b) = match hp as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = l - c / 2.0;
        let q = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        Color::rgb(q(r), q(g), q(b))
    }
}

/// Gradiente conico estilo CSS: stops igualmente espacados em 0..1, comecando no
/// topo e girando no sentido horario. `u` em 0..1.
pub fn conic(stops: &[Color], u: f32) -> Color {
    let n = stops.len() - 1;
    let p = u.rem_euclid(1.0) * n as f32;
    let i = (p as usize).min(n - 1);
    stops[i].lerp(stops[i + 1], p - i as f32)
}

// ---------------------------------------------------------------------------
// Geometria (distancias assinadas, em pixels)
// ---------------------------------------------------------------------------

#[inline]
fn cov(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

#[inline]
fn sd_rrect(px: f32, py: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - (hw - r);
    let qy = (py - cy).abs() - (hh - r);
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

/// Borda de retangulo arredondado (anel entre o retangulo externo e o interno).
struct Ring {
    cx: f32,
    cy: f32,
    hw: f32,
    hh: f32,
    r: f32,
    ihw: f32,
    ihh: f32,
    ir: f32,
}

impl Ring {
    fn new(x: f32, y: f32, w: f32, h: f32, r: f32, bw: f32) -> Ring {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        Ring {
            cx: x + w / 2.0,
            cy: y + h / 2.0,
            hw: w / 2.0,
            hh: h / 2.0,
            r,
            ihw: (w / 2.0 - bw).max(0.0),
            ihh: (h / 2.0 - bw).max(0.0),
            ir: (r - bw).max(0.0),
        }
    }

    fn cov(&self, px: f32, py: f32) -> f32 {
        let o = cov(sd_rrect(px, py, self.cx, self.cy, self.hw, self.hh, self.r));
        if o <= 0.0 {
            return 0.0;
        }
        let i = cov(sd_rrect(
            px, py, self.cx, self.cy, self.ihw, self.ihh, self.ir,
        ));
        o * (1.0 - i)
    }
}

/// Posicao (comprimento de arco, sentido horario a partir do fim do canto
/// superior esquerdo) do ponto mais proximo na linha central do retangulo
/// arredondado de meia-largura `hw`, meia-altura `hh` e raio `r`. Usado para
/// tracejar a borda de forma continua pelos cantos.
fn rrect_param(qx: f32, qy: f32, hw: f32, hh: f32, r: f32) -> f32 {
    let (ix, iy) = ((hw - r).max(0.0), (hh - r).max(0.0));
    let (ax, ay) = (qx.abs(), qy.abs());
    let q = PI * r / 2.0; // um quarto de arco
    let top = 2.0 * ix;
    let side = 2.0 * iy;
    if ax > ix && ay > iy {
        let (lx, ly) = (ax - ix, ay - iy);
        return match (qx > 0.0, qy > 0.0) {
            // TR: do topo para a direita
            (true, false) => top + r * lx.atan2(ly),
            // BR: da direita para baixo
            (true, true) => top + q + side + r * ly.atan2(lx),
            // BL: de baixo para a esquerda
            (false, true) => top + 2.0 * q + side + top + r * lx.atan2(ly),
            // TL: da esquerda para cima
            (false, false) => 2.0 * top + 3.0 * q + 2.0 * side + r * ly.atan2(lx),
        };
    }
    if hh - ay < hw - ax {
        // borda horizontal
        if qy < 0.0 {
            qx.clamp(-ix, ix) + ix
        } else {
            top + q + side + q + (ix - qx.clamp(-ix, ix))
        }
    } else if qx > 0.0 {
        top + q + (qy.clamp(-iy, iy) + iy)
    } else {
        2.0 * top + 3.0 * q + side + (iy - qy.clamp(-iy, iy))
    }
}

/// Cobertura do traco numa sequencia "tracos de `dash` a cada `period`".
fn dash_cov(t: f32, period: f32, dash: f32) -> f32 {
    let s = t.rem_euclid(period);
    let c = |s: f32| ((s.min(dash - s)) + 0.5).clamp(0.0, 1.0);
    c(s).max(c(s - period))
}

// ---------------------------------------------------------------------------
// Fontes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Sans500,
    Sans600,
    Sans700,
    Sans800,
    Mono500,
    Mono700,
}

/// Fontes do design, embutidas: Manrope (500/600/700/800) e JetBrains Mono (500/700),
/// ambas SIL OFL 1.1 (veja `assets/fonts/OFL-*.txt`), em subconjunto ASCII.
pub struct Fonts {
    faces: [FontRef<'static>; 6],
}

impl Fonts {
    pub fn new() -> Fonts {
        fn load(b: &'static [u8]) -> FontRef<'static> {
            FontRef::try_from_slice(b).expect("fonte embutida invalida")
        }
        Fonts {
            faces: [
                load(include_bytes!("../assets/fonts/Manrope-500.ttf")),
                load(include_bytes!("../assets/fonts/Manrope-600.ttf")),
                load(include_bytes!("../assets/fonts/Manrope-700.ttf")),
                load(include_bytes!("../assets/fonts/Manrope-800.ttf")),
                load(include_bytes!("../assets/fonts/JetBrainsMono-500.ttf")),
                load(include_bytes!("../assets/fonts/JetBrainsMono-700.ttf")),
            ],
        }
    }

    fn font(&self, f: Face) -> &FontRef<'static> {
        &self.faces[f as usize]
    }

    /// (ascendente, descendente) em fracao do em; o descendente e negativo.
    pub fn metrics(&self, f: Face) -> (f32, f32) {
        let font = self.font(f);
        let upm = font.units_per_em().unwrap_or(1000.0);
        (font.ascent_unscaled() / upm, font.descent_unscaled() / upm)
    }

    fn scale(&self, f: Face, em_px: f32) -> PxScale {
        let font = self.font(f);
        let upm = font.units_per_em().unwrap_or(1000.0);
        // No ab_glyph, PxScale e a altura (ascendente - descendente) em pixels.
        PxScale::from(em_px * font.height_unscaled() / upm)
    }

    /// Largura do texto como o CSS a calcula: avancos + kerning + `spacing` depois de
    /// cada caractere (inclusive o ultimo).
    pub fn width(&self, f: Face, em_px: f32, spacing: f32, s: &str) -> f32 {
        let font = self.font(f);
        let sc = font.as_scaled(self.scale(f, em_px));
        let mut x = 0.0;
        let mut prev: Option<GlyphId> = None;
        for ch in s.chars() {
            let id = self.glyph(f, ch);
            if let Some(p) = prev {
                x += sc.kern(p, id);
            }
            x += sc.h_advance(id) + spacing;
            prev = Some(id);
        }
        x
    }

    fn glyph(&self, f: Face, ch: char) -> GlyphId {
        let font = self.font(f);
        let id = font.glyph_id(ch);
        if id.0 == 0 {
            font.glyph_id('?')
        } else {
            id
        }
    }

    /// Corta `s` com reticencias para caber em `max_w`.
    pub fn fit(&self, f: Face, em_px: f32, spacing: f32, s: &str, max_w: f32) -> String {
        if self.width(f, em_px, spacing, s) <= max_w {
            return s.to_string();
        }
        let mut out: Vec<char> = s.chars().collect();
        while !out.is_empty() {
            out.pop();
            let t: String = out.iter().collect::<String>() + "\u{2026}";
            if self.width(f, em_px, spacing, &t) <= max_w {
                return t;
            }
        }
        "\u{2026}".into()
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Fonts::new()
    }
}

// ---------------------------------------------------------------------------
// Pixmap
// ---------------------------------------------------------------------------

/// Imagem BGRA pre-multiplicada. Camadas transparentes (alfa < 255) tambem sao
/// `Pixmap`s e entram no quadro por `blit`.
#[derive(Clone)]
pub struct Pixmap {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

type BBox = (f32, f32, f32, f32);

impl Pixmap {
    pub fn new(w: usize, h: usize) -> Pixmap {
        Pixmap {
            w,
            h,
            px: vec![0; w * h * 4],
        }
    }

    pub fn width(&self) -> usize {
        self.w
    }
    pub fn height(&self) -> usize {
        self.h
    }
    pub fn pitch(&self) -> usize {
        self.w * 4
    }
    /// Bytes B,G,R,A (= ARGB8888 do SDL em little-endian).
    pub fn data(&self) -> &[u8] {
        &self.px
    }

    /// Pixel (r, g, b, a) *pre-multiplicado*; `None` fora da imagem.
    pub fn get(&self, x: usize, y: usize) -> Option<[u8; 4]> {
        if x >= self.w || y >= self.h {
            return None;
        }
        let i = (y * self.w + x) * 4;
        Some([self.px[i + 2], self.px[i + 1], self.px[i], self.px[i + 3]])
    }

    /// Reamostra uma imagem RGBA (nao pre-multiplicada, `sw` x `sh`) para `dw` x `dh` com
    /// media de area (boa para reduzir) e devolve o resultado pre-multiplicado.
    pub fn from_rgba_scaled(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Pixmap {
        let mut out = Pixmap::new(dw, dh);
        if src.len() < sw * sh * 4 || dw == 0 || dh == 0 {
            return out;
        }
        let (fx, fy) = (sw as f32 / dw as f32, sh as f32 / dh as f32);
        for y in 0..dh {
            let (y0, y1) = (y as f32 * fy, (y as f32 + 1.0) * fy);
            for x in 0..dw {
                let (x0, x1) = (x as f32 * fx, (x as f32 + 1.0) * fx);
                let (mut r, mut g, mut b, mut a, mut wsum) =
                    (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
                let (iy0, iy1) = (y0.floor() as usize, (y1.ceil() as usize).min(sh));
                let (ix0, ix1) = (x0.floor() as usize, (x1.ceil() as usize).min(sw));
                for sy in iy0..iy1 {
                    let wy = (y1.min(sy as f32 + 1.0) - y0.max(sy as f32)).max(0.0);
                    for sx in ix0..ix1 {
                        let w = wy * (x1.min(sx as f32 + 1.0) - x0.max(sx as f32)).max(0.0);
                        let i = (sy * sw + sx) * 4;
                        let al = src[i + 3] as f32 / 255.0;
                        r += src[i] as f32 * al * w;
                        g += src[i + 1] as f32 * al * w;
                        b += src[i + 2] as f32 * al * w;
                        a += al * 255.0 * w;
                        wsum += w;
                    }
                }
                if wsum > 0.0 {
                    let o = (y * dw + x) * 4;
                    out.px[o] = (b / wsum + 0.5) as u8;
                    out.px[o + 1] = (g / wsum + 0.5) as u8;
                    out.px[o + 2] = (r / wsum + 0.5) as u8;
                    out.px[o + 3] = (a / wsum + 0.5) as u8;
                }
            }
        }
        out
    }

    pub fn clear(&mut self, c: Color) {
        let a = c.a as f32 / 255.0;
        let px = [
            (c.b as f32 * a).round() as u8,
            (c.g as f32 * a).round() as u8,
            (c.r as f32 * a).round() as u8,
            c.a,
        ];
        for p in self.px.chunks_exact_mut(4) {
            p.copy_from_slice(&px);
        }
    }

    /// Copia o conteudo de `o` (e ajusta o tamanho se for diferente).
    pub fn copy_from(&mut self, o: &Pixmap) {
        self.w = o.w;
        self.h = o.h;
        self.px.clear();
        self.px.extend_from_slice(&o.px);
    }

    #[inline]
    fn blend(&mut self, x: i32, y: i32, c: Color, cv: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let a = c.a as f32 * (1.0 / 255.0) * cv;
        if a <= 0.002 {
            return;
        }
        let i = (y as usize * self.w + x as usize) * 4;
        let ia = 1.0 - a;
        let d = &mut self.px[i..i + 4];
        d[0] = (c.b as f32 * a + d[0] as f32 * ia + 0.5) as u8;
        d[1] = (c.g as f32 * a + d[1] as f32 * ia + 0.5) as u8;
        d[2] = (c.r as f32 * a + d[2] as f32 * ia + 0.5) as u8;
        d[3] = (255.0 * a + d[3] as f32 * ia + 0.5) as u8;
    }

    fn clip(&self, b: BBox) -> (i32, i32, i32, i32) {
        (
            (b.0.floor() as i32 - 1).max(0),
            (b.1.floor() as i32 - 1).max(0),
            (b.2.ceil() as i32 + 1).min(self.w as i32),
            (b.3.ceil() as i32 + 1).min(self.h as i32),
        )
    }

    /// Percorre a caixa e pinta cada pixel com a (cor, cobertura) devolvida por `f`,
    /// que recebe o centro do pixel.
    fn shade(&mut self, b: BBox, mut f: impl FnMut(f32, f32) -> (Color, f32)) {
        let (xa, ya, xb, yb) = self.clip(b);
        for y in ya..yb {
            for x in xa..xb {
                let (c, cv) = f(x as f32 + 0.5, y as f32 + 0.5);
                if cv > 0.0 {
                    self.blend(x, y, c, cv);
                }
            }
        }
    }

    // --- formas ---------------------------------------------------------

    pub fn fill_rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let (cx, cy, hw, hh) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
        self.shade((x, y, x + w, y + h), |px, py| {
            (c, cov(sd_rrect(px, py, cx, cy, hw, hh, r)))
        });
    }

    /// Borda de `bw` px para dentro do retangulo.
    pub fn stroke_rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, bw: f32, c: Color) {
        let ring = Ring::new(x, y, w, h, r, bw);
        self.shade((x, y, x + w, y + h), |px, py| (c, ring.cov(px, py)));
    }

    /// Borda tracejada. `dash`/`gap` sao ajustados para fechar um numero inteiro de
    /// tracos no perimetro.
    #[allow(clippy::too_many_arguments)]
    pub fn stroke_rrect_dashed(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        bw: f32,
        c: Color,
        dash: f32,
        gap: f32,
    ) {
        let ring = Ring::new(x, y, w, h, r, bw);
        let (hw, hh) = (ring.hw - bw / 2.0, ring.hh - bw / 2.0);
        let rc = (ring.r - bw / 2.0).max(0.0);
        let perim = 4.0 * (hw - rc).max(0.0) + 4.0 * (hh - rc).max(0.0) + 2.0 * PI * rc;
        let n = (perim / (dash + gap)).round().max(1.0);
        let period = perim / n;
        let dash = dash * period / (dash + gap);
        let (cx, cy) = (ring.cx, ring.cy);
        self.shade((x, y, x + w, y + h), |px, py| {
            let rc_ = ring.cov(px, py);
            if rc_ <= 0.0 {
                return (c, 0.0);
            }
            let t = rrect_param(px - cx, py - cy, hw, hh, rc);
            (c, rc_ * dash_cov(t, period, dash))
        });
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        self.fill_rrect(x, y, w, h, 0.0, c);
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, c: Color) {
        self.fill_circle_with(cx, cy, r, |_, _| c);
    }

    /// Circulo cuja cor depende do ponto (gradientes).
    pub fn fill_circle_with(&mut self, cx: f32, cy: f32, r: f32, f: impl Fn(f32, f32) -> Color) {
        self.shade((cx - r, cy - r, cx + r, cy + r), |px, py| {
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() - r;
            (f(px - cx, py - cy), cov(d))
        });
    }

    /// Anel de raio central `r` e espessura `bw`.
    pub fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32, bw: f32, c: Color) {
        let e = r + bw;
        self.shade((cx - e, cy - e, cx + e, cy + e), |px, py| {
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            (c, cov((d - r).abs() - bw / 2.0))
        });
    }

    pub fn stroke_circle_dashed(
        &mut self,
        cx: f32,
        cy: f32,
        r: f32,
        bw: f32,
        c: Color,
        dash: f32,
        gap: f32,
    ) {
        let perim = 2.0 * PI * r;
        let n = (perim / (dash + gap)).round().max(1.0);
        let period = perim / n;
        let dash = dash * period / (dash + gap);
        let e = r + bw;
        self.shade((cx - e, cy - e, cx + e, cy + e), |px, py| {
            let (dx, dy) = (px - cx, py - cy);
            let d = (dx * dx + dy * dy).sqrt();
            let rc = cov((d - r).abs() - bw / 2.0);
            if rc <= 0.0 {
                return (c, 0.0);
            }
            // sentido horario a partir do topo
            let ang = dx.atan2(-dy).rem_euclid(2.0 * PI);
            (c, rc * dash_cov(ang * r, period, dash))
        });
    }

    // --- brilho (box-shadow sem deslocamento) --------------------------------

    /// Desenha, *sob* o que vier depois, a sombra desfocada de uma forma. `shape`
    /// devolve a cobertura 0..1 do ponto; `sigma` e o desvio do desfoque (CSS:
    /// raio/2). A forma e desfocada por 3 passadas de "box blur" (aprox. gaussiana).
    pub fn glow(&mut self, b: BBox, sigma: f32, c: Color, shape: impl Fn(f32, f32) -> f32) {
        let pad = (sigma * 3.0).ceil() as i32 + 2;
        let x0 = b.0.floor() as i32 - pad;
        let y0 = b.1.floor() as i32 - pad;
        let w = (b.2.ceil() as i32 + pad - x0) as usize;
        let h = (b.3.ceil() as i32 + pad - y0) as usize;
        if w == 0 || h == 0 {
            return;
        }
        let mut a = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                a[y * w + x] = shape((x0 + x as i32) as f32 + 0.5, (y0 + y as i32) as f32 + 0.5);
            }
        }
        let mut t = vec![0.0f32; w * h];
        for r in box_radii(sigma) {
            box_blur(&a, &mut t, w, h, r, true);
            box_blur(&t, &mut a, w, h, r, false);
        }
        for y in 0..h {
            for x in 0..w {
                let v = a[y * w + x];
                if v > 0.002 {
                    self.blend(x0 + x as i32, y0 + y as i32, c, v.min(1.0));
                }
            }
        }
    }

    pub fn glow_circle(&mut self, cx: f32, cy: f32, r: f32, sigma: f32, c: Color) {
        self.glow((cx - r, cy - r, cx + r, cy + r), sigma, c, |px, py| {
            cov(((px - cx).powi(2) + (py - cy).powi(2)).sqrt() - r)
        });
    }

    pub fn glow_rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, sigma: f32, c: Color) {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let (cx, cy, hw, hh) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
        self.glow((x, y, x + w, y + h), sigma, c, |px, py| {
            cov(sd_rrect(px, py, cx, cy, hw, hh, r))
        });
    }

    // --- camadas --------------------------------------------------------------

    /// Compoe `layer` em (`x`, `y`) com opacidade de grupo `opacity` (como o CSS
    /// `opacity` num container).
    pub fn blit(&mut self, layer: &Pixmap, x: i32, y: i32, opacity: f32) {
        let op = opacity.clamp(0.0, 1.0);
        if op <= 0.0 {
            return;
        }
        for ly in 0..layer.h {
            let dy = y + ly as i32;
            if dy < 0 || dy as usize >= self.h {
                continue;
            }
            for lx in 0..layer.w {
                let dx = x + lx as i32;
                if dx < 0 || dx as usize >= self.w {
                    continue;
                }
                let si = (ly * layer.w + lx) * 4;
                let sa = layer.px[si + 3];
                if sa == 0 {
                    continue;
                }
                let di = (dy as usize * self.w + dx as usize) * 4;
                let ia = 1.0 - (sa as f32 / 255.0) * op;
                for k in 0..4 {
                    self.px[di + k] =
                        (layer.px[si + k] as f32 * op + self.px[di + k] as f32 * ia + 0.5) as u8;
                }
            }
        }
    }

    // --- texto ---------------------------------------------------------------

    /// Desenha `s` com a linha de base em `baseline` e o inicio em `x`. `em_px` e o
    /// tamanho da fonte (como o `font-size` do CSS) e `spacing` o `letter-spacing`.
    /// Devolve a largura desenhada.
    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self,
        fonts: &Fonts,
        face: Face,
        em_px: f32,
        spacing: f32,
        c: Color,
        x: f32,
        baseline: f32,
        s: &str,
    ) -> f32 {
        let font = fonts.font(face);
        let scale = fonts.scale(face, em_px);
        let sc = font.as_scaled(scale);
        let mut pen = x;
        let mut prev: Option<GlyphId> = None;
        for ch in s.chars() {
            let id = fonts.glyph(face, ch);
            if let Some(p) = prev {
                pen += sc.kern(p, id);
            }
            let glyph = id.with_scale_and_position(scale, point(pen, baseline));
            if let Some(og) = font.outline_glyph(glyph) {
                let bb = og.px_bounds();
                let (bx, by) = (bb.min.x as i32, bb.min.y as i32);
                og.draw(|gx, gy, cv| {
                    if cv > 0.0 {
                        self.blend(bx + gx as i32, by + gy as i32, c, cv);
                    }
                });
            }
            pen += sc.h_advance(id) + spacing;
            prev = Some(id);
        }
        pen - x
    }

    // --- saida -----------------------------------------------------------------

    /// Grava como PPM (P6), RGB sobre fundo preto. Serve para capturas de tela.
    pub fn write_ppm(&self, path: &Path) -> std::io::Result<()> {
        let mut out = Vec::with_capacity(self.w * self.h * 3 + 32);
        write!(out, "P6\n{} {}\n255\n", self.w, self.h)?;
        for p in self.px.chunks_exact(4) {
            out.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        std::fs::write(path, out)
    }
}

// ---------------------------------------------------------------------------
// Desfoque
// ---------------------------------------------------------------------------

/// Raios de 3 "box blurs" que aproximam uma gaussiana de desvio `sigma`.
fn box_radii(sigma: f32) -> [usize; 3] {
    let n = 3.0f32;
    let ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = ideal.floor() as i32;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wl = wl.max(1);
    let wu = wl + 2;
    let wlf = wl as f32;
    let m = ((12.0 * sigma * sigma - n * wlf * wlf - 4.0 * n * wlf - 3.0 * n) / (-4.0 * wlf - 4.0))
        .round() as i32;
    let mut r = [0usize; 3];
    for (i, v) in r.iter_mut().enumerate() {
        let size = if (i as i32) < m { wl } else { wu };
        *v = ((size - 1) / 2) as usize;
    }
    r
}

/// Media numa janela de `2r+1` com zeros fora da imagem; `horizontal` escolhe o eixo.
fn box_blur(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, horizontal: bool) {
    let norm = 1.0 / (2 * r + 1) as f32;
    let (lines, len, step, lstep) = if horizontal {
        (h, w, 1usize, w)
    } else {
        (w, h, w, 1usize)
    };
    for l in 0..lines {
        let base = l * lstep;
        let mut sum = 0.0f32;
        for i in 0..=r.min(len.saturating_sub(1)) {
            sum += src[base + i * step];
        }
        for i in 0..len {
            dst[base + i * step] = sum * norm;
            let add = i + r + 1;
            if add < len {
                sum += src[base + add * step];
            }
            if i >= r {
                sum -= src[base + (i - r) * step];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Color = Color::rgb(255, 0, 0);

    fn px(p: &Pixmap, x: usize, y: usize) -> [u8; 4] {
        p.get(x, y).unwrap()
    }

    #[test]
    fn cores() {
        assert_eq!(Color::hex(0x2EE66B), Color::rgb(0x2E, 0xE6, 0x6B));
        assert_eq!(RED.alpha(0.5).a, 128);
        assert_eq!(
            Color::rgb(0, 0, 0).lerp(Color::rgb(200, 100, 50), 0.5),
            Color::rgb(100, 50, 25)
        );
        // hsl(0,100%,50%) = vermelho; hsl(120,...) = verde; hsl(240,...) = azul
        assert_eq!(Color::hsl(0.0, 1.0, 0.5), Color::rgb(255, 0, 0));
        assert_eq!(Color::hsl(120.0, 1.0, 0.5), Color::rgb(0, 255, 0));
        assert_eq!(Color::hsl(240.0, 1.0, 0.5), Color::rgb(0, 0, 255));
    }

    #[test]
    fn conico_percorre_os_stops() {
        let s = [
            Color::rgb(255, 0, 0),
            Color::rgb(0, 255, 0),
            Color::rgb(255, 0, 0),
        ];
        assert_eq!(conic(&s, 0.0), s[0]);
        assert_eq!(conic(&s, 0.5), s[1]);
        assert_eq!(conic(&s, 1.0), s[0]); // volta ao inicio
        assert_eq!(conic(&s, 0.25), Color::rgb(128, 128, 0));
    }

    #[test]
    fn retangulo_arredondado_tem_cantos_vazados_e_borda_suave() {
        let mut p = Pixmap::new(40, 40);
        p.fill_rrect(4.0, 4.0, 32.0, 32.0, 10.0, RED);
        assert_eq!(px(&p, 20, 20), [255, 0, 0, 255]); // miolo cheio
        assert_eq!(px(&p, 4, 4)[3], 0); // canto vazado
        assert_eq!(px(&p, 0, 20), [0, 0, 0, 0]); // fora
        let edge = px(&p, 4, 20); // borda reta exatamente alinhada ao pixel
        assert_eq!(edge[3], 255);
        // borda deslocada meio pixel: cobertura parcial (anti-aliasing)
        let mut q = Pixmap::new(40, 40);
        q.fill_rrect(4.5, 4.0, 31.0, 32.0, 0.0, RED);
        let a = px(&q, 4, 20)[3];
        assert!((120..=135).contains(&a), "{a}");
    }

    #[test]
    fn contorno_deixa_o_miolo_vazio() {
        let mut p = Pixmap::new(60, 40);
        p.stroke_rrect(2.0, 2.0, 56.0, 36.0, 8.0, 2.0, RED);
        assert_eq!(px(&p, 30, 3)[3], 255); // sobre a borda
        assert_eq!(px(&p, 30, 20)[3], 0); // miolo
        assert_eq!(px(&p, 3, 20)[3], 255);
    }

    #[test]
    fn contorno_tracejado_alterna_tracos_e_vaos_e_fecha_o_perimetro() {
        let mut p = Pixmap::new(140, 40);
        p.stroke_rrect_dashed(2.0, 2.0, 136.0, 36.0, 10.0, 2.0, RED, 6.0, 4.0);
        // percorre a borda superior (y = 3): tem trechos cheios e vazios
        let row: Vec<u8> = (14..126).map(|x| px(&p, x, 3)[3]).collect();
        let on = row.iter().filter(|&&a| a > 200).count();
        let off = row.iter().filter(|&&a| a < 30).count();
        assert!(on > 20 && off > 15, "on={on} off={off}");
        assert_eq!(px(&p, 70, 20)[3], 0); // miolo continua vazio
    }

    #[test]
    fn param_do_arco_e_continuo_e_cobre_o_perimetro() {
        // anda pela linha central e confere que o parametro cresce sem saltos
        let (hw, hh, r) = (40.0f32, 20.0f32, 8.0f32);
        let perim = 4.0 * (hw - r) + 4.0 * (hh - r) + 2.0 * PI * r;
        let mut last = rrect_param(-(hw - r), -hh, hw, hh, r);
        let mut total = 0.0;
        let n = 4000;
        for i in 1..=n {
            let s = perim * i as f32 / n as f32;
            // ponto na linha central a distancia de arco `s` do inicio
            let (x, y) = centerline(s, hw, hh, r);
            let t = rrect_param(x, y, hw, hh, r);
            let d = (t - last).rem_euclid(perim);
            assert!(d < perim * 0.02, "salto {d} em s={s}");
            total += d;
            last = t;
        }
        assert!((total - perim).abs() < perim * 0.01, "{total} vs {perim}");
    }

    fn centerline(s: f32, hw: f32, hh: f32, r: f32) -> (f32, f32) {
        let (ix, iy) = (hw - r, hh - r);
        let q = PI * r / 2.0;
        let segs = [2.0 * ix, q, 2.0 * iy, q, 2.0 * ix, q, 2.0 * iy, q];
        let mut s = s;
        let mut k = 0;
        while k < 7 && s > segs[k] {
            s -= segs[k];
            k += 1;
        }
        match k {
            0 => (-ix + s, -hh),
            1 => (ix + r * (s / r).sin(), -iy - r * (s / r).cos()),
            2 => (hw, -iy + s),
            3 => (ix + r * (s / r).cos(), iy + r * (s / r).sin()),
            4 => (ix - s, hh),
            5 => (-ix - r * (s / r).sin(), iy + r * (s / r).cos()),
            6 => (-hw, iy - s),
            _ => (-ix - r * (s / r).cos(), -iy - r * (s / r).sin()),
        }
    }

    #[test]
    fn circulo_e_anel_pontilhado() {
        let mut p = Pixmap::new(40, 40);
        p.fill_circle(20.0, 20.0, 8.0, RED);
        assert_eq!(px(&p, 20, 20), [255, 0, 0, 255]);
        assert_eq!(px(&p, 20, 5)[3], 0);
        let mut q = Pixmap::new(40, 40);
        q.stroke_circle(20.0, 20.0, 10.0, 2.0, RED);
        assert_eq!(px(&q, 20, 20)[3], 0);
        assert!(px(&q, 30, 20)[3] > 200);
        let mut d = Pixmap::new(40, 40);
        d.stroke_circle_dashed(20.0, 20.0, 12.0, 1.5, RED, 4.5, 3.0);
        let ring: Vec<u8> = (0..360)
            .map(|a| {
                let r = (a as f32).to_radians();
                px(
                    &d,
                    (20.0 + 12.0 * r.sin()) as usize,
                    (20.0 - 12.0 * r.cos()) as usize,
                )[3]
            })
            .collect();
        assert!(ring.iter().any(|&a| a > 150) && ring.iter().any(|&a| a < 40));
    }

    #[test]
    fn brilho_espalha_e_enfraquece_com_a_distancia() {
        let mut p = Pixmap::new(80, 80);
        p.glow_circle(40.0, 40.0, 7.0, 5.0, RED);
        let near = px(&p, 40, 30)[3];
        let mid = px(&p, 40, 22)[3];
        let far = px(&p, 40, 8)[3];
        assert!(near > mid && mid > far, "{near} {mid} {far}");
        // disco r=7 com sigma=5: ~0,2 de intensidade a 3 px da borda (conta analitica)
        assert!((35..90).contains(&near) && far < 10, "{near} {far}");
        // a energia total e conservada pelo desfoque (area do disco ~ 154 px)
        let sum: f32 = (0..80)
            .flat_map(|y| (0..80).map(move |x| (x, y)))
            .map(|(x, y)| px(&p, x, y)[3] as f32 / 255.0)
            .sum();
        let disc = PI * 49.0;
        assert!((sum - disc).abs() < disc * 0.12, "{sum} vs {disc}");
    }

    #[test]
    fn camada_com_opacidade_de_grupo() {
        let mut layer = Pixmap::new(10, 10);
        layer.fill_rect(0.0, 0.0, 10.0, 10.0, RED);
        let mut base = Pixmap::new(20, 20);
        base.clear(Color::rgb(0, 0, 255));
        base.blit(&layer, 5, 5, 0.5);
        let m = px(&base, 8, 8);
        assert!(
            (m[0] as i32 - 128).abs() <= 1 && (m[2] as i32 - 128).abs() <= 1,
            "{m:?}"
        );
        assert_eq!(px(&base, 1, 1), [0, 0, 255, 255]); // fora da camada
        base.blit(&layer, 5, 5, 0.0); // opacidade 0 nao altera
        assert_eq!(px(&base, 8, 8), m);
    }

    #[test]
    fn fontes_medem_e_desenham() {
        let f = Fonts::new();
        let w = f.width(Face::Sans700, 16.0, 0.0, "Rainbow");
        assert!(w > 40.0 && w < 100.0, "{w}");
        // letter-spacing soma por caractere
        let w2 = f.width(Face::Sans700, 16.0, 2.0, "Rainbow");
        assert!((w2 - w - 14.0).abs() < 0.01);
        // peso maior = mais largo
        assert!(
            f.width(Face::Sans800, 16.0, 0.0, "Rainbow")
                > f.width(Face::Sans500, 16.0, 0.0, "Rainbow")
        );
        // mono: todos os caracteres com o mesmo avanco (0,6 em)
        let m = f.width(Face::Mono700, 20.0, 0.0, "0123");
        assert!((m - 4.0 * 12.0).abs() < 0.2, "{m}");
        let (asc, desc) = f.metrics(Face::Sans700);
        assert!(asc > 0.8 && desc < -0.1);

        let mut p = Pixmap::new(120, 30);
        let adv = p.text(&f, Face::Sans700, 16.0, 0.0, RED, 4.0, 22.0, "Green");
        assert!((adv - f.width(Face::Sans700, 16.0, 0.0, "Green")).abs() < 0.01);
        let inked = (0..120)
            .flat_map(|x| (0..30).map(move |y| (x, y)))
            .filter(|&(x, y)| px(&p, x, y)[3] > 128)
            .count();
        assert!(inked > 40, "{inked}");
        // caractere ausente vira '?'
        assert_eq!(
            f.width(Face::Sans700, 16.0, 0.0, "\u{4e2d}"),
            f.width(Face::Sans700, 16.0, 0.0, "?")
        );
    }

    #[test]
    fn fit_corta_com_reticencias() {
        let f = Fonts::new();
        let s = "Permission denied (os error 13)";
        assert_eq!(f.fit(Face::Sans700, 14.0, 0.0, s, 1000.0), s);
        let cut = f.fit(Face::Sans700, 14.0, 0.0, s, 100.0);
        assert!(cut.ends_with('\u{2026}') && cut.len() < s.len());
        assert!(f.width(Face::Sans700, 14.0, 0.0, &cut) <= 100.0);
    }

    #[test]
    fn ppm_tem_cabecalho_e_tamanho_certos() {
        let mut p = Pixmap::new(3, 2);
        p.clear(Color::rgb(1, 2, 3));
        let path = std::env::temp_dir().join(format!("rg52gfx-{}.ppm", std::process::id()));
        p.write_ppm(&path).unwrap();
        let b = std::fs::read(&path).unwrap();
        assert!(b.starts_with(b"P6\n3 2\n255\n"));
        assert_eq!(b.len(), 11 + 18);
        assert_eq!(&b[11..14], &[1, 2, 3]);
        let _ = std::fs::remove_file(path);
    }
}
