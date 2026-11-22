//! Tela do app: o design 960x540 (tema escuro, Manrope + JetBrains Mono), no mesmo
#![allow(clippy::too_many_arguments)]
//! visual de led-rgb / system-info / controller-test. Um interruptor grande liga e desliga os
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

impl P<'_> {
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

/// Posicoes para a proporcao da tela: larga (16:9) ou compacta (4:3, 3:2).
#[derive(Clone, Copy)]
struct Geo {
    w: f32,
    h: f32,
    compact: bool,
    hero: R,
    side: R,
    sw: (f32, f32, f32, f32),
    inset: f32,
    x0: f32,
    right: f32,
    hdr_cy: f32,
    head_y: f32,
    head_size: f32,
    sub_y: f32,
    sub_size: f32,
    small: f32,
    rows: [f32; 3],
    sep: f32,
    addr: f32,
    ip: f32,
    ip_size: f32,
    cmd: f32,
    cmd_pitch: f32,
    foot: f32,
}

impl Geo {
    fn new(px_w: f32, px_h: f32) -> Geo {
        let aspect = px_w / px_h;
        if aspect >= 1.6 {
            return Geo {
                w: DESIGN_W,
                h: DESIGN_H,
                compact: false,
                hero: HERO,
                side: SIDE,
                sw: switch_rect(),
                inset: 18.0,
                x0: X0,
                right: 928.0,
                hdr_cy: 44.0,
                head_y: 252.0,
                head_size: 40.0,
                sub_y: 288.0,
                sub_size: 14.0,
                small: 13.0,
                rows: [42.0, 90.0, 138.0],
                sep: 196.0,
                addr: 216.0,
                ip: 250.0,
                ip_size: 34.0,
                cmd: 292.0,
                cmd_pitch: 24.0,
                foot: 514.0,
            };
        }
        let (w, h) = (640.0, 640.0 / aspect);
        let y = 38.0;
        let body = h - y - 32.0;
        let hero = R {
            x: 8.0,
            y,
            w: 236.0,
            h: body,
        };
        let (sw_w, sw_h) = (88.0, 44.0);
        let sw_y = y + 40.0;
        Geo {
            w,
            h,
            compact: true,
            hero,
            side: R {
                x: 252.0,
                y,
                w: 380.0,
                h: body,
            },
            sw: (hero.x + (hero.w - sw_w) / 2.0, sw_y, sw_w, sw_h),
            inset: 10.0,
            x0: 10.0,
            right: 630.0,
            hdr_cy: 20.0,
            head_y: sw_y + sw_h + 34.0,
            head_size: 28.0,
            sub_y: sw_y + sw_h + 60.0,
            sub_size: 11.0,
            small: 11.0,
            rows: [34.0, 72.0, 110.0],
            sep: 156.0,
            addr: 172.0,
            ip: 200.0,
            ip_size: 26.0,
            cmd: 234.0,
            cmd_pitch: 20.0,
            foot: h - 15.0,
        }
    }
}

