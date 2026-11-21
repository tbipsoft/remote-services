//! Tela do app: o design 960x540 (tema escuro, Manrope + JetBrains Mono), no mesmo
#![allow(clippy::too_many_arguments)]
//! visual de rg52-led / rg52-info / rg52-pad. Um interruptor grande liga e desliga os
//! Remote Services; ao lado, o estado de cada servico e o endereco para conectar.

use crate::{
    gfx::{Color, Face, Fonts, Pixmap},
    remote::{Action, Level, Progress, Svcs},
};

pub const DESIGN_W: f32 = 960.0;
pub const DESIGN_H: f32 = 540.0;

const BG: Color = Color::hex(0x0B0D12);
const TEXT: Color = Color::hex(0xE8ECF4);
const MUTED: Color = Color::hex(0x8E98AD);
const LINE: Color = Color::hex(0x1D2332);
const CARD: Color = Color::hex(0x12161F);
const TRACK: Color = Color::hex(0x242B3B);
const KEYCAP_BG: Color = Color::hex(0x1B2130);
const KEYCAP_BORDER: Color = Color::hex(0x2A3144);
const OK: Color = Color::hex(0x4ADE80);
const WARN: Color = Color::hex(0xFFB938);
const BAD: Color = Color::hex(0xFF6B6B);
pub const ACCENT: Color = Color::hex(0xFFB938);

pub fn parse_accent(s: &str) -> Result<Color, String> {
    match s.to_ascii_lowercase().as_str() {
        "amber" => Ok(ACCENT),
        "teal" => Ok(Color::hex(0x5EEAD4)),
        "violet" => Ok(Color::hex(0xA78BFA)),
        "coral" => Ok(Color::hex(0xFF7A59)),
        t => {
            let h = t.strip_prefix('#').unwrap_or(t);
            match (h.len(), u32::from_str_radix(h, 16)) {
                (6, Ok(v)) => Ok(Color::hex(v)),
                _ => Err(format!(
                    "cor invalida: {s:?} (use amber, teal, violet, coral ou #RRGGBB)"
                )),
            }
        }
    }
}

struct P<'a> {
    pm: &'a mut Pixmap,
    f: &'a Fonts,
    k: f32,
    ox: f32,
    oy: f32,
}

