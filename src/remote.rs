//! Estado e acoes dos Remote Services.
//!
//! Reproduz o que `Enable Remote Services.sh` e `Disable Remote Services.sh` do dArkOS
//! fazem (smbd, nmbd, ssh, filebrowser na porta 80, NTP e NetworkManager-wait-online),
//! mas sem tela de console: o estado vem de `/proc` (sem root) e as acoes viram uma
//! lista de passos `sudo -n ...`, executados em outra thread para a tela nao travar.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

/// Quais servicos estao rodando agora.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Svcs {
    pub ssh: bool,
    pub smb: bool,
    pub files: bool,
}

/// Estado geral: tudo ligado, nada ligado ou so parte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    On,
    Partial,
    Off,
}

impl Svcs {
    pub const ALL: Svcs = Svcs {
        ssh: true,
        smb: true,
        files: true,
    };

    pub fn count(&self) -> usize {
        self.ssh as usize + self.smb as usize + self.files as usize
    }

    pub fn level(&self) -> Level {
        match self.count() {
            3 => Level::On,
            0 => Level::Off,
            _ => Level::Partial,
        }
    }
}

/// Servicos que um passo liga ou desliga (usado so pelo modo de simulacao).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Svc {
    Ssh,
    Smb,
    Files,
}

/// Um passo da acao: o que mostrar e o comando a rodar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub label: &'static str,
    /// Linhas de comando; o passo falha se qualquer uma sair com erro.
    pub cmds: Vec<Vec<String>>,
    /// O comando fica rodando em segundo plano (filebrowser): nao esperamos ele terminar.
    pub detach: bool,
    /// Falha neste passo nao interrompe os seguintes (ex.: `pkill` sem nada para matar).
    pub optional: bool,
    pub effect: Option<(Svc, bool)>,
}

fn sudo(args: &[&str]) -> Vec<String> {
    let mut v = vec!["sudo".to_string(), "-n".to_string()];
    v.extend(args.iter().map(|s| s.to_string()));
    v
}

/// Os passos de `Enable Remote Services.sh`.
pub fn enable_plan() -> Vec<Step> {
    vec![
        Step {
            label: "Network wait",
            cmds: vec![
                sudo(&["systemctl", "enable", "NetworkManager-wait-online"]),
                sudo(&["systemctl", "start", "NetworkManager-wait-online"]),
            ],
            detach: false,
            optional: true,
            effect: None,
        },
        Step {
            label: "Time sync",
            cmds: vec![sudo(&["timedatectl", "set-ntp", "1"])],
            detach: false,
            optional: true,
            effect: None,
        },
        Step {
            label: "Samba",
            cmds: vec![
                sudo(&["systemctl", "start", "smbd"]),
                sudo(&["systemctl", "start", "nmbd"]),
            ],
            detach: false,
            optional: false,
            effect: Some((Svc::Smb, true)),
        },
        Step {
            label: "SSH",
            cmds: vec![sudo(&["systemctl", "start", "ssh.service"])],
            detach: false,
            optional: false,
            effect: Some((Svc::Ssh, true)),
        },
        Step {
            label: "File browser",
            cmds: vec![sudo(&[
                "filebrowser",
                "-a",
                "0.0.0.0",
                "-p",
                "80",
                "-d",
                "/home/ark/.config/filebrowser.db",
                "-r",
                "/",
            ])],
            detach: true,
            optional: false,
            effect: Some((Svc::Files, true)),
        },
    ]
}

/// Os passos de `Disable Remote Services.sh`.
pub fn disable_plan() -> Vec<Step> {
    vec![
        Step {
            label: "File browser",
            cmds: vec![sudo(&["pkill", "filebrowser"])],
            detach: false,
            optional: true,
            effect: Some((Svc::Files, false)),
        },
        Step {
            label: "SSH",
            cmds: vec![sudo(&["systemctl", "stop", "ssh.service"])],
            detach: false,
            optional: false,
            effect: Some((Svc::Ssh, false)),
        },
        Step {
            label: "Samba",
            cmds: vec![
                sudo(&["systemctl", "stop", "smbd"]),
                sudo(&["systemctl", "stop", "nmbd"]),
            ],
            detach: false,
            optional: false,
            effect: Some((Svc::Smb, false)),
        },
        Step {
            label: "Time sync",
            cmds: vec![sudo(&["timedatectl", "set-ntp", "0"])],
            detach: false,
            optional: true,
            effect: None,
        },
        Step {
            label: "Network wait",
            cmds: vec![
                sudo(&["systemctl", "disable", "NetworkManager-wait-online"]),
                sudo(&["systemctl", "stop", "NetworkManager-wait-online"]),
            ],
            detach: false,
            optional: true,
            effect: None,
        },
    ]
}

