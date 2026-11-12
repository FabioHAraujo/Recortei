# Recortei

Captura de área de tela para GNOME/Wayland, inspirada no ShareX. Fica na bandeja do sistema e abre com a tecla **Insert**.

## Instalação

```bash
./install.sh
```

Instala as dependências (`gir1.2-ayatanaappindicator3-0.1`, `python3-gi-cairo`), registra o autostart, o atalho **Insert** no GNOME e a permissão do portal de screenshot.

## Uso

1. Aperte **Insert** e arraste para selecionar a área.
2. Anote com a barra de ferramentas que aparece abaixo da seleção.
3. **Enter**, **Ctrl+C** ou **duplo clique** dentro da área: copia a imagem para a área de transferência e salva em `~/Imagens/Recortei`.

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