#[derive(Clone, Copy)]
struct R {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Logo (RGBA, 264x56 = 2x do tamanho no design) e o tamanho em que e desenhado.
const LOGO: &[u8] = include_bytes!("../assets/logo_264x56.rgba");
const LOGO_SRC: (usize, usize) = (264, 56);
const LOGO_W: f32 = 132.0;
const LOGO_H: f32 = 27.8;
const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"));

impl P<'_> {
    /// Desenha o logo centrado em (`cx`, `cy`) (coordenadas do design). A versao
    /// reamostrada fica em cache, ja que o desenho pode rodar uma vez por segundo.
    fn logo(&mut self, cx: f32, cy: f32) {
        thread_local! {
            static CACHE: std::cell::RefCell<Option<(usize, usize, Pixmap)>> =
                const { std::cell::RefCell::new(None) };
        }
        let (dw, dh) = (
            (LOGO_W * self.k).round() as usize,
            (LOGO_H * self.k).round() as usize,
        );
        let (x, y) = (
            (self.ox + (cx - LOGO_W / 2.0) * self.k).round() as i32,
            (self.oy + (cy - LOGO_H / 2.0) * self.k).round() as i32,
        );
        CACHE.with(|c| {
            let mut c = c.borrow_mut();
            if !matches!(&*c, Some((w, h, _)) if *w == dw && *h == dh) {
                let pm = Pixmap::from_rgba_scaled(LOGO, LOGO_SRC.0, LOGO_SRC.1, dw, dh);
                *c = Some((dw, dh, pm));
            }
            if let Some((_, _, pm)) = &*c {
                self.pm.blit(pm, x, y, 1.0);
            }
        });
    }
    fn rrect(&mut self, r: R, rad: f32, c: Color) {
        let k = self.k;
        self.pm.fill_rrect(
            self.ox + r.x * k,
            self.oy + r.y * k,
            r.w * k,
            r.h * k,
            rad * k,
            c,
        );
    }
    fn stroke(&mut self, r: R, rad: f32, bw: f32, c: Color) {
        let k = self.k;
        self.pm.stroke_rrect(
            self.ox + r.x * k,
            self.oy + r.y * k,
            r.w * k,
            r.h * k,
            rad * k,
            bw * k,
            c,
        );
    }
    fn circle(&mut self, cx: f32, cy: f32, r: f32, c: Color) {
        let k = self.k;
        self.pm
            .fill_circle(self.ox + cx * k, self.oy + cy * k, r * k, c);
    }
    fn ring(&mut self, cx: f32, cy: f32, r: f32, bw: f32, c: Color) {
        let k = self.k;
        self.pm
            .stroke_circle(self.ox + cx * k, self.oy + cy * k, r * k, bw * k, c);
    }
    fn width(&self, face: Face, size: f32, sp: f32, s: &str) -> f32 {
        self.f.width(face, size * self.k, sp * self.k, s) / self.k
    }
    fn text(
        &mut self,
        face: Face,
        size: f32,
        sp: f32,
        c: Color,
        x: f32,
        baseline: f32,
        s: &str,
    ) -> f32 {
        let k = self.k;
        self.pm.text(
            self.f,
            face,
            size * k,
            sp * k,
            c,
            self.ox + x * k,
            self.oy + baseline * k,
            s,
        ) / k
    }
    /// Texto centrado verticalmente em `cy` (como o `align-items:center` do CSS).
    fn text_mid(
        &mut self,
        face: Face,
        size: f32,
        sp: f32,
        c: Color,
        x: f32,
        cy: f32,
        s: &str,
    ) -> f32 {
        let (a, d) = self.f.metrics(face);
        self.text(face, size, sp, c, x, cy + (a + d) * size / 2.0, s)
    }
    /// Texto alinhado a direita em `right`, cortado com reticencias se passar de `max_w`.
    fn text_right(
        &mut self,
        face: Face,
        size: f32,
        c: Color,
        right: f32,
        cy: f32,
        s: &str,
        max_w: f32,
    ) {
        let s = self.f.fit(face, size * self.k, 0.0, s, max_w * self.k);
        let w = self.width(face, size, 0.0, &s);
        self.text_mid(face, size, 0.0, c, right - w, cy, &s);
    }
}

impl P<'_> {
    fn glow_rrect(&mut self, r: R, rad: f32, sigma: f32, c: Color) {
        let k = self.k;
        self.pm.glow_rrect(
            self.ox + r.x * k,
            self.oy + r.y * k,
            r.w * k,
            r.h * k,
            rad * k,
            sigma * k,
            c,
        );
    }
    fn text_c(&mut self, face: Face, size: f32, sp: f32, c: Color, cx: f32, cy: f32, s: &str) {
        let w = self.width(face, size, sp, s);
        self.text_mid(face, size, sp, c, cx - w / 2.0, cy, s);
    }
}

const X0: f32 = 32.0;
const Y0: f32 = 80.0;
const HERO: R = R {
    x: X0,
    y: Y0,
    w: 420.0,
    h: 372.0,
};
const SIDE: R = R {
    x: 468.0,
    y: Y0,
    w: 460.0,
    h: 372.0,
};
const SW_W: f32 = 112.0;
const SW_H: f32 = 58.0;
const SW_Y: f32 = 150.0;

/// Retangulo do interruptor, em coordenadas do design (para o clique do mouse).
pub fn switch_rect() -> (f32, f32, f32, f32) {
    (HERO.x + (HERO.w - SW_W) / 2.0, SW_Y, SW_W, SW_H)
}

/// Converte um ponto da janela (pixels) para o design 960x540.
pub fn to_design(w: f32, h: f32, px: f32, py: f32) -> (f32, f32) {
    let k = (w / DESIGN_W).min(h / DESIGN_H);
    (
        (px - (w - DESIGN_W * k) / 2.0) / k,
        (py - (h - DESIGN_H * k) / 2.0) / k,
    )
}