// --- leitura do estado (sem root) -------------------------------------------------------

/// Procura os processos dos servicos em `<proc>/<pid>/comm`.
pub fn scan(proc_dir: &Path) -> Svcs {
    let mut s = Svcs::default();
    let Ok(rd) = fs::read_dir(proc_dir) else {
        return s;
    };
    for e in rd.flatten() {
        let n = e.file_name();
        let n = n.to_string_lossy();
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(comm) = fs::read_to_string(e.path().join("comm")) else {
            continue;
        };
        match comm.trim() {
            "sshd" => s.ssh = true,
            // o Samba so serve de verdade com o smbd; o nmbd e so o nome NetBIOS
            "smbd" => s.smb = true,
            "filebrowser" => s.files = true,
            _ => {}
        }
    }
    s
}

/// Ha rota padrao em `/proc/net/route`? (o mesmo teste de `ip route | grep default`)
pub fn has_default_route(route: &str) -> bool {
    route.lines().skip(1).any(|l| {
        let mut c = l.split_whitespace();
        let _iface = c.next();
        c.next() == Some("00000000")
    })
}

/// Primeira interface IPv4 ativa que nao seja loopback: (nome, endereco).
pub fn primary_ipv4() -> Option<(String, String)> {
    // SAFETY: getifaddrs preenche uma lista encadeada que liberamos com freeifaddrs;
    // so lemos campos enquanto ela existe.
    unsafe {
        let mut ifa: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifa) != 0 {
            return None;
        }
        let mut found = None;
        let mut cur = ifa;
        while !cur.is_null() && found.is_none() {
            let a = &*cur;
            let up = a.ifa_flags & libc::IFF_UP as u32 != 0
                && a.ifa_flags & libc::IFF_LOOPBACK as u32 == 0;
            if up && !a.ifa_addr.is_null() && (*a.ifa_addr).sa_family as i32 == libc::AF_INET {
                let sin = &*(a.ifa_addr as *const libc::sockaddr_in);
                let ip = u32::from_be(sin.sin_addr.s_addr);
                let name = std::ffi::CStr::from_ptr(a.ifa_name)
                    .to_string_lossy()
                    .into_owned();
                found = Some((
                    name,
                    format!(
                        "{}.{}.{}.{}",
                        ip >> 24,
                        ip >> 16 & 255,
                        ip >> 8 & 255,
                        ip & 255
                    ),
                ));
            }
            cur = a.ifa_next;
        }
        libc::freeifaddrs(ifa);
        found
    }
}

/// Arquivo que indica "Remote Services ligados": o EmulationStation olha para ele para
/// mostrar o icone na barra de cima. Fica em /tmp, que o boot limpa, e os servicos
/// tambem voltam desligados no boot (os scripts originais so dao `start`).
pub const FLAG_FILE: &str = "/tmp/rg52-remote.on";

