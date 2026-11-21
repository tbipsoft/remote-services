# rg52-remote

Liga e desliga os **Remote Services** do dArkOS (SSH, Samba e filebrowser) numa tela só,
no mesmo visual dos outros apps `rg52-*`. Substitui, no menu Tools, o par
`Enable Remote Services.sh` / `Disable Remote Services.sh`.

## Como usar

| Botão | Ação |
|---|---|
| **A** (ou Enter / Espaço / toque no interruptor) | Ligar se estiver desligado ou parcial; desligar se estiver ligado |
| **B** (ou Start / Esc) | Sair (bloqueado enquanto uma ação está rodando) |

A tela mostra o interruptor grande (ON / OFF / PARTIAL), o estado de cada serviço
(SSH porta 22, Samba porta 445, File browser porta 80), o IP da rede e os comandos para
conectar (`ssh ark@IP`, `http://IP`, `smb://IP`). Sem Wi-Fi, ligar é recusado com a mesma
orientação do script original.

## O que cada ação faz (igual aos scripts originais)

**Ligar:** `systemctl enable/start NetworkManager-wait-online`, `timedatectl set-ntp 1`,
`start smbd`, `start nmbd`, `start ssh.service` e
`filebrowser -a 0.0.0.0 -p 80 -d /home/ark/.config/filebrowser.db -r /` em segundo plano.

**Desligar:** `pkill filebrowser`, `stop ssh.service`, `stop smbd`, `stop nmbd`,
`timedatectl set-ntp 0` e `disable/stop NetworkManager-wait-online`.

Tudo roda com `sudo -n` (sem pedir senha; o usuário `ark` já tem sudo livre). Os efeitos
duram até o próximo boot, como nos scripts originais (os `enable` dos serviços continuam
comentados). Passos obrigatórios que falham param a ação e mostram o erro; os opcionais
(NTP, wait-online) só viram aviso.

O estado vem de `/proc` (processos `sshd`, `smbd`, `filebrowser`), sem root.

## Instalação

1. Compile no aparelho: `cargo build --release` (precisa de `libsdl2-dev`).
2. Copie `target/release/rg52-remote` para uma pasta `rg52-remote/` ao lado do script
   `scripts/Remote Services.sh` no menu Tools (ou `/opt/rg52-remote/rg52-remote`).
3. `chmod +x "Remote Services.sh"`. Para trocar o destaque: `ACCENT=teal`.

## Linha de comando

```
rg52-remote --status       # imprime on / partial / off (código 0 se algum serviço roda)
rg52-remote --enable       # liga sem abrir a tela
rg52-remote --disable      # desliga sem abrir a tela
rg52-remote --dry-run      # simula tudo, não executa nada (para testar no PC)
rg52-remote --dry-run --no-network
```

## Ícone na barra de cima (Wi-Fi / energia / bateria)

Quem desenha esses ícones é o **EmulationStation**, não os scripts: ele os pinta como
bitmaps 32x16 no `swapBuffers` do `Renderer_GLES10.cpp`. Por isso o ícone exige um patch
pequeno no ES (pasta `es-patch/`):

* o app mantém o arquivo `/tmp/rg52-remote.on` (existe quando algum serviço está rodando;
  atualizado ao abrir o app, a cada ação e em `--status`). `/tmp` é limpo no boot e os
  serviços também voltam desligados, então os dois nunca ficam fora de sincronia depois
  de um reinício;
* o patch (`es-patch/rg52-remote-indicator.patch`) adiciona `RemoteIcon.h` (ícone de
  terminal `>_`) e um bloco que o desenha no slot livre `w - 135`, à esquerda do Wi-Fi
  (`w - 100`), da energia (`w - 65`) e da bateria (`w - 32`); sem o arquivo, o slot fica
  em branco, ou seja, nada aparece.

Aplicar: `git apply es-patch/rg52-remote-indicator.patch` na raiz do código do ES e
recompilar. Se alguém ligar os serviços pelos scripts antigos, o arquivo só é atualizado
na próxima vez que o app (ou `rg52-remote --status`) rodar.

## Verificado e não verificado

* 27 testes automáticos (estado por `/proc` falso, rota padrão, planos de comandos,
  ligar/desligar/falhas na simulação, arquivo de estado, desenho da tela e clique).
* O patch do ES foi checado com `git apply --check` no código do fork
  `christianhaitian/EmulationStation-fcamod`, **mas não foi compilado**, e esse fork é o
  da linha Odroid Go (go2/DRM). Não confirmei que o ES do dArkOS no RG52 (RK3562) usa o
  mesmo `Renderer_GLES10.cpp` e as mesmas posições.
* Não testado no aparelho: os comandos reais com `sudo -n`, a tela no KMSDRM e o
  launcher dentro do EmulationStation.
