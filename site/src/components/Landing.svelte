<!--
  Landing do Recortei. Base: receita "Landing page" do @codeatlasdev/ui (llms/recipes/landing-page),
  portada para uma ilha Svelte no Astro: dados, textos e tema trocados; componentes e props mantidos.
-->
<script lang="ts">
	import {
		Accordion,
		AccordionItem,
		Badge,
		Button,
		Card,
		CodeTabs,
		Container,
		Divider,
		Drawer,
		ExternalLink,
		Flex,
		Grid,
		Heading,
		Highlight,
		I18nProvider,
		IconButton,
		Kbd,
		LocaleSwitcher,
		NavigationMenu,
		NavigationMenuLink,
		Section,
		SimpleTable,
		SiteHeader,
		SkipLink,
		Stack,
		Stat,
		StrikeThrough,
		Text,
		ThemeToggle,
	} from "@codeatlasdev/ui";
	import {
		ArrowDown,
		Circle,
		Download,
		Grid3x3,
		Hash,
		Highlighter,
		Menu,
		Minus,
		Monitor,
		Move,
		MoveUpRight,
		Pencil,
		Square,
		Terminal,
		Type,
	} from "@lucide/svelte";
	import { onMount } from "svelte";
	import CaptureDemo from "./CaptureDemo.svelte";
	import GithubMark from "./GithubMark.svelte";
	import {
		downloads,
		install,
		keyOf,
		license,
		messages,
		releases,
		repo,
		sourceTabs,
		toolOrder,
		type Locale,
		type Os,
		type ToolId,
	} from "../lib/landing.ts";

	const locales = [
		{ value: "pt-BR", label: "Português", short: "PT" },
		{ value: "en", label: "English", short: "EN" },
	];

	// Troca de idioma na própria página (receita): `locale` é estado, `m` segue, e o I18nProvider
	// traduz os textos internos do pacote (aria-labels, o seletor).
	let locale = $state<Locale>("pt-BR");
	const m = $derived(messages[locale]);
	let menuOpen = $state(false);

	// Sistema do visitante: só no cliente. No servidor (e sem JS) os botões levam à seção de downloads.
	let os = $state<Os | null>(null);

	onMount(() => {
		const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
		const p = `${nav.userAgentData?.platform ?? ""} ${navigator.platform} ${navigator.userAgent}`.toLowerCase();
		const mobile = /android|iphone|ipad|ipod/.test(p);
		if (!mobile) os = p.includes("win") ? "windows" : p.includes("linux") || p.includes("x11") ? "linux" : null;

		try {
			const saved = localStorage.getItem("recortei:locale");
			if (saved === "pt-BR" || saved === "en") locale = saved;
			else if (!navigator.language.toLowerCase().startsWith("pt")) locale = "en";
		} catch {
			// Sem acesso ao storage (aba privada, preview): fica no idioma padrão.
		}
	});

	// O idioma vale para o documento inteiro: `lang`, título e descrição seguem a troca.
	$effect(() => {
		document.documentElement.lang = locale;
		document.title = m.metaTitle;
		document.querySelector('meta[name="description"]')?.setAttribute("content", m.metaDescription);
	});

	function pickLocale(next: string) {
		locale = next as Locale;
		try {
			localStorage.setItem("recortei:locale", next);
		} catch {
			// Preferência só desta visita.
		}
	}

	const toolIcons: Record<ToolId, typeof Move> = {
		move: Move,
		pen: Pencil,
		line: Minus,
		arrow: MoveUpRight,
		rect: Square,
		ellipse: Circle,
		text: Type,
		highlight: Highlighter,
		pixelate: Grid3x3,
		step: Hash,
	};
	const platformOrder: Os[] = ["windows", "linux"];
	const platformIcons = { windows: Monitor, linux: Terminal };
	const sourceOrder = ["linux", "windows", "cargo"] as const;
	// Forma de dados do CodeTabs: o <pre> já vem focável e nomeado, e o botão de copiar é o da biblioteca.
	const installTabs = sourceOrder.map((k) => ({ label: sourceTabs[k], code: install[k] }));

	// Ao escolher uma seção no menu do celular, o Drawer devolve o foco ao botão que o abriu.
	// Assim que esse foco volta, ele segue para o título da seção escolhida.
	let pendingTarget: string | null = null;
	function goFromMenu(href: string) {
		pendingTarget = href;
		menuOpen = false;
	}
	function onMenuButtonFocus() {
		if (!pendingTarget) return;
		const heading = document.querySelector<HTMLElement>(`${pendingTarget} h2`);
		pendingTarget = null;
		if (!heading) return;
		heading.tabIndex = -1;
		heading.focus({ preventScroll: true });
	}

	// ca-ui-workaround(#pending-accordion): AccordionItem always renders an h2 and has no headingLevel
	// prop; inside a section titled by an h2 the questions must be level 3.
	// Lê `m.faq` para rodar de novo quando o idioma troca e os itens são recriados.
	function faqLevel(node: HTMLElement) {
		void m.faq;
		node.querySelectorAll(".ca-accordion-item__header").forEach((h) => h.setAttribute("aria-level", "3"));
	}

	const nav = $derived([
		{ href: "#ferramentas", label: m.navFeatures },
		{ href: "#download", label: m.navDownload },
		{ href: "#atalhos", label: m.navShortcuts },
		{ href: "#perguntas", label: m.navFaq },
	]);