/// Cria o arquivo se algum servico estiver rodando e apaga se nenhum estiver.
pub fn sync_flag(path: &Path, s: &Svcs) -> std::io::Result<()> {
    if s.count() > 0 {
        fs::write(path, b"on\n")
    } else {
        match fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

// --- execucao ----------------------------------------------------------------------------

/// Quem executa os passos: o sistema de verdade ou a simulacao (`--dry-run`).
pub trait Runner: Send + Sync {
    fn run(&self, step: &Step) -> Result<(), String>;
    /// Servicos rodando agora.
    fn probe(&self) -> Svcs;
    fn has_network(&self) -> bool;
}

/// Executa `sudo -n ...` de verdade.
pub struct SystemRunner {
    pub proc_dir: PathBuf,
    pub route_file: PathBuf,
}

impl Default for SystemRunner {
    fn default() -> Self {
        SystemRunner {
            proc_dir: "/proc".into(),
            route_file: "/proc/net/route".into(),
        }
    }
}

fn last_line(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

impl Runner for SystemRunner {
    fn run(&self, step: &Step) -> Result<(), String> {
        for argv in &step.cmds {
            let mut c = Command::new(&argv[0]);
            c.args(&argv[1..]).stdin(Stdio::null());
            if step.detach {
                c.stdout(Stdio::null()).stderr(Stdio::null());
                // Fica vivo depois que o app fecha: novo grupo de processos, sem terminal.
                use std::os::unix::process::CommandExt;
                c.process_group(0);
                c.spawn().map_err(|e| format!("{}: {e}", argv[0]))?;
            } else {
                let o = c.output().map_err(|e| format!("{}: {e}", argv[0]))?;
                if !o.status.success() {
                    let why = last_line(&o.stderr);
                    return Err(if why.is_empty() {
                        format!("{} failed ({})", argv[2.min(argv.len() - 1)], o.status)
                    } else {
                        why
                    });
                }
            }
        }
        Ok(())
    }

    fn probe(&self) -> Svcs {
        scan(&self.proc_dir)
    }

    fn has_network(&self) -> bool {
        fs::read_to_string(&self.route_file)
            .map(|t| has_default_route(&t))
            .unwrap_or(false)
    }
}

/// Simulacao para testar a tela no PC: nada e executado, o estado e so uma variavel.
pub struct DryRunner {
    pub state: Arc<Mutex<Svcs>>,
    pub network: bool,
    pub delay: Duration,
}

impl DryRunner {
    pub fn new(network: bool) -> DryRunner {
        DryRunner {
            state: Arc::new(Mutex::new(Svcs::default())),
            network,
            delay: Duration::from_millis(450),
        }
    }
}

impl Runner for DryRunner {
    fn run(&self, step: &Step) -> Result<(), String> {
        thread::sleep(self.delay);
        if let Some((svc, on)) = step.effect {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            match svc {
                Svc::Ssh => s.ssh = on,
                Svc::Smb => s.smb = on,
                Svc::Files => s.files = on,
            }
        }
        Ok(())
    }

    fn probe(&self) -> Svcs {
        *self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn has_network(&self) -> bool {
        self.network
    }
}

// --- acao em segundo plano ---------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Enable,
    Disable,
}

/// Mensagem de resultado e se foi um problema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub ok: bool,
    pub text: String,
}

/// Andamento da acao, lido pela tela.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    pub running: Option<Action>,
    pub step: usize,
    pub total: usize,
    pub label: String,
    pub outcome: Option<Outcome>,
}

pub type Shared = Arc<Mutex<Progress>>;

fn set(p: &Shared, f: impl FnOnce(&mut Progress)) {
    f(&mut p.lock().unwrap_or_else(|e| e.into_inner()));
}

/// Executa a acao de forma sincrona (a thread de `spawn` so chama isto).
pub fn perform(r: &dyn Runner, act: Action, p: &Shared, settle: Duration) {
    let plan = match act {
        Action::Enable => enable_plan(),
        Action::Disable => disable_plan(),
    };
    if act == Action::Enable && !r.has_network() {
        set(p, |p| {
            *p = Progress {
                outcome: Some(Outcome {
                    ok: false,
                    text: "No network connection. Set up Wi-Fi in Options > Wifi first.".into(),
                }),
                ..Progress::default()
            }
        });
        return;
    }
    let total = plan.len();
    let mut warnings: Vec<String> = Vec::new();
    for (i, st) in plan.iter().enumerate() {
        set(p, |p| {
            p.running = Some(act);
            p.step = i + 1;
            p.total = total;
            p.label = st.label.to_string();
            p.outcome = None;
        });
        if let Err(e) = r.run(st) {
            if st.optional {
                warnings.push(format!("{}: {e}", st.label));
            } else {
                set(p, |p| {
                    *p = Progress {
                        outcome: Some(Outcome {
                            ok: false,
                            text: format!("{} failed: {e}", st.label),
                        }),
                        ..Progress::default()
                    }
                });
                return;
            }
        }
    }
    // O filebrowser e o ssh levam um instante para aparecer: espera o estado esperado.
    let want = match act {
        Action::Enable => Svcs::ALL,
        Action::Disable => Svcs::default(),
    };
    let t0 = Instant::now();
    while r.probe() != want && t0.elapsed() < settle {
        thread::sleep(Duration::from_millis(100));
    }
    let now = r.probe();
    let text = match (act, now == want) {
        (Action::Enable, true) => "Remote Services enabled.".to_string(),
        (Action::Disable, true) => "Remote Services disabled.".to_string(),
        (Action::Enable, false) => {
            let mut miss = Vec::new();
            if !now.ssh {
                miss.push("SSH");
            }
            if !now.smb {
                miss.push("Samba");
            }
            if !now.files {
                miss.push("File browser");
            }
            format!("Not running: {}.", miss.join(", "))
        }
        (Action::Disable, false) => "Some services are still running.".to_string(),
    };
    let ok = now == want && warnings.is_empty();
    let text = match warnings.first() {
        Some(w) if now == want => format!("{text} ({w})"),
        _ => text,
    };
    set(p, |p| {
        *p = Progress {
            outcome: Some(Outcome { ok, text }),
            ..Progress::default()
        }
    });
}

/// Dispara a acao numa thread; devolve o estado compartilhado para a tela ler.
pub fn spawn(r: Arc<dyn Runner>, act: Action, p: Shared) -> thread::JoinHandle<()> {
    set(&p, |p| {
        p.running = Some(act);
        p.step = 0;
        p.total = 0;
        p.outcome = None;
    });
    thread::spawn(move || perform(&*r, act, &p, Duration::from_secs(5)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rg52remote-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_proc(d: &Path, procs: &[(u32, &str)]) {
        for (pid, comm) in procs {
            let p = d.join(pid.to_string());
            fs::create_dir_all(&p).unwrap();
            fs::write(p.join("comm"), format!("{comm}\n")).unwrap();
        }
        // entradas que nao sao processos
        fs::create_dir_all(d.join("net")).unwrap();
        fs::write(d.join("uptime"), "1 1").unwrap();
    }

    #[test]
    fn scan_acha_os_tres_servicos() {
        let d = tmpdir("scan");
        fake_proc(
            &d,
            &[
                (1, "systemd"),
                (200, "sshd"),
                (300, "smbd"),
                (301, "nmbd"),
                (400, "filebrowser"),
            ],
        );
        let s = scan(&d);
        assert_eq!(s, Svcs::ALL);
        assert_eq!(s.level(), Level::On);
    }

    #[test]
    fn scan_parcial_e_vazio() {
        let d = tmpdir("scan2");
        fake_proc(&d, &[(1, "systemd"), (200, "sshd"), (301, "nmbd")]);
        let s = scan(&d);
        assert_eq!(
            s,
            Svcs {
                ssh: true,
                smb: false,
                files: false
            }
        );
        assert_eq!(s.level(), Level::Partial);
        assert_eq!(scan(&tmpdir("vazio")).level(), Level::Off);
        assert_eq!(scan(Path::new("/nao/existe")).level(), Level::Off);
    }

    #[test]
    fn arquivo_de_estado_acompanha_os_servicos() {
        let d = tmpdir("flag");
        let f = d.join("rg52-remote.on");
        sync_flag(&f, &Svcs::default()).unwrap(); // apagar o que nao existe nao e erro
        assert!(!f.exists());
        sync_flag(&f, &Svcs::ALL).unwrap();
        assert!(f.exists());
        let so_ssh = Svcs {
            ssh: true,
            ..Svcs::default()
        };
        sync_flag(&f, &so_ssh).unwrap();
        assert!(f.exists(), "parcial tambem acende o icone");
        sync_flag(&f, &Svcs::default()).unwrap();
        assert!(!f.exists());
    }

    #[test]
    fn rota_padrao() {
        let com = "Iface\tDestination\tGateway\tFlags\nwlan0\t00000000\t0100A8C0\t0003\nwlan0\t0000A8C0\t00000000\t0001\n";
        let sem = "Iface\tDestination\tGateway\tFlags\nwlan0\t0000A8C0\t00000000\t0001\n";
        assert!(has_default_route(com));
        assert!(!has_default_route(sem));
        assert!(!has_default_route(""));
    }

    #[test]
    fn planos_seguem_os_scripts_do_darkos() {
        let en = enable_plan();
        let flat: Vec<String> = en
            .iter()
            .flat_map(|s| s.cmds.iter().map(|c| c.join(" ")))
            .collect();
        assert!(flat.contains(&"sudo -n systemctl start smbd".to_string()));
        assert!(flat.contains(&"sudo -n systemctl start nmbd".to_string()));
        assert!(flat.contains(&"sudo -n systemctl start ssh.service".to_string()));
        assert!(flat.contains(&"sudo -n timedatectl set-ntp 1".to_string()));
        assert!(flat.contains(
            &"sudo -n filebrowser -a 0.0.0.0 -p 80 -d /home/ark/.config/filebrowser.db -r /"
                .to_string()
        ));
        let fb = en.iter().find(|s| s.label == "File browser").unwrap();
        assert!(fb.detach, "o filebrowser fica rodando em segundo plano");

        let di = disable_plan();
        let flat: Vec<String> = di
            .iter()
            .flat_map(|s| s.cmds.iter().map(|c| c.join(" ")))
            .collect();
        assert!(flat.contains(&"sudo -n pkill filebrowser".to_string()));
        assert!(flat.contains(&"sudo -n systemctl stop ssh.service".to_string()));
        assert!(flat.contains(&"sudo -n timedatectl set-ntp 0".to_string()));
        // todo comando usa sudo sem pedir senha
        for s in en.iter().chain(di.iter()) {
            for c in &s.cmds {
                assert_eq!(&c[..2], ["sudo", "-n"]);
            }
        }
    }

    #[test]
    fn habilitar_e_desabilitar_na_simulacao() {
        let r = DryRunner {
            delay: Duration::ZERO,
            ..DryRunner::new(true)
        };
        let p: Shared = Arc::default();
        perform(&r, Action::Enable, &p, Duration::from_millis(200));
        assert_eq!(r.probe(), Svcs::ALL);
        let o = p.lock().unwrap().outcome.clone().unwrap();
        assert!(o.ok, "{o:?}");
        assert_eq!(o.text, "Remote Services enabled.");
        assert!(p.lock().unwrap().running.is_none());

        perform(&r, Action::Disable, &p, Duration::from_millis(200));
        assert_eq!(r.probe(), Svcs::default());
        assert_eq!(
            p.lock().unwrap().outcome.clone().unwrap().text,
            "Remote Services disabled."
        );
    }

    #[test]
    fn sem_rede_nao_liga_nada() {
        let r = DryRunner {
            delay: Duration::ZERO,
            ..DryRunner::new(false)
        };
        let p: Shared = Arc::default();
        perform(&r, Action::Enable, &p, Duration::ZERO);
        assert_eq!(r.probe(), Svcs::default());
        let o = p.lock().unwrap().outcome.clone().unwrap();
        assert!(!o.ok);
        assert!(o.text.contains("No network"));
        // desligar nao exige rede
        *r.state.lock().unwrap() = Svcs::ALL;
        perform(&r, Action::Disable, &p, Duration::from_millis(200));
        assert_eq!(r.probe(), Svcs::default());
    }

    struct Failing {
        fail: &'static str,
        ran: Mutex<Vec<&'static str>>,
    }
    impl Runner for Failing {
        fn run(&self, st: &Step) -> Result<(), String> {
            self.ran.lock().unwrap().push(st.label);
            if st.label == self.fail {
                Err("Unit smbd.service not found.".into())
            } else {
                Ok(())
            }
        }
        fn probe(&self) -> Svcs {
            Svcs::default()
        }
        fn has_network(&self) -> bool {
            true
        }
    }

    #[test]
    fn falha_obrigatoria_para_e_opcional_continua() {
        let r = Failing {
            fail: "Samba",
            ran: Mutex::new(vec![]),
        };
        let p: Shared = Arc::default();
        perform(&r, Action::Enable, &p, Duration::ZERO);
        let o = p.lock().unwrap().outcome.clone().unwrap();
        assert!(!o.ok);
        assert!(o.text.starts_with("Samba failed: Unit smbd"), "{}", o.text);
        // parou no Samba: SSH e filebrowser nem foram tentados
        assert_eq!(
            *r.ran.lock().unwrap(),
            ["Network wait", "Time sync", "Samba"]
        );

        let r = Failing {
            fail: "Time sync",
            ran: Mutex::new(vec![]),
        };
        perform(&r, Action::Enable, &p, Duration::ZERO);
        assert_eq!(
            r.ran.lock().unwrap().len(),
            5,
            "passo opcional nao interrompe"
        );
    }

    #[test]
    fn spawn_marca_andamento() {
        let r = Arc::new(DryRunner {
            delay: Duration::from_millis(30),
            ..DryRunner::new(true)
        });
        let p: Shared = Arc::default();
        let h = spawn(r.clone(), Action::Enable, p.clone());
        assert_eq!(p.lock().unwrap().running, Some(Action::Enable));
        h.join().unwrap();
        assert_eq!(r.probe(), Svcs::ALL);
        assert!(p.lock().unwrap().running.is_none());
    }
}