/// O ponto (design) esta em cima do interruptor?
pub fn hits_switch(x: f32, y: f32) -> bool {
    let (sx, sy, sw, sh) = switch_rect();
    x >= sx - 16.0 && x <= sx + sw + 16.0 && y >= sy - 16.0 && y <= sy + sh + 16.0
}

/// Tudo o que a tela mostra num quadro.
pub struct View<'a> {
    pub svcs: Svcs,
    /// (interface, IPv4) da rede ativa.
    pub net: Option<(String, String)>,
    pub progress: &'a Progress,
}

impl View<'_> {
    pub fn level(&self) -> Level {
        self.svcs.level()
    }
    pub fn busy(&self) -> bool {
        self.progress.running.is_some()
    }
}

/// Texto grande do estado.
pub fn headline(v: &View) -> &'static str {
    match v.progress.running {
        Some(Action::Enable) => "ENABLING",
        Some(Action::Disable) => "DISABLING",
        None => match v.level() {
            Level::On => "ON",
            Level::Partial => "PARTIAL",
            Level::Off => "OFF",
        },
    }
}

/// O que o botao A faz agora.
pub fn action_label(v: &View) -> &'static str {
    if v.busy() {
        "Working..."
    } else if v.level() == Level::On {
        "Turn off"
    } else {
        "Turn on"
    }
}

fn state_color(v: &View, accent: Color) -> Color {
    if v.busy() {
        return accent;
    }
    match v.level() {
        Level::On => OK,
        Level::Partial => WARN,
        Level::Off => MUTED,
    }
}

fn card(p: &mut P, r: R, title: &str, accent: Color) {
    p.rrect(r, 18.0, CARD);
    p.stroke(r, 18.0, 1.0, LINE);
    let cy = r.y + 24.0;
    p.circle(r.x + 21.5, cy, 3.5, accent);
    p.text_mid(Face::Sans800, 12.0, 1.92, MUTED, r.x + 32.0, cy, title);
}

fn keycap(p: &mut P, x: f32, cy: f32, key: &str, label: &str, dim: bool) -> f32 {
    let kw = (p.width(Face::Mono700, 11.0, 0.0, key) + 12.0).max(24.0);
    let r = R {
        x,
        y: cy - 12.0,
        w: kw,
        h: 24.0,
    };
    p.rrect(r, 7.0, KEYCAP_BG);
    p.stroke(r, 7.0, 1.0, KEYCAP_BORDER);
    p.text_c(Face::Mono700, 11.0, 0.0, TEXT, x + kw / 2.0, cy, key);
    let col = if dim { KEYCAP_BORDER } else { MUTED };
    let lw = p.text_mid(Face::Sans600, 13.0, 0.0, col, x + kw + 8.0, cy, label);
    x + kw + 8.0 + lw + 22.0
}

fn service_row(p: &mut P, y: f32, name: &str, detail: &str, on: bool, busy: bool) {
    let cy = y + 22.0;
    let x = SIDE.x + 18.0;
    let dot = if on { OK } else { TRACK };
    if on {
        p.glow_circle(x + 5.0, cy, 5.0, 6.0, OK.alpha(0.5));
    }
    p.circle(x + 5.0, cy, 5.0, dot);
    p.text_mid(Face::Sans700, 16.0, 0.0, TEXT, x + 22.0, cy - 7.0, name);
    p.text_mid(Face::Mono500, 12.0, 0.0, MUTED, x + 22.0, cy + 11.0, detail);
    let (txt, col) = if on {
        ("RUNNING", OK)
    } else if busy {
        ("...", MUTED)
    } else {
        ("STOPPED", MUTED)
    };
    let tw = p.width(Face::Mono700, 12.0, 0.6, txt);
    let chip = R {
        x: SIDE.x + SIDE.w - 18.0 - tw - 24.0,
        y: cy - 12.0,
        w: tw + 24.0,
        h: 24.0,
    };
    p.rrect(chip, 12.0, if on { OK.alpha(0.14) } else { KEYCAP_BG });
    p.text_c(
        Face::Mono700,
        12.0,
        0.6,
        col,
        chip.x + chip.w / 2.0,
        cy,
        txt,
    );
}