/// O ponto (pixels de uma tela `w` x `h`) esta em cima do interruptor?
pub fn hits_switch_px(w: f32, h: f32, px: f32, py: f32) -> bool {
    let g = Geo::new(w.max(1.0), h.max(1.0));
    let k = (w / g.w).min(h / g.h);
    let (x, y) = (
        (px - (w - g.w * k) / 2.0) / k,
        (py - (h - g.h * k) / 2.0) / k,
    );
    let (sx, sy, sw, sh) = g.sw;
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

fn service_row(p: &mut P, g: &Geo, row: usize, name: &str, detail: &str, on: bool, busy: bool) {
    let y = g.side.y + g.rows[row];
    let cy = y + 22.0;
    let x = g.side.x + g.inset;
    let dot = if on { OK } else { TRACK };
    if on {
        p.glow_circle(x + 5.0, cy, 5.0, 6.0, OK.alpha(0.5));
    }
    p.circle(x + 5.0, cy, 5.0, dot);
    let (nf, df) = if g.compact {
        (14.0, 10.5)
    } else {
        (16.0, 12.0)
    };
    p.text_mid(Face::Sans700, nf, 0.0, TEXT, x + 22.0, cy - 7.0, name);
    p.text_mid(Face::Mono500, df, 0.0, MUTED, x + 22.0, cy + 10.0, detail);
    let (txt, col) = if on {
        ("RUNNING", OK)
    } else if busy {
        ("...", MUTED)
    } else {
        ("STOPPED", MUTED)
    };
    let tw = p.width(Face::Mono700, 12.0, 0.6, txt);
    let chip = R {
        x: g.side.x + g.side.w - g.inset - tw - 24.0,
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
pub fn render(pm: &mut Pixmap, fonts: &Fonts, v: &View, accent: Color, title: &str) {
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let g = Geo::new(w.max(1.0), h.max(1.0));
    let k = (w / g.w).min(h / g.h);
    let (ox, oy) = ((w - g.w * k) / 2.0, (h - g.h * k) / 2.0);
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
    let cy = g.hdr_cy;
    crate::header::draw(
        p.pm,
        p.f,
        p.k,
        p.ox,
        p.oy,
        &crate::header::Header {
            left: g.x0,
            center: g.w / 2.0,
            cy: cy,
            compact: g.compact,
            device: title,
            app: "REMOTE",
        },
    );
    // selo do estado, no canto direito
    let tag = headline(v);
    let tw = p.width(Face::Sans800, 12.0, 1.68, tag);
    let ph = if g.compact { 26.0 } else { 32.0 };
    let pill = R {
        x: g.right - (tw + 46.0),
        y: cy - ph / 2.0,
        w: tw + 46.0,
        h: ph,
    };
    p.stroke(pill, ph / 2.0, 1.5, col);
    p.circle(pill.x + 18.0, cy, 4.0, col);
    p.text_mid(Face::Sans800, 12.0, 1.68, col, pill.x + 30.0, cy, tag);

    // --- cartao do interruptor ---
    card(&mut p, g.hero, "REMOTE SERVICES", accent);
    let (sx, sy, sw, sh) = g.sw;
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

    let mid = g.hero.x + g.hero.w / 2.0;
    p.text_c(
        Face::Sans800,
        g.head_size,
        -0.4,
        col,
        mid,
        g.head_y,
        headline(v),
    );
    let sub = if v.busy() {
        "Please wait...".to_string()
    } else {
        match v.level() {
            Level::On => "SSH, Samba and File browser are running".to_string(),
            Level::Off => "Remote access is off".to_string(),
            Level::Partial => format!("{} of 3 services running", v.svcs.count()),
        }
    };
    p.text_c(Face::Sans600, g.sub_size, 0.0, MUTED, mid, g.sub_y, &sub);

    // andamento ou resultado
    let strip = R {
        x: g.hero.x + g.inset,
        y: g.hero.y + g.hero.h - 60.0 - g.inset,
        w: g.hero.w - 2.0 * g.inset,
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
            g.small + 1.0,
            0.0,
            TEXT,
            strip.x + 12.0,
            strip.y + 20.0,
            &lbl,
        );
        let t = R {
            x: strip.x + 12.0,
            y: strip.y + 38.0,
            w: strip.w - 24.0,
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
        let max = strip.w - 46.0;
        let fit =
            p.f.fit(Face::Sans600, g.small * k, 0.0, &o.text, max * k * 2.0);
        // quebra em duas linhas quando nao cabe numa
        let (l1, l2) = split_two(&p, &fit, max, g.small);
        let ys = if l2.is_empty() {
            strip.y + 30.0
        } else {
            strip.y + 21.0
        };
        p.text_mid(Face::Sans600, g.small, 0.0, TEXT, strip.x + 36.0, ys, &l1);
        if !l2.is_empty() {
            p.text_mid(
                Face::Sans600,
                g.small,
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
            g.small,
            0.0,
            MUTED,
            strip.x + 12.0,
            strip.y + 20.0,
            "Changes last until the next reboot.",
        );
        p.text_mid(
            Face::Sans600,
            g.small,
            0.0,
            MUTED,
            strip.x + 12.0,
            strip.y + 40.0,
            "Needs a working Wi-Fi connection.",
        );
    }

    // --- cartao dos servicos ---
    card(&mut p, g.side, "SERVICES", accent);
    let b = v.busy();
    service_row(&mut p, &g, 0, "SSH", "port 22", v.svcs.ssh, b);
    service_row(&mut p, &g, 1, "Samba", "SMB, port 445", v.svcs.smb, b);
    service_row(
        &mut p,
        &g,
        2,
        "File browser",
        "HTTP, port 80",
        v.svcs.files,
        b,
    );
    let sep = R {
        x: g.side.x + g.inset,
        y: g.side.y + g.sep,
        w: g.side.w - 2.0 * g.inset,
        h: 1.0,
    };
    p.rrect(sep, 0.0, LINE);
    p.text_mid(
        Face::Sans800,
        11.0,
        1.7,
        MUTED,
        g.side.x + g.inset,
        g.side.y + g.addr,
        "ADDRESS",
    );
    match &v.net {
        Some((ifc, ip)) => {
            p.text_mid(
                Face::Mono700,
                g.ip_size,
                0.0,
                TEXT,
                g.side.x + g.inset,
                g.side.y + g.ip,
                ip,
            );
            let lbl = format!("on {ifc}");
            p.text_right(
                Face::Sans600,
                13.0,
                MUTED,
                g.side.x + g.side.w - g.inset,
                g.side.y + g.ip,
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
                    g.small + if g.compact { 1.0 } else { 0.0 },
                    0.0,
                    c,
                    g.side.x + g.inset,
                    g.side.y + g.cmd + i as f32 * g.cmd_pitch,
                    t,
                );
            }
        }
        None => {
            p.text_mid(
                Face::Sans800,
                g.ip_size - if g.compact { 4.0 } else { 8.0 },
                -0.3,
                WARN,
                g.side.x + g.inset,
                g.side.y + g.ip,
                "No network",
            );
            p.text_mid(
                Face::Sans600,
                g.small + 1.0,
                0.0,
                MUTED,
                g.side.x + g.inset,
                g.side.y + g.cmd,
                "Connect to Wi-Fi first (Options > Wifi).",
            );
        }
    }

    // --- rodape ---
    let fy = g.foot;
    let x = keycap(&mut p, g.x0, fy, "A", action_label(v), v.busy());
    keycap(&mut p, x, fy, "B", "Exit", false);
}

/// Divide `s` em ate duas linhas de largura `max` (quebra em espaco).
fn split_two(p: &P, s: &str, max: f32, size: f32) -> (String, String) {
    if p.width(Face::Sans600, size, 0.0, s) <= max {
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
        if rest.is_empty() && p.width(Face::Sans600, size, 0.0, &cand) <= max {
            first = cand;
        } else {
            if !rest.is_empty() {
                rest.push(' ');
            }
            rest.push_str(word);
        }
    }
    let rest = p.f.fit(Face::Sans600, size * p.k, 0.0, &rest, max * p.k);
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
        render(&mut pm, &Fonts::new(), v, ACCENT, "RG52");
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
    fn telas_compactas_cabem_e_o_clique_acompanha() {
        for (w, h) in [
            (640.0f32, 480.0f32),
            (480.0, 320.0),
            (1024.0, 768.0),
            (720.0, 720.0),
        ] {
            let g = Geo::new(w, h);
            assert!(g.compact, "{w}x{h}");
            assert!(g.hero.x + g.hero.w <= g.side.x, "{w}x{h}");
            assert!(g.side.x + g.side.w <= g.w, "{w}x{h}");
            let last = g.cmd + 2.0 * g.cmd_pitch + 10.0;
            assert!(last <= g.side.h, "{w}x{h}: comandos passam do cartao");
            assert!(g.hero.y + g.hero.h <= g.foot - 12.0, "{w}x{h}");
            assert!(
                g.sub_y + 8.0 <= g.hero.h + g.hero.y - 60.0 - g.inset,
                "{w}x{h}: texto encosta na faixa"
            );
            let k = (w / g.w).min(h / g.h);
            let (ox, oy) = ((w - g.w * k) / 2.0, (h - g.h * k) / 2.0);
            let (sx, sy, sw, sh) = g.sw;
            assert!(hits_switch_px(
                w,
                h,
                ox + (sx + sw / 2.0) * k,
                oy + (sy + sh / 2.0) * k
            ));
            assert!(!hits_switch_px(w, h, w - 2.0, h - 2.0));
            let idle = Progress::default();
            let mut pm = Pixmap::new(w as usize, h as usize);
            render(
                &mut pm,
                &Fonts::new(),
                &view(Svcs::ALL, &idle, true),
                ACCENT,
                "R36S",
            );
        }
    }

    #[test]
    fn captura_para_inspecao() {
        // REMOTE_SERVICES_DUMP=/caminho cargo test captura  -> grava 4 estados em PPM
        if let Ok(dir) = std::env::var("REMOTE_SERVICES_DUMP") {
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
                render(&mut pm, &Fonts::new(), &v, ACCENT, "RG52");
                pm.write_ppm(std::path::Path::new(&format!("{dir}/remote_{n}.ppm")))
                    .unwrap();
            }
        }
    }
}
