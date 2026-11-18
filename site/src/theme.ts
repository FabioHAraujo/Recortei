import { designStyle, type ThemeOptions } from "@codeatlasdev/ui/astro";

/**
 * Tema "Corte": Swiss (grade, filetes, cantos retos, sem sombra) com a cor da caneta do Recortei.
 *
 * - A marca é o vermelho padrão da anotação (`COLORS[0]` em src/overlay.rs, ≈ #EB3333), um pouco
 *   mais fechado para passar contraste como fundo de botão.
 * - As 7 cores da paleta de anotação do app viram tokens de domínio (`--ca-token-ink-*`), usados
 *   pela ilustração do hero. Não são papéis de UI, por isso ficam em `tokens`, não em CSS solto.
 * - Papel quadriculado (`paper: "grid"`) lembra a área de seleção do recorte.
 *
 * Mantenha este arquivo como objeto simples com import só de `@codeatlasdev/ui/astro`: o
 * astro.config o carrega no Node.
 */
export const theme: ThemeOptions = designStyle("swiss", {
	presets: [
		{
			id: "corte",
			label: "Corte",
			color: "#d92d2a",
			// No escuro o botão principal usa o próprio vermelho da marca, com texto branco.
			colors: {
				dark: { brandFill: "#d4332e", brandFillHover: "#b8231f", brandContrast: "#ffffff" },
			},
			tokens: {
				ink: {
					red: "#eb3333",
					yellow: "#ffc70d",
					green: "#33c74d",
					blue: "#338cff",
					purple: "#a659f2",
					black: "#000000",
					white: "#ffffff",
				},
			},
		},
	],
	defaultPreset: "corte",
	defaultScheme: "system",
	defaultStyle: "outlined",
	radius: 0.25,
	controlRadius: "sm",
	elevation: "none",
	paper: "grid",
	fonts: {
		body: {
			family: "Geist",
			src: "/fonts/geist.woff2",
			weight: "100 900",
			preload: true,
			fallback: "Arial",
			fallbackMetrics: { sizeAdjust: "104%", ascent: "92%", descent: "22%", lineGap: "0%" },
		},
		// Mesmo arquivo do corpo: string simples reaproveita o @font-face e o fallback com métricas.
		display: '"Geist", "Geist Fallback", Arial, sans-serif',
		mono: {
			family: "Geist Mono",
			src: "/fonts/geist-mono.woff2",
			weight: "100 900",
			fallback: "Courier New",
			generic: "monospace",
		},
	},
	typography: {
		weights: { heading: 700, semibold: 600 },
		headingTracking: "-0.02em",
		displayTracking: "-0.045em",
		displayLeading: 0.98,
		labelCase: "uppercase",
		labelTracking: "0.08em",
		measure: "62ch",
	},
	motion: { scale: 0.8, easing: "emphasized" },
	// Rótulos em caixa-alta espaçados (Swiss) ficam bem em etiquetas, não em botões com frase.
	components: { button: { tracking: "-0.005em", weight: 600 } },
	iconStroke: 1.75,
});