impl P<'_> {
    fn glow_circle(&mut self, cx: f32, cy: f32, r: f32, sigma: f32, c: Color) {
        let k = self.k;
        self.pm
            .glow_circle(self.ox + cx * k, self.oy + cy * k, r * k, sigma * k, c);
    }
}

/// Desenha o quadro completo em `pm` (tamanho livre; o design e centralizado e escalado).
pub fn render(pm: &mut Pixmap, fonts: &Fonts, v: &View, accent: Color) {
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let k = (w / DESIGN_W).min(h / DESIGN_H);
    let (ox, oy) = ((w - DESIGN_W * k) / 2.0, (h - DESIGN_H * k) / 2.0);
    pm.clear(BG);
    let mut p = P {
        pm,
        f: fonts,
        k,
        ox,
        oy,
    };
    let col = state_color(v, accent);

    // --- cabecalho ---
    let cy = 44.0;
    p.ring(41.0, cy, 6.5, 2.0, TEXT);
    p.ring(53.0, cy, 6.5, 2.0, accent);
    let w1 = p.text_mid(Face::Sans800, 24.0, -0.48, TEXT, 74.0, cy, "RG52");
    p.text_mid(
        Face::Sans500,
        24.0,
        -0.48,
        MUTED,
        74.0 + w1 + 6.0,
        cy,
        "REMOTE",
    );
    p.logo(480.0, cy);
    // selo do estado, no canto direito
    let tag = headline(v);
    let tw = p.width(Face::Sans800, 12.0, 1.68, tag);
    let pill = R {
        x: 928.0 - (tw + 46.0),
        y: cy - 16.0,
        w: tw + 46.0,
        h: 32.0,
    };
    p.stroke(pill, 16.0, 1.5, col);
    p.circle(pill.x + 18.0, cy, 4.0, col);
    p.text_mid(Face::Sans800, 12.0, 1.68, col, pill.x + 30.0, cy, tag);

    // --- cartao do interruptor ---
    card(&mut p, HERO, "REMOTE SERVICES", accent);
    let (sx, sy, sw, sh) = switch_rect();
    let track = R {
        x: sx,
        y: sy,
        w: sw,
        h: sh,
    };
    let (tcol, knob_x) = if v.busy() {
        (accent, sx + sw / 2.0)
    } else {
        match v.level() {
            Level::On => (OK, sx + sw - sh / 2.0),
            Level::Partial => (WARN, sx + sw / 2.0),
            Level::Off => (TRACK, sx + sh / 2.0),
        }
    };
    if tcol != TRACK {
        p.glow_rrect(track, sh / 2.0, 10.0, tcol.alpha(0.35));
    }
    p.rrect(track, sh / 2.0, tcol);
    p.circle(
        knob_x,
        sy + sh / 2.0 + 2.0,
        sh / 2.0 - 6.0,
        Color::hex(0x000000).alpha(0.25),
    );
    p.circle(knob_x, sy + sh / 2.0, sh / 2.0 - 6.0, Color::hex(0xF4F6FB));

    let mid = HERO.x + HERO.w / 2.0;
    p.text_c(Face::Sans800, 40.0, -0.4, col, mid, 252.0, headline(v));
    let sub = if v.busy() {
        "Please wait...".to_string()
    } else {
        match v.level() {
            Level::On => "SSH, Samba and File browser are running".to_string(),
            Level::Off => "Remote access is off".to_string(),
            Level::Partial => format!("{} of 3 services running", v.svcs.count()),
        }
    };
    p.text_c(Face::Sans600, 14.0, 0.0, MUTED, mid, 288.0, &sub);

    // andamento ou resultado
    let strip = R {
        x: HERO.x + 18.0,
        y: HERO.y + HERO.h - 78.0,
        w: HERO.w - 36.0,
        h: 60.0,
    };
    p.rrect(strip, 12.0, BG);
    p.stroke(strip, 12.0, 1.0, LINE);
    if v.busy() {
        let pr = v.progress;
        let lbl = if pr.total > 0 {
            format!("{}   {}/{}", pr.label, pr.step, pr.total)
        } else {
            "Starting...".to_string()
        };
        p.text_mid(
            Face::Sans700,
            14.0,
            0.0,
            TEXT,
            strip.x + 16.0,
            strip.y + 20.0,
            &lbl,
        );
        let t = R {
            x: strip.x + 16.0,
            y: strip.y + 38.0,
            w: strip.w - 32.0,
            h: 6.0,
        };
        p.rrect(t, 3.0, TRACK);
        let f = if pr.total > 0 {
            (pr.step as f32 - 0.5).max(0.0) / pr.total as f32
        } else {
            0.0
        };
        if f > 0.0 {
            p.rrect(
                R {
                    w: (t.w * f).max(6.0),
                    ..t
                },
                3.0,
                accent,
            );
        }
    } else if let Some(o) = &v.progress.outcome {
        let c = if o.ok { OK } else { BAD };
        p.circle(strip.x + 20.0, strip.y + strip.h / 2.0, 5.0, c);
        let max = strip.w - 54.0;
        let fit =
            p.f.fit(Face::Sans600, 13.0 * k, 0.0, &o.text, max * k * 2.0);
        // quebra em duas linhas quando nao cabe numa
        let (l1, l2) = split_two(&p, &fit, max);
        let ys = if l2.is_empty() {
            strip.y + 30.0
        } else {
            strip.y + 21.0
        };
        p.text_mid(Face::Sans600, 13.0, 0.0, TEXT, strip.x + 36.0, ys, &l1);
        if !l2.is_empty() {
            p.text_mid(
                Face::Sans600,
                13.0,
                0.0,
                TEXT,
                strip.x + 36.0,
                strip.y + 40.0,
                &l2,
            );
        }
    } else {
        p.text_mid(
            Face::Sans600,
            13.0,
            0.0,
            MUTED,
            strip.x + 16.0,
            strip.y + 20.0,
            "Changes last until the next reboot.",
        );
        p.text_mid(
            Face::Sans600,
            13.0,
            0.0,
            MUTED,
            strip.x + 16.0,
            strip.y + 40.0,
            "Needs a working Wi-Fi connection.",
        );
    }

    // --- cartao dos servicos ---
    card(&mut p, SIDE, "SERVICES", accent);
    let b = v.busy();
    service_row(&mut p, SIDE.y + 42.0, "SSH", "port 22", v.svcs.ssh, b);
    service_row(
        &mut p,
        SIDE.y + 90.0,
        "Samba",
        "SMB, port 445",
        v.svcs.smb,
        b,
    );
    service_row(
        &mut p,
        SIDE.y + 138.0,
        "File browser",
        "HTTP, port 80",
        v.svcs.files,
        b,
    );
    let sep = R {
        x: SIDE.x + 18.0,
        y: SIDE.y + 196.0,
        w: SIDE.w - 36.0,
        h: 1.0,
    };
    p.rrect(sep, 0.0, LINE);
    p.text_mid(
        Face::Sans800,
        11.0,
        1.7,
        MUTED,
        SIDE.x + 18.0,
        SIDE.y + 216.0,
        "ADDRESS",
    );
    match &v.net {
        Some((ifc, ip)) => {
            p.text_mid(
                Face::Mono700,
                34.0,
                0.0,
                TEXT,
                SIDE.x + 18.0,
                SIDE.y + 250.0,
                ip,
            );
            let lbl = format!("on {ifc}");
            p.text_right(
                Face::Sans600,
                13.0,
                MUTED,
                SIDE.x + SIDE.w - 18.0,
                SIDE.y + 250.0,
                &lbl,
                120.0,
            );
            let lines = [
                (format!("ssh ark@{ip}"), v.svcs.ssh),
                (format!("http://{ip}"), v.svcs.files),
                (format!("smb://{ip}"), v.svcs.smb),
            ];
            for (i, (t, on)) in lines.iter().enumerate() {
                let c = if *on { TEXT } else { KEYCAP_BORDER };
                p.text_mid(
                    Face::Mono500,
                    13.0,
                    0.0,
                    c,
                    SIDE.x + 18.0,
                    SIDE.y + 292.0 + i as f32 * 24.0,
                    t,
                );
            }
        }
        None => {
            p.text_mid(
                Face::Sans800,
                26.0,
                -0.3,
                WARN,
                SIDE.x + 18.0,
                SIDE.y + 250.0,
                "No network",
            );
            p.text_mid(
                Face::Sans600,
                13.0,
                0.0,
                MUTED,
                SIDE.x + 18.0,
                SIDE.y + 290.0,
                "Connect to Wi-Fi first (Options > Wifi).",
            );
        }
    }

    // --- rodape ---
    let fy = 514.0;
    let x = keycap(&mut p, X0, fy, "A", action_label(v), v.busy());
    keycap(&mut p, x, fy, "B", "Exit", false);
    let vw = p.width(Face::Mono500, 12.0, 0.0, VERSION);
    p.text_mid(Face::Mono500, 12.0, 0.0, MUTED, 928.0 - vw, fy, VERSION);
}

