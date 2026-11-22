//! remote-services: liga/desliga os Remote Services (SSH, Samba, filebrowser) numa tela so.
//! A = ligar/desligar, B/Esc/Q sai. Os comandos rodam com `sudo -n`, como nos scripts
//! originais do dArkOS (o usuario `ark` ja tem sudo sem senha).

use std::{
    env,
    path::PathBuf,
    process,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use remote_services::{
    devices::{self, screen, Feature},
    gfx::{Color, Fonts, Pixmap},
    remote::{self, Action, DryRunner, Progress, Runner, Shared, Svcs, SystemRunner},
    ui::{hits_switch_px, parse_accent, render, View, ACCENT},
    Level,
};
use sdl2::{
    controller::{Button, GameController},
    event::{Event, WindowEvent},
    keyboard::Keycode,
    mouse::MouseButton,
    pixels::PixelFormatEnum,
};

const HELP: &str = "\
remote-services - liga/desliga os Remote Services (SSH, Samba, filebrowser) no RG52 Mini

USO: remote-services [opcoes]
  --device ID       auto (padrao), rg52mini, rg43, r36s, rgb10, a10mini, rk3566 ou generic
  --fullscreen      tela cheia (use no aparelho)
  --no-gamepad      ignora o gamepad do SDL (use com o gptokeyb)
  --accent COR      amber (padrao), teal, violet, coral ou #RRGGBB
  --status          imprime on, partial ou off e sai (codigo 0 se algum servico roda)
  --enable          liga sem abrir a tela (imprime o resultado)
  --disable         desliga sem abrir a tela
  --dry-run         simula: nao executa nada, so mexe no estado da tela
  --no-network      (com --dry-run) simula Wi-Fi desconectado
  --screenshot PPM  (depuracao) desenha um quadro 1280x720 sem janela e sai
  -h, --help        esta ajuda
";

struct Args {
    device: Option<String>,
    fullscreen: bool,
    no_gamepad: bool,
    accent: Color,
    status: bool,
    act: Option<Action>,
    dry: bool,
    no_net: bool,
    screenshot: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        device: None,
        fullscreen: false,
        no_gamepad: false,
        accent: ACCENT,
        status: false,
        act: None,
        dry: false,
        no_net: false,
        screenshot: None,
    };
    let mut it = env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut val = |n: &str| it.next().ok_or_else(|| format!("{n} precisa de um valor"));
        match arg.as_str() {
            "--device" => a.device = Some(val("--device")?),
            "--fullscreen" => a.fullscreen = true,
            "--no-gamepad" => a.no_gamepad = true,
            "--accent" => a.accent = parse_accent(&val("--accent")?)?,
            "--status" => a.status = true,
            "--enable" => a.act = Some(Action::Enable),
            "--disable" => a.act = Some(Action::Disable),
            "--dry-run" => a.dry = true,
            "--no-network" => a.no_net = true,
            "--screenshot" => a.screenshot = Some(PathBuf::from(val("--screenshot")?)),
            "-h" | "--help" => {
                print!("{HELP}");
                process::exit(0);
            }
            o => return Err(format!("opcao desconhecida: {o}\n\n{HELP}")),
        }
    }
    Ok(a)
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            process::exit(2);
        }
    };
    if let Err(e) = run(args) {
        eprintln!("erro: {e}");
        process::exit(1);
    }
}

fn runner(args: &Args) -> Arc<dyn Runner> {
    if args.dry {
        Arc::new(DryRunner::new(!args.no_net))
    } else {
        Arc::new(SystemRunner::default())
    }
}

/// Atualiza o arquivo que acende o icone no EmulationStation (nao mexe na simulacao).
fn sync(args: &Args, s: &Svcs) {
    if !args.dry {
        let _ = remote::sync_flag(std::path::Path::new(remote::FLAG_FILE), s);
    }
}

fn level_word(l: Level) -> &'static str {
    match l {
        Level::On => "on",
        Level::Partial => "partial",
        Level::Off => "off",
    }
}

/// O que a tela precisa para decidir se redesenha.
#[derive(Clone, PartialEq)]
struct Snap {
    svcs: Svcs,
    net: Option<(String, String)>,
    progress: Progress,
}

fn is_exit_key(k: Keycode) -> bool {
    matches!(k, Keycode::Escape | Keycode::Backspace | Keycode::Q)
}

fn is_toggle_key(k: Keycode) -> bool {
    matches!(k, Keycode::Return | Keycode::KpEnter | Keycode::Space)
}

const INCOMPATIBLE: &str = "dArkOS was not detected on this device.";

