<!--
  Ilustração do hero: a sobreposição do Recortei sobre uma janela qualquer.
  Base: demo #385 (Hero intro sequence) do @codeatlasdev/ui · MIT License (c) CodeAtlas.
  O primeiro quadro estático (HTML do servidor, movimento reduzido) já é o recorte anotado completo:
  a animação só roda quando o MotionStage está tocando, e nada começa escondido.
-->
<script lang="ts">
	import { MotionStage } from "@codeatlasdev/ui";
	import {
		Check,
		Circle,
		CloudUpload,
		Grid3x3,
		Hash,
		Highlighter,
		Minus,
		Move,
		MoveUpRight,
		Pencil,
		Square,
		Type,
		X,
	} from "@lucide/svelte";
	import { keyOf, type ToolId } from "../lib/landing.ts";

	interface Props {
		label: string;
		size: string;
		buttonText: string;
		toolNames: Record<ToolId, { name: string }>;
	}
	let { label, size, buttonText, toolNames }: Props = $props();

	const tools: { id: ToolId; icon: typeof Move }[] = [
		{ id: "move", icon: Move },
		{ id: "pen", icon: Pencil },
		{ id: "line", icon: Minus },
		{ id: "arrow", icon: MoveUpRight },
		{ id: "rect", icon: Square },
		{ id: "ellipse", icon: Circle },
		{ id: "text", icon: Type },
		{ id: "highlight", icon: Highlighter },
		{ id: "pixelate", icon: Grid3x3 },
		{ id: "step", icon: Hash },
	];
	const inks = ["red", "yellow", "green", "blue", "purple", "black", "white"];
	// Quadradinhos da área pixelizada: tons fixos para o desenho ser igual no servidor e no cliente.
	const pixels = [3, 1, 4, 2, 0, 3, 2, 4, 1, 3, 0, 2, 4, 1, 3, 2, 0, 4, 1, 2, 3, 1, 4, 0];
</script>

