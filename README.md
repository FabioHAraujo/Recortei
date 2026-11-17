# Recortei

Captura de área de tela com anotações, inspirada no ShareX. Roda nativamente no **Linux** (GNOME/Wayland e X11) e no **Windows**, fica na bandeja do sistema e abre com a tecla **Insert**.

Escrito em Rust: um executável único, sem runtime nem DLLs (≈3 MB).

## Instalação

### Windows

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1
```

Compila (se preciso), instala em `%LOCALAPPDATA%\Programs\Recortei`, cria atalhos no Menu Iniciar e na Inicialização e inicia o app. A tecla **Insert** é registrada como atalho global. Para remover: `install.ps1 -Uninstall`.

### Linux

```bash
./install.sh
```

Compila (se preciso), instala em `~/.local/bin/recortei`, registra o autostart, o atalho **Insert** no GNOME (que chama `recortei --capture`) e a permissão do portal de screenshot.

- Ícone na bandeja: no GNOME é preciso a extensão **AppIndicator** (padrão no Ubuntu).
- Em outros ambientes (KDE etc.), crie um atalho global para `recortei --capture`.

### Compilar manualmente

Requer o [Rust](https://rustup.rs) (e, no Linux, um compilador C para linkar — `build-essential`).

```bash
cargo build --release      # gera target/release/recortei(.exe)
cargo test                 # testes
```

## Uso

1. Aperte **Insert** e arraste para selecionar a área (funciona com vários monitores).
2. Anote com a barra de ferramentas que aparece abaixo da seleção.
3. **Enter**, **Ctrl+C**, **duplo clique** dentro da área ou **Insert** de novo: copia a imagem para a área de transferência e salva em `Imagens/Recortei`.

### Ferramentas

| Tecla | Ferramenta |
|-------|------------|
| V | Mover / ajustar a área |
| P | Caneta (desenho livre) |
| L | Linha |
| A | Seta |
| R | Retângulo |
| E | Elipse |
| T | Texto (Enter confirma, Shift+Enter quebra linha) |
| H | Marca-texto |
| B | Pixelizar (censurar) |
| N | Numeração (1, 2, 3…) |

### Atalhos

- **1–7**: cor · **roda do mouse** ou **+ / −**: espessura
- **Shift** ao arrastar: linhas em 45°, quadrados e círculos
- **Ctrl+Z / Ctrl+Y**: desfazer / refazer
- **Setas**: move a área (Shift = 10px) · **Ctrl+setas**: redimensiona
- **Botão direito**: descarta a seleção e recomeça · **Esc**: cancela

### Linha de comando

```
recortei             inicia na bandeja
recortei --capture   captura agora (inicia o app se não estiver rodando)
recortei --quit      encerra a instância em execução
```

Ferramenta, cor e espessura escolhidas são lembradas em `~/.config/recortei/config.json` (Windows: `%APPDATA%\recortei\config.json`). Para depurar eventos de teclado/mouse, rode com `RECORTEI_DEBUG=1`.

## Como funciona

| | Linux | Windows |
|---|---|---|
| Captura | xdg-desktop-portal (Wayland e X11) | GDI (todos os monitores) |
| Tecla Insert | atalho do GNOME → `--capture` | `RegisterHotKey` |
| Bandeja | StatusNotifierItem (ksni) | `tray-icon` |
| Instância única | socket Unix abstrato | mutex + named pipe |

A sobreposição (uma janela por monitor) é desenhada em software com `winit` + `softbuffer` + `tiny-skia`; o texto usa a fonte DejaVu Sans embutida (licença em `assets/fonts`).

Código: `src/overlay.rs` (seleção, ferramentas, barra e desenho), `src/app.rs` (janelas e eventos), `src/platform/` (partes específicas de cada sistema).