</script>

<I18nProvider {locale}>
	<SkipLink>{m.skip}</SkipLink>

	<!-- ca-ui-workaround(#pending): SiteHeader has no brand/leading snippet outside an app shell; the brand link goes first in `trailing`, which is stretched to fill the bar. -->
	<SiteHeader class="rc-header">
		{#snippet trailing()}
			<a class="rc-brand" href="/" aria-label={m.brand}>
				<span class="rc-brand__mark" aria-hidden="true">
					<svg viewBox="0 0 22 22" width="18" height="18"><path d="M6 2v14h14" /><path d="M2 6h14v14" /></svg>
				</span>
				<span class="rc-brand__name">{m.brand}</span>
			</a>
			<NavigationMenu aria-label={m.navLabel}>
				{#each nav as item (item.href)}
					<NavigationMenuLink href={item.href}>{item.label}</NavigationMenuLink>
				{/each}
			</NavigationMenu>
			<span class="mobile-menu" onfocusin={onMenuButtonFocus}>
				<IconButton aria-label={m.menu} onclick={() => (menuOpen = true)}>
					<Menu size={18} />
				</IconButton>
			</span>
			<LocaleSwitcher options={locales} value={locale} onselect={pickLocale} compact />
			<Button variant="filled" size="sm" href="#download">{m.navDownload}</Button>
		{/snippet}
	</SiteHeader>

	<Drawer bind:open={menuOpen} side="left" title={m.menuTitle}>
		<Stack as="ul" gap="2">
			{#each nav as item (item.href)}
				<li>
					<Button variant="ghost" tone="neutral" fullWidth href={item.href} onclick={() => goFromMenu(item.href)}>{item.label}</Button>
				</li>
			{/each}
			<li>
				<Button variant="outlined" tone="neutral" fullWidth href={repo}>
					{#snippet leading()}<GithubMark />{/snippet}
					{m.github}
				</Button>
			</li>
		</Stack>
	</Drawer>

	<main id="main" tabindex="-1">
		<Container size="xl">
			<!-- Hero -->
			<Section size="3" aria-label={m.introduction}>
				<Grid columns={{ base: 1, lg: 2 }} gap={{ base: "8", lg: "12" }} align="center">
					<Stack gap="5" align="start">
						<Badge tone="brand" subtle size="md">{m.eyebrow}</Badge>
						<Heading level={1} size="display">
							{m.headline} <Highlight animated>{m.headlineWord}</Highlight>
						</Heading>
						<p class="rc-lead">
							{m.leadBefore} <StrikeThrough animated>{m.leadStruck}</StrikeThrough>{m.leadAfter}
						</p>
						<Flex gap="3" wrap>
							<Button variant="filled" size="lg" href="#download">
								{#snippet leading()}<Download size={18} />{/snippet}
								{os ? m.downloadFor[os] : m.downloadGeneric}
							</Button>
							<Button variant="outlined" tone="neutral" size="lg" href={repo}>
								{#snippet leading()}<GithubMark size={18} />{/snippet}
								{m.viewSource}
							</Button>
						</Flex>
						<Text role="caption" as="p">{m.platformsNote}</Text>
					</Stack>
					<CaptureDemo
						label={m.demoLabel}
						size={m.demoSize}
						buttonText={m.demoText}
						toolNames={m.tools}
					/>
				</Grid>
			</Section>

			<!-- Números -->
			<Section size="1" aria-label={m.statsLabel}>
				<Grid columns={{ base: 2, md: 4 }} gap="4">
					{#each m.stats as s (s.label)}
						<Card>
							<Stat label={s.label} value={s.value} unit={s.unit || undefined} hint={s.hint} size="lg" />
						</Card>
					{/each}
				</Grid>
			</Section>

			<!-- Como funciona -->
			<Section aria-labelledby="passos-title">
				<Stack gap="6">
					<h2 id="passos-title">{m.stepsTitle}</h2>
					<Grid columns={{ base: 1, md: 3 }} gap="4">
						{#each m.steps as step, i (step.title)}
							<Card title="{i + 1}. {step.title}" headingLevel={3} full>{step.body}</Card>
						{/each}
					</Grid>
				</Stack>
			</Section>

			<!-- Ferramentas -->
			<Section id="ferramentas" aria-labelledby="ferramentas-title">
				<Stack gap="6">
					<Stack gap="2">
						<h2 id="ferramentas-title">{m.featuresTitle}</h2>
						<p class="rc-muted">{m.featuresLead}</p>
					</Stack>
					<Grid columns={{ base: 2, md: 3, lg: 5 }} gap="3">
						{#each toolOrder as id (id)}
							{@const Icon = toolIcons[id]}
							<Card title={m.tools[id].name} headingLevel={3} full gap="sm">
								{#snippet headerActions()}<Kbd>{keyOf(id)}</Kbd>{/snippet}
								<span class="rc-tool-icon" aria-hidden="true"><Icon size={20} /></span>
								<span>{m.tools[id].body}</span>
							</Card>
						{/each}
					</Grid>
				</Stack>
			</Section>

			<!-- Download -->
			<Section id="download" aria-labelledby="download-title">
				<Stack gap="6">
					<Stack gap="2">
						<h2 id="download-title">{m.downloadTitle}</h2>
						<p class="rc-muted">{m.downloadLead}</p>
					</Stack>
					<Stack gap="3">
						<Grid columns={{ base: 1, md: 2 }} gap="4">
							{#each platformOrder as p (p)}
								{@const info = m.platforms[p]}
								{@const Icon = platformIcons[p]}
								{@const mine = os === p}
								<Card title={info.name} headingLevel={3} full gap="md">
									{#snippet headerActions()}
										{#if mine}<Badge tone="brand" subtle>{m.yourSystem}</Badge>{/if}
									{/snippet}
									<Flex gap="3" align="center">
										<span class="rc-tool-icon" aria-hidden="true"><Icon size={20} /></span>
										<Text role="label">{info.detail}</Text>
									</Flex>
									<Stack as="ul" gap="2" class="rc-list">
										{#each info.notes as note, ni (ni)}
											<li>{#if typeof note === "string"}{note}{:else}{note.text} <code class="rc-cmd">{note.cmd}</code>{/if}</li>
										{/each}
									</Stack>
									<Stack gap="2" class="rc-download">
										<Button
											variant={mine || !os ? "filled" : "outlined"}
											tone={mine || !os ? undefined : "neutral"}
											fullWidth
											href={downloads[info.primary.id]}
										>
											{#snippet leading()}<Download size={16} />{/snippet}
											{info.primary.label}
										</Button>
										<Flex as="ul" gap="1" wrap align="center" class="rc-list rc-list--plain" aria-label={m.otherFormats}>
											{#each info.more as link (link.id)}
												<li>
													<Button variant="ghost" tone="neutral" size="sm" href={downloads[link.id]}>{link.label}</Button>
												</li>
											{/each}
										</Flex>
									</Stack>
								</Card>
							{/each}
						</Grid>
						<Text role="caption" as="p">
							<ExternalLink href="{releases}/latest">{m.allFiles}</ExternalLink>
						</Text>
					</Stack>

					<Card title={m.sourceTitle} headingLevel={3}>
						<Stack gap="4">
							<p class="rc-muted">{m.sourceLead}</p>
							<CodeTabs sync="recortei-os" tabs={installTabs} />
							<Text role="caption" as="p">
								<ExternalLink href={releases}>{m.github} Releases</ExternalLink>
							</Text>
						</Stack>
					</Card>
				</Stack>
			</Section>

			<!-- Atalhos -->
			<Section id="atalhos" aria-labelledby="atalhos-title">
				<Stack gap="6">
					<h2 id="atalhos-title">{m.shortcutsTitle}</h2>
					<Grid columns={{ base: 1, md: 2 }} gap="4" align="start">
						<Card>
							<Stack as="ul" gap="3" class="rc-list rc-list--plain">
								{#each m.shortcuts as s (s.label)}
									<li class="rc-shortcut">
										<span class="rc-keys">
											{#each s.keys as combo, ci (ci)}
												{#if ci > 0}<span class="rc-muted">{"range" in s ? m.to : m.or}</span>{/if}
												{#each combo as key, ki (ki)}
													{#if ki > 0}<span aria-hidden="true">+</span>{/if}<Kbd>{key}</Kbd>
												{/each}
											{/each}
										</span>
										<span>{s.label}</span>
									</li>
								{/each}
							</Stack>
						</Card>
						<Card title={m.cliTitle} headingLevel={3}>
							<Stack as="ul" gap="3" class="rc-list rc-list--plain">
								{#each m.cli as c (c.cmd)}
									<li class="rc-shortcut">
										<code class="rc-cmd">{c.cmd}</code>
										<span class="rc-muted">{c.label}</span>
									</li>
								{/each}
							</Stack>
						</Card>
					</Grid>
				</Stack>
			</Section>

			<!-- Nativo -->
			<Section aria-labelledby="nativo-title">
				<Stack gap="6">
					<Stack gap="2">
						<h2 id="nativo-title">{m.nativeTitle}</h2>
						<p class="rc-muted">{m.nativeLead}</p>
					</Stack>
					<SimpleTable responsive="cards">
						<thead>
							<tr><th>{m.nativeHead.what}</th><th>{m.nativeHead.linux}</th><th>{m.nativeHead.windows}</th></tr>
						</thead>
						<tbody>
							{#each m.native as row (row.what)}
								<tr>
									<td class="primary" data-label={m.nativeHead.what}>{row.what}</td>
									<td data-label={m.nativeHead.linux}>{row.linux}</td>
									<td data-label={m.nativeHead.windows}>{row.windows}</td>
								</tr>
							{/each}
						</tbody>
					</SimpleTable>
				</Stack>
			</Section>

			<!-- Perguntas -->
			<Section id="perguntas" aria-labelledby="perguntas-title" size="1">
				<Stack gap="6">
					<h2 id="perguntas-title">{m.faqTitle}</h2>
					<div class="rc-faq" {@attach faqLevel}>
					<Accordion type="single">
						{#each m.faq as item, i (item.question)}
							<AccordionItem value={String(i)} title={item.question}>{item.answer}</AccordionItem>
						{/each}
					</Accordion>
					</div>
					<Flex gap="3" wrap>
						<Button variant="outlined" tone="neutral" href="#download">
							{#snippet leading()}<ArrowDown size={16} />{/snippet}
							{m.navDownload}
						</Button>
					</Flex>
				</Stack>
			</Section>
		</Container>
	</main>

	<footer aria-label={m.footerLabel}>
		<Container size="xl">
			<Divider />
			<Section size="1">
				<Flex gap="4" wrap justify="between" align="center">
					<Stack gap="1">
						<strong>{m.brand}</strong>
						<Text role="caption">{m.footerNote}</Text>
					</Stack>
					<Stack as="nav" aria-label={m.footerLinks}>
						<Flex as="ul" gap="4" wrap align="center" class="rc-list rc-list--plain">
							<li><ExternalLink href={repo}>{m.github}</ExternalLink></li>
							<li><ExternalLink href="{repo}/issues">{m.footerIssues}</ExternalLink></li>
							<li><ExternalLink href={license}>{m.footerLicense}</ExternalLink></li>
						</Flex>
					</Stack>
					<ThemeToggle />
				</Flex>
			</Section>
		</Container>
	</footer>
</I18nProvider>

<style>
	/* Receita: o botão do menu só existe abaixo de 600px, quando o SiteHeader esconde o NavigationMenu. */
	.mobile-menu {
		display: contents;
	}
	@media (min-width: 600px) {
		.mobile-menu {
			display: none;
		}
	}

	/* ca-ui-workaround(#pending): sem slot de marca no SiteHeader, o `trailing` ocupa a barra toda e a marca empurra o resto para a direita. */
	:global(.rc-header .ca-site-header__left) {
		display: none;
	}
	:global(.rc-header .ca-site-header__trailing) {
		flex: 1;
		gap: var(--ca-space-2);
		padding-inline-start: var(--ca-space-3);
	}
	.rc-brand {
		display: inline-flex;
		align-items: center;
		gap: var(--ca-space-2);
		margin-inline-end: auto;
		color: var(--ca-text-primary);
		text-decoration: none;
		font-family: var(--ca-font-display);
		font-weight: var(--ca-font-weight-heading, 700);
		letter-spacing: var(--ca-display-tracking);
		font-size: var(--ca-font-size-h4, 1.125rem);
	}
	.rc-brand__mark {
		display: grid;
		place-items: center;
		inline-size: var(--ca-space-8);
		block-size: var(--ca-space-8);
		border-radius: var(--ca-radius-sm);
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
	}
	.rc-brand__mark path {
		fill: none;
		stroke: currentColor;
		stroke-width: 2.2;
		stroke-linecap: round;
	}
	@media (max-width: 379.98px) {
		.rc-brand__name {
			display: none;
		}
	}

	.rc-lead {
		font-size: var(--ca-font-size-h5, 1.125rem);
		color: var(--ca-text-secondary);
		max-inline-size: 36rem;
		text-wrap: pretty;
	}
	.rc-muted {
		color: var(--ca-text-secondary);
	}
	.rc-tool-icon {
		display: inline-grid;
		place-items: center;
		inline-size: var(--ca-space-10);
		block-size: var(--ca-space-10);
		border-radius: var(--ca-radius-sm);
		/* Mesmo tom do Badge subtle da biblioteca. */
		background: color-mix(in oklab, var(--ca-brand-text) 14%, transparent);
		color: var(--ca-brand-text);
	}
	:global(.rc-list) {
		padding-inline-start: var(--ca-space-5);
	}
	:global(.rc-list--plain) {
		list-style: none;
		padding-inline-start: 0;
	}
	.rc-shortcut {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--ca-space-2) var(--ca-space-4);
	}
	.rc-keys {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--ca-space-1);
	}
	.rc-cmd {
		font-family: var(--ca-font-mono);
		font-size: var(--ca-font-size-body2);
		white-space: nowrap;
	}
	.rc-faq {
		display: contents;
	}
	/* No celular, teclas em cima e descrição embaixo: alinhamento previsível em toda linha. */
	@media (max-width: 599.98px) {
		.rc-shortcut {
			flex-direction: column;
			align-items: flex-start;
			gap: var(--ca-space-1);
		}
	}
</style>
