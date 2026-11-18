// Textos da landing: uma tabela por idioma. Os fatos vêm do README do Recortei.

export const repo = "https://github.com/FabioHAraujo/Recortei";
export const releases = `${repo}/releases`;
export const license = `${repo}/blob/main/LICENSE`;
export const version = "0.1.0";
// Arquivos anexados à release pelo workflow de release. Os nomes sem versão sempre apontam para a
// última release; o pacote do Arch leva a versão no nome, então o link aponta para a tag.
const latest = `${releases}/latest/download`;
export const downloads = {
	windowsSetup: `${latest}/Recortei-Setup-x86_64.exe`,
	windowsZip: `${latest}/recortei-windows-x86_64.zip`,
	deb: `${latest}/recortei-linux-amd64.deb`,
	rpm: `${latest}/recortei-linux-x86_64.rpm`,
	arch: `${releases}/download/v${version}/recortei-${version}-1-x86_64.pkg.tar.zst`,
	tarball: `${latest}/recortei-linux-x86_64.tar.gz`,
} as const;
export type DownloadId = keyof typeof downloads;

export const install = {
	linux: `git clone ${repo}.git\ncd Recortei\n./install.sh`,
	windows: `git clone ${repo}.git\ncd Recortei\npowershell -ExecutionPolicy Bypass -File install.ps1`,
	cargo: `git clone ${repo}.git\ncd Recortei\ncargo build --release`,
} as const;

// Rótulos das abas de instalação: iguais nos dois idiomas, porque o CodeTabs guarda a aba escolhida pelo rótulo.
export const sourceTabs = { linux: "Linux", windows: "Windows", cargo: "Cargo" } as const;

export type Os = "windows" | "linux";
export type ToolId = "move" | "pen" | "line" | "arrow" | "rect" | "ellipse" | "text" | "highlight" | "pixelate" | "step";

const toolKeys: Record<ToolId, string> = {
	move: "V",
	pen: "P",
	line: "L",
	arrow: "A",
	rect: "R",
	ellipse: "E",
	text: "T",
	highlight: "H",
	pixelate: "B",
	step: "N",
};
type PlatformLink = { id: DownloadId; label: string };
// Cartão de download: notas curtas, um botão principal e links menores para os outros formatos.
type Platform = {
	name: string;
	detail: string;
	notes: readonly (string | { text: string; cmd: string })[];
	primary: PlatformLink;
	more: readonly PlatformLink[];
};

export const toolOrder = Object.keys(toolKeys) as ToolId[];
export const keyOf = (id: ToolId) => toolKeys[id];