fn run(args: Args) -> Result<(), String> {
    // Antes de qualquer `sudo systemctl`: fora do dArkOS o app nao deve mexer em servicos.
    let detected = devices::resolve(args.device.as_deref())?;
    let simulated = args.dry && args.device.is_none();
    if !detected.supports(Feature::RemoteServices) && !simulated && !args.status {
        eprintln!("device incompatible: {}", detected.label());
        if args.act.is_some() {
            process::exit(1);
        }
        if let Some(path) = &args.screenshot {
            let mut pm = Pixmap::new(1280, 720);
            let fonts = Fonts::new();
            screen::render(
                &mut pm,
                &fonts,
                "Remote Services",
                &detected,
                INCOMPATIBLE,
                args.accent,
            );
            return pm.write_ppm(path).map_err(|e| e.to_string());
        }
        return screen::show(
            "Remote Services",
            &detected,
            INCOMPATIBLE,
            args.accent,
            args.fullscreen,
            args.no_gamepad,
        );
    }
    let title = detected.profile.title;
    let rn = runner(&args);

    if args.status {
        let s = rn.probe();
        sync(&args, &s);
        let l = s.level();
        println!("{}", level_word(l));
        process::exit(if l == Level::Off { 1 } else { 0 });
    }
    if let Some(act) = args.act {
        let p: Shared = Arc::default();
        remote::perform(&*rn, act, &p, Duration::from_secs(5));
        let o = p.lock().map_err(|e| e.to_string())?.outcome.clone();
        let o = o.ok_or("sem resultado")?;
        sync(&args, &rn.probe());
        println!("{}", o.text);
        process::exit(if o.ok { 0 } else { 1 });
    }

    let fonts = Fonts::new();
    let progress: Shared = Arc::new(Mutex::new(Progress::default()));
    let snap = |rn: &Arc<dyn Runner>, progress: &Shared| Snap {
        svcs: rn.probe(),
        net: remote::primary_ipv4(),
        progress: progress.lock().map(|p| p.clone()).unwrap_or_default(),
    };
    let draw = |pm: &mut Pixmap, s: &Snap| {
        let v = View {
            svcs: s.svcs,
            net: s.net.clone(),
            progress: &s.progress,
        };
        render(pm, &fonts, &v, args.accent, title);
    };

    if let Some(path) = &args.screenshot {
        let mut pm = Pixmap::new(1280, 720);
        draw(&mut pm, &snap(&rn, &progress));
        return pm.write_ppm(path).map_err(|e| e.to_string());
    }

    let sdl = sdl2::init()?;
    let video = sdl.video()?;
    let gcs = sdl.game_controller()?;
    let mut pads: Vec<GameController> = Vec::new();
    if !args.no_gamepad {
        for i in 0..gcs.num_joysticks()? {
            if gcs.is_game_controller(i) {
                if let Ok(p) = gcs.open(i) {
                    pads.push(p);
                }
            }
        }
    }
    let mut wb = video.window(&format!("{title} REMOTE"), 960, 540);
    wb.position_centered().resizable();
    if args.fullscreen {
        wb.fullscreen_desktop();
    }
    let window = wb.build().map_err(|e| e.to_string())?;
    let mut canvas = window
        .into_canvas()
        .present_vsync()
        .build()
        .map_err(|e| e.to_string())?;
    if args.fullscreen {
        sdl.mouse().show_cursor(false);
    }
    let tc = canvas.texture_creator();
    let mut pump = sdl.event_pump()?;
    let (mut w, mut h) = canvas.output_size()?;
    let mut pm = Pixmap::new(w as usize, h as usize);
    let mut tex = tc
        .create_texture_streaming(PixelFormatEnum::ARGB8888, w, h)
        .map_err(|e| e.to_string())?;

    let every = Duration::from_millis(400);
    let mut last_poll = Instant::now() - every;
    let mut shown: Option<Snap> = None;
    let mut cur = snap(&rn, &progress);
    let mut force = true;
    let mut quit = false;

    let toggle = |cur: &Snap| {
        if cur.progress.running.is_some() {
            return;
        }
        let act = if cur.svcs.level() == Level::On {
            Action::Disable
        } else {
            Action::Enable
        };
        // a thread termina sozinha; o resultado chega pelo estado compartilhado
        remote::spawn(rn.clone(), act, progress.clone());
    };

    while !quit {
        for ev in pump.poll_iter() {
            match ev {
                Event::Quit { .. } => quit = true,
                Event::KeyDown {
                    keycode: Some(k), ..
                } if is_exit_key(k) => {
                    if cur.progress.running.is_none() {
                        quit = true;
                    }
                }
                Event::KeyDown {
                    keycode: Some(k),
                    repeat: false,
                    ..
                } if is_toggle_key(k) => toggle(&cur),
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Left,
                    x,
                    y,
                    ..
                } => {
                    if hits_switch_px(w as f32, h as f32, x as f32, y as f32) {
                        toggle(&cur);
                    }
                }
                Event::ControllerButtonDown { button, .. } if !args.no_gamepad => match button {
                    Button::A => toggle(&cur),
                    Button::B | Button::Start | Button::Back if cur.progress.running.is_none() => {
                        quit = true
                    }
                    _ => {}
                },
                Event::ControllerDeviceAdded { which, .. } if !args.no_gamepad => {
                    if let Ok(p) = gcs.open(which) {
                        pads.push(p);
                    }
                }
                Event::ControllerDeviceRemoved { which, .. } => {
                    pads.retain(|p| p.instance_id() != which)
                }
                Event::Window {
                    win_event:
                        WindowEvent::SizeChanged(..)
                        | WindowEvent::Exposed
                        | WindowEvent::Restored
                        | WindowEvent::Shown,
                    ..
                } => force = true,
                _ => {}
            }
        }
        let (nw, nh) = canvas.output_size()?;
        if (nw, nh) != (w, h) {
            (w, h) = (nw, nh);
            pm = Pixmap::new(w as usize, h as usize);
            tex = tc
                .create_texture_streaming(PixelFormatEnum::ARGB8888, w, h)
                .map_err(|e| e.to_string())?;
            force = true;
        }
        // Estado: a cada 400 ms (ou na hora, enquanto uma acao roda, para a barra andar).
        let busy = cur.progress.running.is_some();
        if last_poll.elapsed() >= every || busy {
            last_poll = Instant::now();
            cur = snap(&rn, &progress);
        }
        if shown.as_ref().map(|s| s.svcs) != Some(cur.svcs) {
            sync(&args, &cur.svcs);
        }
        if force || shown.as_ref() != Some(&cur) {
            draw(&mut pm, &cur);
            tex.update(None, pm.data(), pm.pitch())
                .map_err(|e| e.to_string())?;
            canvas.copy(&tex, None, None)?;
            canvas.present();
            shown = Some(cur.clone());
            force = false;
        } else {
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    Ok(())
}