/// Divide `s` em ate duas linhas de largura `max` (quebra em espaco).
fn split_two(p: &P, s: &str, max: f32) -> (String, String) {
    if p.width(Face::Sans600, 13.0, 0.0, s) <= max {
        return (s.to_string(), String::new());
    }
    let mut first = String::new();
    let mut rest = String::new();
    for word in s.split_whitespace() {
        let cand = if first.is_empty() {
            word.to_string()
        } else {
            format!("{first} {word}")
        };
        if rest.is_empty() && p.width(Face::Sans600, 13.0, 0.0, &cand) <= max {
            first = cand;
        } else {
            if !rest.is_empty() {
                rest.push(' ');
            }
            rest.push_str(word);
        }
    }
    let rest = p.f.fit(Face::Sans600, 13.0 * p.k, 0.0, &rest, max * p.k);
    (first, rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::Outcome;

    fn view<'a>(s: Svcs, pr: &'a Progress, net: bool) -> View<'a> {
        View {
            svcs: s,
            net: net.then(|| ("wlan0".to_string(), "192.168.0.23".to_string())),
            progress: pr,
        }
    }

    fn shot(v: &View) -> Pixmap {
        let mut pm = Pixmap::new(960, 540);
        render(&mut pm, &Fonts::new(), v, ACCENT);
        pm
    }

    fn near(a: [u8; 4], b: Color, tol: i32) -> bool {
        (a[0] as i32 - b.r as i32).abs() <= tol
            && (a[1] as i32 - b.g as i32).abs() <= tol
            && (a[2] as i32 - b.b as i32).abs() <= tol
    }

    fn at(pm: &Pixmap, x: f32, y: f32) -> [u8; 4] {
        pm.get(x as usize, y as usize).unwrap()
    }

    #[test]
    fn interruptor_muda_de_cor_e_de_lado() {
        let idle = Progress::default();
        let (sx, sy, sw, sh) = switch_rect();
        let ymid = sy + sh / 2.0;
        // trilho (perto da borda esquerda e direita, fora do botao)
        let off = shot(&view(Svcs::default(), &idle, true));
        let on = shot(&view(Svcs::ALL, &idle, true));
        let left = (sx + 6.0 + sh / 2.0 - 6.0, ymid);
        assert!(
            near(at(&off, sx + sw - 12.0, ymid), TRACK, 4),
            "off: trilho"
        );
        assert!(
            near(at(&on, left.0 + 10.0, left.1), OK, 6),
            "on: trilho verde"
        );
        // botao: claro no lado direito quando ligado, no esquerdo quando desligado
        let knob_on = at(&on, sx + sw - sh / 2.0, ymid);
        let knob_off = at(&off, sx + sh / 2.0, ymid);
        assert!(knob_on[0] > 220, "{knob_on:?}");
        assert!(knob_off[0] > 220, "{knob_off:?}");
        assert!(
            at(&off, sx + sw - sh / 2.0, ymid)[0] < 100,
            "off: lado direito vazio"
        );
    }

    #[test]
    fn textos_do_estado() {
        let idle = Progress::default();
        assert_eq!(headline(&view(Svcs::ALL, &idle, true)), "ON");
        assert_eq!(headline(&view(Svcs::default(), &idle, true)), "OFF");
        let one = Svcs {
            ssh: true,
            ..Svcs::default()
        };
        assert_eq!(headline(&view(one, &idle, true)), "PARTIAL");
        assert_eq!(action_label(&view(Svcs::ALL, &idle, true)), "Turn off");
        assert_eq!(action_label(&view(Svcs::default(), &idle, true)), "Turn on");
        assert_eq!(action_label(&view(one, &idle, true)), "Turn on");
        let busy = Progress {
            running: Some(Action::Enable),
            step: 2,
            total: 5,
            label: "Time sync".into(),
            outcome: None,
        };
        assert_eq!(headline(&view(Svcs::default(), &busy, true)), "ENABLING");
        assert_eq!(
            action_label(&view(Svcs::default(), &busy, true)),
            "Working..."
        );
    }

    #[test]
    fn clique_no_interruptor() {
        let (sx, sy, sw, sh) = switch_rect();
        assert!(hits_switch(sx + sw / 2.0, sy + sh / 2.0));
        assert!(!hits_switch(20.0, 500.0));
        // janela maior que o design: o ponto central continua dentro
        let (x, y) = to_design(
            1280.0,
            720.0,
            1280.0 * (sx + sw / 2.0) / 960.0,
            720.0 * (sy + sh / 2.0) / 540.0,
        );
        assert!(hits_switch(x, y));
        // com barras laterais (janela larga) o design fica centralizado
        let (x, _) = to_design(1600.0, 540.0, 320.0 + 480.0, 270.0);
        assert!((x - 480.0).abs() < 0.01);
    }

    #[test]
    fn resultado_e_andamento_aparecem() {
        let ok = Progress {
            outcome: Some(Outcome {
                ok: true,
                text: "Remote Services enabled.".into(),
            }),
            ..Progress::default()
        };
        let bad = Progress {
            outcome: Some(Outcome {
                ok: false,
                text: "No network connection. Set up Wi-Fi in Options > Wifi first.".into(),
            }),
            ..Progress::default()
        };
        let strip_y = HERO.y + HERO.h - 78.0 + 30.0;
        let dot_x = HERO.x + 18.0 + 20.0;
        let a = shot(&view(Svcs::ALL, &ok, true));
        let b = shot(&view(Svcs::default(), &bad, false));
        assert!(near(at(&a, dot_x, strip_y), OK, 8));
        assert!(near(at(&b, dot_x, strip_y), BAD, 8));
    }

    #[test]
    fn sem_rede_mostra_aviso_e_nao_o_ip() {
        let idle = Progress::default();
        let a = shot(&view(Svcs::default(), &idle, false));
        let b = shot(&view(Svcs::default(), &idle, true));
        assert_ne!(a.data(), b.data());
    }

    #[test]
    fn captura_para_inspecao() {
        // RG52_REMOTE_DUMP=/caminho cargo test captura  -> grava 4 estados em PPM
        if let Ok(dir) = std::env::var("RG52_REMOTE_DUMP") {
            let idle = Progress::default();
            let busy = Progress {
                running: Some(Action::Enable),
                step: 3,
                total: 5,
                label: "Samba".into(),
                outcome: None,
            };
            let err = Progress {
                outcome: Some(Outcome {
                    ok: false,
                    text: "No network connection. Set up Wi-Fi in Options > Wifi first.".into(),
                }),
                ..Progress::default()
            };
            let part = Svcs {
                ssh: true,
                ..Svcs::default()
            };
            for (n, v) in [
                ("on", view(Svcs::ALL, &idle, true)),
                ("off", view(Svcs::default(), &idle, true)),
                ("busy", view(part, &busy, true)),
                ("err", view(Svcs::default(), &err, false)),
            ] {
                let mut pm = Pixmap::new(1280, 720);
                render(&mut pm, &Fonts::new(), &v, ACCENT);
                pm.write_ppm(std::path::Path::new(&format!("{dir}/remote_{n}.ppm")))
                    .unwrap();
            }
        }
    }
}