export const messages = {
	"pt-BR": {
		brand: "Recortei",
		metaTitle: "Recortei: print de área com anotações, leve e open source",
		metaDescription:
			"Aperte Insert, selecione a área, anote e compartilhe. Um executável de menos de 10 MB para Windows e Linux, nativo, sem runtime e open source (GPL-3.0).",
		skip: "Pular para o conteúdo",
		navLabel: "Principal",
		navFeatures: "Ferramentas",
		navDownload: "Download",
		navShortcuts: "Atalhos",
		navFaq: "Perguntas",
		menu: "Abrir menu",
		menuTitle: "Navegação",
		github: "GitHub",
		introduction: "Apresentação",
		eyebrow: "Open source · GPL-3.0 · v0.1.0",
		headline: "Aperte Insert. Recorte, anote e",
		headlineWord: "compartilhe.",
		leadBefore: "Sem",
		leadStruck: "instalador gigante, conta ou anúncio",
		leadAfter: ": um executável de menos de 10 MB que fica na bandeja e abre na hora, com as ferramentas que um print de verdade precisa.",
		downloadFor: { windows: "Baixar para Windows", linux: "Baixar para Linux" },
		downloadGeneric: "Baixar o Recortei",
		viewSource: "Ver o código",
		platformsNote: "Para Windows 10/11 e Linux (Wayland e X11), em 64 bits.",
		demoLabel:
			"Ilustração: uma área da tela selecionada sobre uma janela escurecida, com seta, retângulo, marca-texto, área pixelizada e marcadores numerados, e a barra de ferramentas do Recortei logo abaixo.",
		demoSize: "640 × 360",
		demoText: "Clique aqui",
		statsLabel: "Números",
		stats: [
			{ value: "<10", unit: "MB", label: "Um executável só", hint: "Sem runtime para instalar" },
			{ value: "9", unit: "", label: "Ferramentas de anotação", hint: "Da seta à pixelização" },
			{ value: "7", unit: "cores", label: "Paleta rápida", hint: "Teclas 1 a 7" },
			{ value: "1", unit: "tecla", label: "Para abrir", hint: "Insert, de qualquer lugar" },
		],
		stepsTitle: "Do atalho ao link em três passos",
		steps: [
			{ title: "Selecione", body: "Aperte Insert e arraste sobre a área. Funciona com vários monitores; o botão direito recomeça a seleção." },
			{ title: "Anote", body: "A barra aparece logo abaixo do recorte: setas, caixas, texto, marca-texto, numeração e pixelização para esconder dados." },
			{ title: "Compartilhe", body: "Enter copia a imagem e salva em Imagens/Recortei. Ctrl+U envia para o vkdl.link e já copia o link." },
		],
		featuresTitle: "Tudo o que um print precisa, nada além",
		featuresLead: "Cada ferramenta tem a sua tecla. A ferramenta, a cor e a espessura escolhidas ficam salvas para a próxima vez.",
		tools: {
			move: { name: "Mover", body: "Ajusta e reposiciona a área selecionada." },
			pen: { name: "Caneta", body: "Desenho livre, para circular ou rabiscar." },
			line: { name: "Linha", body: "Retas, com Shift para travar em 45°." },
			arrow: { name: "Seta", body: "Aponta o que importa sem precisar explicar." },
			rect: { name: "Retângulo", body: "Destaca uma região; Shift faz um quadrado." },
			ellipse: { name: "Elipse", body: "Círculos e elipses; Shift faz um círculo." },
			text: { name: "Texto", body: "Enter confirma, Shift+Enter quebra a linha." },
			highlight: { name: "Marca-texto", body: "Grifa trechos sem cobrir o conteúdo." },
			pixelate: { name: "Pixelizar", body: "Censura senhas, e-mails e dados pessoais." },
			step: { name: "Numeração", body: "Marcadores 1, 2, 3… para um passo a passo." },
		} satisfies Record<ToolId, { name: string; body: string }>,
		downloadTitle: "Baixe para o seu sistema",
		downloadLead: "Instalador para Windows e pacotes para as principais distros Linux, sem pedir conta nem cadastro. No Linux, a primeira execução já configura a inicialização com a sessão e, no GNOME, a tecla Insert.",
		platforms: {
			windows: {
				name: "Windows",
				detail: "Windows 10 e 11, 64 bits",
				notes: ["Instala só para o seu usuário, sem administrador", "Abre com o Windows e fica na bandeja", "Tecla Insert global"],
				primary: { id: "windowsSetup", label: "Baixar instalador (.exe)" },
				more: [{ id: "windowsZip", label: "Versão portátil (.zip)" }],
			},
			linux: {
				name: "Linux",
				detail: "x86_64, Wayland e X11",
				notes: ["No GNOME, a tecla Insert já vem configurada", { text: "No KDE e em outros: atalho para", cmd: "recortei --capture" }, "Bandeja com AppIndicator"],
				primary: { id: "deb", label: "Debian e Ubuntu (.deb)" },
				more: [
					{ id: "rpm", label: "Fedora e openSUSE (.rpm)" },
					{ id: "arch", label: "Arch Linux (.pkg.tar.zst)" },
					{ id: "tarball", label: "Outras distros (.tar.gz)" },
				],
			},
		} satisfies Record<Os, Platform>,
		otherFormats: "Outros formatos",
		allFiles: "Todos os arquivos e o SHA256SUMS na página da release",
		yourSystem: "Seu sistema",
		sourceTitle: "Prefere instalar pelo código?",
		sourceLead: "Os scripts compilam se for preciso (requer Rust e, no Linux, um compilador C como o build-essential) e instalam tudo no seu usuário, sem pedir administrador.",
		shortcutsTitle: "Atalhos para não tirar a mão do teclado",
		shortcuts: [
			{ keys: [["Insert"]], label: "Abre a captura (de novo: confirma)" },
			{ keys: [["Enter"], ["Ctrl", "C"]], label: "Copia a imagem e salva em Imagens/Recortei" },
			{ keys: [["Ctrl", "U"]], label: "Envia para o vkdl.link e copia o link" },
			{ keys: [["1"], ["7"]], label: "Troca a cor (1 a 7)", range: true },
			{ keys: [["+"], ["−"]], label: "Espessura (ou a roda do mouse)" },
			{ keys: [["Ctrl", "Z"]], label: "Desfazer" },
			{ keys: [["Ctrl", "Y"], ["Ctrl", "Shift", "Z"]], label: "Refazer" },
			{ keys: [["Shift"]], label: "Ao arrastar: 45°, quadrados e círculos" },
			{ keys: [["Esc"]], label: "Cancela a captura" },
		],
		or: "ou",
		to: "a",
		cliTitle: "Linha de comando",
		cli: [
			{ cmd: "recortei", label: "inicia na bandeja" },
			{ cmd: "recortei --capture", label: "captura agora (inicia o app se preciso)" },
			{ cmd: "recortei --quit", label: "encerra a instância em execução" },
			{ cmd: "recortei --setup", label: "Linux: refaz a configuração (autostart e tecla Insert)" },
		],
		nativeTitle: "Nativo em cada sistema",
		nativeLead: "Escrito em Rust. A sobreposição é desenhada em software, uma janela por monitor, com a mesma aparência no Linux e no Windows.",
		nativeHead: { what: "Parte", linux: "Linux", windows: "Windows" },
		native: [
			{ what: "Captura", linux: "xdg-desktop-portal (Wayland e X11)", windows: "GDI, todos os monitores" },
			{ what: "Tecla Insert", linux: "Atalho do GNOME → --capture", windows: "RegisterHotKey" },
			{ what: "Bandeja", linux: "StatusNotifierItem", windows: "tray-icon" },
			{ what: "Instância única", linux: "Socket Unix abstrato", windows: "Mutex + named pipe" },
		],
		faqTitle: "Perguntas frequentes",
		faq: [
			{ question: "O Recortei é gratuito?", answer: "Sim. É software livre sob a licença GPL-3.0: use, estude, modifique e compartilhe à vontade. Quem distribuir uma versão modificada precisa manter a mesma licença e o código aberto." },
			{ question: "Funciona no Wayland?", answer: "Sim. No Linux a captura passa pelo xdg-desktop-portal, que funciona no Wayland e no X11. No GNOME, a primeira execução (ou recortei --setup) registra a tecla Insert; em outros ambientes, crie um atalho global para recortei --capture." },
			{ question: "Onde ficam os meus recortes?", answer: "Na área de transferência e na pasta Imagens/Recortei. Nada sai do seu computador a não ser que você use o envio." },
			{ question: "O envio para a nuvem é privado?", answer: "Não: os arquivos no vkdl.link são públicos para quem tiver o link. Envios anônimos não têm prazo de retenção garantido: quando o espaço acaba, os mais antigos são removidos primeiro. O link para apagar cada envio fica salvo em uploads.jsonl, ao lado do config.json (~/.config/recortei/ no Linux, %APPDATA%\\recortei\\ no Windows)." },
			{ question: "Por que o Windows mostra um aviso do SmartScreen?", answer: "O instalador ainda não tem assinatura digital, então o Windows pode avisar que protegeu o computador. Clique em Mais informações e depois em Executar assim mesmo. Se quiser, confira o arquivo com o SHA256SUMS publicado na release." },
			{ question: "E o macOS?", answer: "Não há versão para macOS. O projeto foca em Windows e Linux." },
			{ question: "Preciso de internet?", answer: "Só para enviar um recorte ao vkdl.link. Capturar, anotar, copiar e salvar funcionam offline." },
		],
		footerLabel: "Rodapé",
		footerLinks: "Links",
		footerLicense: "Licença GPL-3.0",
		footerIssues: "Reportar um problema",
		footerNote: "Feito no Brasil, em Rust.",
	},
	en: {
		brand: "Recortei",
		metaTitle: "Recortei: lightweight, open source area screenshots with annotations",
		metaDescription:
			"Press Insert, select an area, annotate and share. A native executable under 10 MB for Windows and Linux, no runtime, open source (GPL-3.0).",
		skip: "Skip to content",
		navLabel: "Main",
		navFeatures: "Tools",
		navDownload: "Download",
		navShortcuts: "Shortcuts",
		navFaq: "FAQ",
		menu: "Open menu",
		menuTitle: "Navigation",
		github: "GitHub",
		introduction: "Introduction",
		eyebrow: "Open source · GPL-3.0 · v0.1.0",
		headline: "Press Insert. Crop, annotate and",
		headlineWord: "share.",
		leadBefore: "No",
		leadStruck: "huge installer, account or ads",
		leadAfter: ": an executable under 10 MB that sits in the tray and opens instantly, with the tools a real screenshot needs.",
		downloadFor: { windows: "Download for Windows", linux: "Download for Linux" },
		downloadGeneric: "Download Recortei",
		viewSource: "View the source",
		platformsNote: "For Windows 10/11 and Linux (Wayland and X11), 64-bit.",
		demoLabel:
			"Illustration: a selected screen area over a dimmed window, with an arrow, a rectangle, a highlighter, a pixelated area and numbered markers, and the Recortei toolbar right below it.",
		demoSize: "640 × 360",
		demoText: "Click here",
		statsLabel: "Numbers",
		stats: [
			{ value: "<10", unit: "MB", label: "One executable", hint: "No runtime to install" },
			{ value: "9", unit: "", label: "Annotation tools", hint: "From arrows to pixelation" },
			{ value: "7", unit: "colors", label: "Quick palette", hint: "Keys 1 to 7" },
			{ value: "1", unit: "key", label: "To open it", hint: "Insert, from anywhere" },
		],
		stepsTitle: "From shortcut to link in three steps",
		steps: [
			{ title: "Select", body: "Press Insert and drag over the area. Works across monitors; right-click starts the selection over." },
			{ title: "Annotate", body: "The toolbar shows up right below the crop: arrows, boxes, text, highlighter, numbering and pixelation to hide data." },
			{ title: "Share", body: "Enter copies the image and saves it to Pictures/Recortei. Ctrl+U uploads to vkdl.link and copies the link." },
		],
		featuresTitle: "Everything a screenshot needs, nothing more",
		featuresLead: "Every tool has its own key. The tool, color and stroke you pick are remembered for next time.",
		tools: {
			move: { name: "Move", body: "Adjusts and repositions the selected area." },
			pen: { name: "Pen", body: "Freehand drawing, to circle or scribble." },
			line: { name: "Line", body: "Straight lines; Shift snaps to 45°." },
			arrow: { name: "Arrow", body: "Points at what matters, no words needed." },
			rect: { name: "Rectangle", body: "Frames a region; Shift makes a square." },
			ellipse: { name: "Ellipse", body: "Circles and ellipses; Shift makes a circle." },
			text: { name: "Text", body: "Enter confirms, Shift+Enter starts a new line." },
			highlight: { name: "Highlighter", body: "Marks text without covering it." },
			pixelate: { name: "Pixelate", body: "Hides passwords, emails and personal data." },
			step: { name: "Numbering", body: "Numbered 1, 2, 3… markers for step-by-step guides." },
		} satisfies Record<ToolId, { name: string; body: string }>,
		downloadTitle: "Download for your system",
		downloadLead: "A Windows installer and packages for the major Linux distros, no account or sign-up. On Linux, the first run sets up autostart and, on GNOME, the Insert key.",
		platforms: {
			windows: {
				name: "Windows",
				detail: "Windows 10 and 11, 64-bit",
				notes: ["Installs for your user only, no admin rights", "Starts with Windows and lives in the tray", "Global Insert hotkey"],
				primary: { id: "windowsSetup", label: "Download installer (.exe)" },
				more: [{ id: "windowsZip", label: "Portable version (.zip)" }],
			},
			linux: {
				name: "Linux",
				detail: "x86_64, Wayland and X11",
				notes: ["On GNOME, the Insert key is set up for you", { text: "On KDE and others: bind", cmd: "recortei --capture" }, "Tray with AppIndicator"],
				primary: { id: "deb", label: "Debian and Ubuntu (.deb)" },
				more: [
					{ id: "rpm", label: "Fedora and openSUSE (.rpm)" },
					{ id: "arch", label: "Arch Linux (.pkg.tar.zst)" },
					{ id: "tarball", label: "Other distros (.tar.gz)" },
				],
			},
		} satisfies Record<Os, Platform>,
		otherFormats: "Other formats",
		allFiles: "Every file and the SHA256SUMS on the release page",
		yourSystem: "Your system",
		sourceTitle: "Rather install from source?",
		sourceLead: "The scripts build when needed (Rust required, plus a C compiler such as build-essential on Linux) and install everything for your user, no admin rights.",
		shortcutsTitle: "Shortcuts that keep your hands on the keyboard",
		shortcuts: [
			{ keys: [["Insert"]], label: "Starts a capture (again: confirms)" },
			{ keys: [["Enter"], ["Ctrl", "C"]], label: "Copies the image and saves it to Pictures/Recortei" },
			{ keys: [["Ctrl", "U"]], label: "Uploads to vkdl.link and copies the link" },
			{ keys: [["1"], ["7"]], label: "Switches color (1 to 7)", range: true },
			{ keys: [["+"], ["−"]], label: "Stroke width (or the mouse wheel)" },
			{ keys: [["Ctrl", "Z"]], label: "Undo" },
			{ keys: [["Ctrl", "Y"], ["Ctrl", "Shift", "Z"]], label: "Redo" },
			{ keys: [["Shift"]], label: "While dragging: 45°, squares and circles" },
			{ keys: [["Esc"]], label: "Cancels the capture" },
		],
		or: "or",
		to: "to",
		cliTitle: "Command line",
		cli: [
			{ cmd: "recortei", label: "starts in the tray" },
			{ cmd: "recortei --capture", label: "captures now (starts the app if needed)" },
			{ cmd: "recortei --quit", label: "quits the running instance" },
			{ cmd: "recortei --setup", label: "Linux: redoes the setup (autostart and Insert key)" },
		],
		nativeTitle: "Native on every system",
		nativeLead: "Written in Rust. The overlay is drawn in software, one window per monitor, and looks the same on Linux and Windows.",
		nativeHead: { what: "Part", linux: "Linux", windows: "Windows" },
		native: [
			{ what: "Capture", linux: "xdg-desktop-portal (Wayland and X11)", windows: "GDI, every monitor" },
			{ what: "Insert key", linux: "GNOME shortcut → --capture", windows: "RegisterHotKey" },
			{ what: "Tray", linux: "StatusNotifierItem", windows: "tray-icon" },
			{ what: "Single instance", linux: "Abstract Unix socket", windows: "Mutex + named pipe" },
		],
		faqTitle: "Questions",
		faq: [
			{ question: "Is Recortei free?", answer: "Yes. It is free software under the GPL-3.0 license: use, study, change and share it freely. Anyone who distributes a modified version must keep the same license and the source open." },
			{ question: "Does it work on Wayland?", answer: "Yes. On Linux, capture goes through xdg-desktop-portal, which works on Wayland and X11. On GNOME, the first run (or recortei --setup) registers the Insert key; elsewhere, bind a global shortcut to recortei --capture." },
			{ question: "Where do my screenshots go?", answer: "To the clipboard and the Pictures/Recortei folder. Nothing leaves your computer unless you use the upload." },
			{ question: "Is the cloud upload private?", answer: "No: files on vkdl.link are public to anyone with the link. Anonymous uploads have no guaranteed retention: when space runs out, the oldest are removed first. The delete link for each upload is saved in uploads.jsonl, next to config.json (~/.config/recortei/ on Linux, %APPDATA%\\recortei\\ on Windows)." },
			{ question: "Why does Windows show a SmartScreen warning?", answer: "The installer is not digitally signed yet, so Windows may say it protected your PC. Click More info, then Run anyway. If you like, check the file against the SHA256SUMS published with the release." },
			{ question: "What about macOS?", answer: "There is no macOS version. The project focuses on Windows and Linux." },
			{ question: "Do I need internet?", answer: "Only to upload a crop to vkdl.link. Capturing, annotating, copying and saving all work offline." },
		],
		footerLabel: "Footer",
		footerLinks: "Links",
		footerLicense: "GPL-3.0 license",
		footerIssues: "Report an issue",
		footerNote: "Made in Brazil, in Rust.",
	},
} as const;

export type Locale = keyof typeof messages;