<MotionStage {label} loop={6400}>
	{#snippet children(m)}
		{#key m.cycle}
			<div class="cap" class:cap--run={m.playing && m.cycle > 0} aria-hidden="true">
				<!-- A janela "capturada": um app genérico desenhado com blocos. -->
				<div class="cap__window">
					<div class="cap__bar">
						<span></span><span></span><span></span>
						<i class="cap__url"></i>
					</div>
					<div class="cap__body">
						<div class="cap__side">
							<i></i><i></i><i></i><i></i><i></i>
						</div>
						<div class="cap__main">
							<i class="cap__line cap__line--title"></i>
							<!-- As anotações ficam presas ao elemento que marcam, então acompanham qualquer largura. -->
							<i class="cap__line"><span class="cap__hl" style="--i:0"></span></i>
							<i class="cap__line cap__line--short"></i>
							<div class="cap__cards">
								<b class="cap__card">
									<span class="cap__box" style="--i:1"></span>
									<span class="cap__step" style="--i:1">1</span>
								</b>
								<b></b><b></b>
							</div>
							<i class="cap__line cap__line--mail">
								<span class="cap__pix" style="--i:2">
									{#each pixels as p, i (i)}<em style="--p:{p}"></em>{/each}
								</span>
								<span class="cap__step cap__step--2" style="--i:2">2</span>
							</i>
							<i class="cap__line cap__line--short"></i>
							<span class="cap__btn">
								{buttonText}
								<svg class="cap__arrow" style="--i:3" viewBox="0 0 100 60" preserveAspectRatio="none">
									<path d="M94 6 C 70 6, 34 16, 10 50" pathLength="1" />
									<path d="M24 50 L 8 53 L 11 37" pathLength="1" />
								</svg>
							</span>
						</div>
					</div>
				</div>

				<!-- Fora da seleção a tela escurece; dentro dela fica o recorte com as anotações. -->
				<div class="cap__sel">
					<span class="cap__size">{size}</span>
					<span class="cap__h cap__h--nw"></span><span class="cap__h cap__h--n"></span><span class="cap__h cap__h--ne"></span>
					<span class="cap__h cap__h--w"></span><span class="cap__h cap__h--e"></span>
					<span class="cap__h cap__h--sw"></span><span class="cap__h cap__h--s"></span><span class="cap__h cap__h--se"></span>
				</div>

				<div class="cap__toolbar">
					{#each tools as t (t.id)}
						{@const Icon = t.icon}
						<span class="cap__tool" class:cap__tool--on={t.id === "arrow"} title="{toolNames[t.id].name} ({keyOf(t.id)})">
							<Icon size={14} />
						</span>
					{/each}
					<span class="cap__sep"></span>
					{#each inks as ink, i (ink)}
						<span class="cap__ink" class:cap__ink--on={i === 0} style="--ink: var(--ca-token-ink-{ink})"></span>
					{/each}
					<span class="cap__sep"></span>
					<span class="cap__tool"><CloudUpload size={14} /></span>
					<span class="cap__tool cap__tool--ok"><Check size={14} /></span>
					<span class="cap__tool"><X size={14} /></span>
				</div>
			</div>
		{/key}
	{/snippet}
</MotionStage>

<style>
	/* Ilustração com geometria própria: posições em % da moldura, cores só de tokens. */
	.cap {
		--sel-x: 21%;
		--sel-y: 17.5%;
		--sel-w: 60%;
		--sel-h: 58.5%;
		position: relative;
		inline-size: 100%;
		aspect-ratio: 16 / 11;
		overflow: hidden;
		border-radius: var(--ca-radius-lg);
		outline: var(--ca-border-width) solid var(--ca-outline-default);
		background: var(--ca-surface-sunken);
		container-type: inline-size;
		user-select: none;
	}

	/* ---------- janela capturada ---------- */
	.cap__window {
		position: absolute;
		inset: 6% 5% 10% 5%;
		display: flex;
		flex-direction: column;
		border-radius: var(--ca-radius-md);
		background: var(--ca-surface-raised);
		outline: var(--ca-border-width) solid var(--ca-outline-subtle);
		overflow: hidden;
	}
	.cap__bar {
		display: flex;
		align-items: center;
		gap: 1.2cqi;
		padding: 1.6cqi 2cqi;
		border-block-end: var(--ca-border-width) solid var(--ca-outline-subtle);
	}
	.cap__bar > span {
		inline-size: 1.4cqi;
		block-size: 1.4cqi;
		border-radius: 50%;
		background: var(--ca-outline-default);
	}
	.cap__url {
		flex: 0 1 40%;
		block-size: 2.2cqi;
		margin-inline-start: 2cqi;
		border-radius: var(--ca-radius-pill);
		background: var(--ca-surface-sunken);
	}
	.cap__body {
		flex: 1;
		display: grid;
		grid-template-columns: 22% 1fr;
	}
	.cap__side {
		display: flex;
		flex-direction: column;
		gap: 2cqi;
		padding: 3cqi 2.5cqi;
		border-inline-end: var(--ca-border-width) solid var(--ca-outline-subtle);
	}
	.cap__side i {
		block-size: 1.6cqi;
		border-radius: var(--ca-radius-pill);
		background: var(--ca-outline-subtle);
	}
	.cap__side i:first-child {
		background: color-mix(in oklab, var(--ca-brand-text) 30%, transparent);
	}
	.cap__main {
		display: flex;
		flex-direction: column;
		gap: 1.8cqi;
		padding: 3cqi 4cqi;
		align-items: start;
	}
	.cap__line {
		display: block;
		inline-size: 78%;
		block-size: 1.6cqi;
		border-radius: var(--ca-radius-pill);
		background: var(--ca-outline-subtle);
	}
	.cap__line--title {
		inline-size: 46%;
		block-size: 3cqi;
		background: var(--ca-outline-default);
	}
	.cap__line--short {
		inline-size: 52%;
	}
	.cap__line--mail {
		inline-size: 40%;
	}
	.cap__cards {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 2cqi;
		inline-size: 100%;
		margin-block: 0.6cqi;
	}
	.cap__cards b {
		block-size: 9cqi;
		border-radius: var(--ca-radius-sm);
		background: var(--ca-surface-sunken);
		outline: var(--ca-border-width) solid var(--ca-outline-subtle);
	}
	.cap__btn {
		margin-block-start: 0.6cqi;
		padding: 1cqi 2.6cqi;
		border-radius: var(--ca-radius-control);
		background: var(--ca-text-primary);
		color: var(--ca-surface-raised);
		font-size: 1.9cqi;
		font-weight: var(--ca-font-weight-semibold);
		line-height: 1.2;
		white-space: nowrap;
	}

	/* ---------- seleção e escurecimento ---------- */
	.cap__sel {
		position: absolute;
		inset-inline-start: var(--sel-x);
		inset-block-start: var(--sel-y);
		inline-size: var(--sel-w);
		block-size: var(--sel-h);
		/* O escurecimento é a sombra enorme da própria seleção: o recorte fica com a cor original. */
		box-shadow: 0 0 0 200cqi color-mix(in oklab, var(--ca-token-ink-black) 52%, transparent);
		outline: max(1px, 0.3cqi) dashed var(--ca-token-ink-white);
		outline-offset: 0;
		transform-origin: top left;
	}
	.cap__size {
		position: absolute;
		inset-block-end: calc(100% + 1.2cqi);
		inset-inline-start: 0;
		padding: 0.3cqi 1.2cqi;
		border-radius: var(--ca-radius-sm);
		background: var(--ca-token-ink-black);
		color: var(--ca-token-ink-white);
		font-family: var(--ca-font-mono);
		font-size: 1.7cqi;
		line-height: 1.4;
		white-space: nowrap;
	}
	.cap__h {
		position: absolute;
		inline-size: 1.4cqi;
		block-size: 1.4cqi;
		background: var(--ca-token-ink-white);
		outline: max(1px, 0.2cqi) solid var(--ca-token-ink-black);
		translate: -50% -50%;
	}
	.cap__h--nw { inset-inline-start: 0; inset-block-start: 0; }
	.cap__h--n { inset-inline-start: 50%; inset-block-start: 0; }
	.cap__h--ne { inset-inline-start: 100%; inset-block-start: 0; }
	.cap__h--w { inset-inline-start: 0; inset-block-start: 50%; }
	.cap__h--e { inset-inline-start: 100%; inset-block-start: 50%; }
	.cap__h--sw { inset-inline-start: 0; inset-block-start: 100%; }
	.cap__h--s { inset-inline-start: 50%; inset-block-start: 100%; }
	.cap__h--se { inset-inline-start: 100%; inset-block-start: 100%; }

	/* ---------- anotações (presas aos elementos da janela) ---------- */
	.cap__line,
	.cap__card,
	.cap__btn {
		position: relative;
	}
	.cap__hl {
		position: absolute;
		inset: -0.9cqi -1.2cqi;
		background: color-mix(in oklab, var(--ca-token-ink-yellow) 60%, transparent);
		mix-blend-mode: multiply;
		transform-origin: left center;
	}
	.cap__box {
		position: absolute;
		inset: -1.1cqi;
		outline: max(2px, 0.55cqi) solid var(--ca-token-ink-blue);
		border-radius: var(--ca-radius-xs);
	}
	.cap__pix {
		position: absolute;
		inset: -0.9cqi -1cqi;
		display: grid;
		grid-template-columns: repeat(12, 1fr);
		grid-template-rows: repeat(2, 1fr);
		overflow: hidden;
	}
	.cap__pix em {
		background: color-mix(in oklab, var(--ca-text-secondary) calc(30% + var(--p) * 14%), var(--ca-surface-raised));
	}
	.cap__arrow {
		/* seta vermelha que chega pela direita e aponta para o botão */
		position: absolute;
		inset-inline-start: calc(100% + 0.6cqi);
		inset-block-end: 35%;
		inline-size: 17cqi;
		block-size: 9cqi;
		overflow: visible;
	}
	.cap__arrow path {
		fill: none;
		stroke: var(--ca-token-ink-red);
		stroke-width: max(2.5px, 0.75cqi);
		stroke-linecap: round;
		stroke-linejoin: round;
		vector-effect: non-scaling-stroke;
		stroke-dasharray: 1;
		stroke-dashoffset: 0;
	}
	.cap__step {
		position: absolute;
		inset-inline-start: -3.4cqi;
		inset-block-start: -3.4cqi;
		display: grid;
		place-items: center;
		inline-size: 4.6cqi;
		block-size: 4.6cqi;
		border-radius: 50%;
		background: var(--ca-token-ink-red);
		color: var(--ca-token-ink-white);
		font-size: 2.4cqi;
		font-style: normal;
		font-weight: var(--ca-font-weight-bold);
		line-height: 1;
		outline: max(1.5px, 0.35cqi) solid var(--ca-token-ink-white);
	}
	.cap__step--2 {
		inset-inline-start: -6cqi;
		inset-block-start: 50%;
		translate: 0 -50%;
		background: var(--ca-token-ink-purple);
	}

	/* ---------- barra de ferramentas ---------- */
	.cap__toolbar {
		position: absolute;
		inset-inline-start: var(--sel-x);
		inset-block-start: calc(var(--sel-y) + var(--sel-h) + 2.2%);
		display: flex;
		align-items: center;
		gap: 0.5cqi;
		padding: 0.8cqi;
		border-radius: var(--ca-radius-md);
		background: var(--ca-surface-overlay);
		outline: var(--ca-border-width) solid var(--ca-outline-default);
		color: var(--ca-text-secondary);
		transform-origin: top left;
		max-inline-size: calc(100% - var(--sel-x) - 2%);
	}
	.cap__tool {
		display: grid;
		place-items: center;
		inline-size: 3.6cqi;
		block-size: 3.6cqi;
		border-radius: var(--ca-radius-sm);
	}
	.cap__tool :global(svg) {
		inline-size: 2.2cqi;
		block-size: 2.2cqi;
	}
	.cap__tool--on {
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
	}
	.cap__tool--ok {
		color: var(--ca-token-ink-green);
	}
	.cap__sep {
		inline-size: var(--ca-border-width);
		block-size: 2.6cqi;
		margin-inline: 0.4cqi;
		background: var(--ca-outline-default);
	}
	.cap__ink {
		inline-size: 2.2cqi;
		block-size: 2.2cqi;
		border-radius: 50%;
		background: var(--ink);
		outline: var(--ca-border-width) solid var(--ca-outline-default);
	}
	.cap__ink--on {
		outline: max(1.5px, 0.35cqi) solid var(--ca-text-primary);
		outline-offset: max(1px, 0.25cqi);
	}

	/* ---------- animação: só com o palco tocando ---------- */
	.cap--run .cap__sel {
		animation: sel-in 900ms var(--ca-ease-emphasized, cubic-bezier(0.2, 0, 0, 1)) 200ms both;
	}
	.cap--run .cap__size {
		animation: fade-in 300ms ease-out 1000ms both;
	}
	.cap--run .cap__h {
		animation: fade-in 200ms ease-out 1000ms both;
	}
	.cap--run .cap__toolbar {
		animation: rise-in 420ms var(--ca-ease-emphasized, cubic-bezier(0.2, 0, 0, 1)) 1150ms both;
	}
	.cap--run .cap__hl {
		animation: grow-x 520ms ease-out calc(1700ms + var(--i) * 380ms) both;
	}
	.cap--run .cap__box,
	.cap--run .cap__pix,
	.cap--run .cap__step {
		animation: pop-in 360ms var(--ca-ease-emphasized, cubic-bezier(0.2, 0, 0, 1)) calc(1700ms + var(--i) * 380ms) both;
	}
	.cap--run .cap__arrow path {
		animation: draw 560ms ease-out calc(1700ms + var(--i) * 380ms) both;
	}
	.cap--run .cap__arrow path + path {
		animation-delay: calc(2260ms + var(--i) * 380ms);
		animation-duration: 220ms;
	}

	@keyframes sel-in {
		from { transform: scale(0.04, 0.06); opacity: 0.6; }
		to { transform: scale(1); opacity: 1; }
	}
	@keyframes fade-in {
		from { opacity: 0; }
		to { opacity: 1; }
	}
	@keyframes rise-in {
		from { opacity: 0; transform: translateY(1.6cqi); }
		to { opacity: 1; transform: translateY(0); }
	}
	@keyframes grow-x {
		from { transform: scaleX(0); }
		to { transform: scaleX(1); }
	}
	@keyframes pop-in {
		from { opacity: 0; transform: scale(0.6); }
		to { opacity: 1; transform: scale(1); }
	}
	@keyframes draw {
		from { stroke-dashoffset: 1; }
		to { stroke-dashoffset: 0; }
	}

	:global(.dark) .cap__hl,
	:global([data-ca-scheme="dark"]) .cap__hl {
		mix-blend-mode: normal;
		background: color-mix(in oklab, var(--ca-token-ink-yellow) 40%, transparent);
	}
</style>
