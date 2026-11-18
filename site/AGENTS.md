## Development

When starting the dev server, use background mode:

```
astro dev --background
```

Manage the background server with `astro dev stop`, `astro dev status`, and `astro dev logs`.

## Documentation

Full documentation: https://docs.astro.build

Consult these guides before working on related tasks:

- [Adding pages, dynamic routes, or middleware](https://docs.astro.build/en/guides/routing/)
- [Working with Astro components](https://docs.astro.build/en/basics/astro-components/)
- [Using React, Vue, Svelte, or other framework components](https://docs.astro.build/en/guides/framework-components/)
- [Adding or managing content](https://docs.astro.build/en/guides/content-collections/)
- [Adding styles or using Tailwind](https://docs.astro.build/en/guides/styling/)
- [Supporting multiple languages](https://docs.astro.build/en/guides/internationalization/)

<!-- codeatlas-ui:start -->
# Using @codeatlasdev/ui in this app

Docs entry point for agents: https://ui.codeatlas.com.br/llms.txt (recipes for app shell, list, form, settings, auth, landing, dashboard and theming; decision tables; common workarounds).

1. Find the screen type in the recipes and copy it whole; change only data, copy and theme.
2. Before writing CSS, a wrapper, a `setTimeout` or a local copy of a component, check https://ui.codeatlas.com.br/docs/getting-started/choosing/ and https://ui.codeatlas.com.br/docs/getting-started/workarounds/.
3. Never guess a prop. Ask the `ca-ui-mcp` MCP server (`get_component`, `get_recipe`, `search_demos`) or read `node_modules/@codeatlasdev/ui/dist/components.json` (`.components[]`, one entry per component; other exports under `.exports[]`). The installed version outranks every page and skill.
4. Pages are `AppList`, `AppForm`, `AppSettings`, `PageShell`, `AuthLayout`; `AppShellLayout` goes only in the layout of an authenticated route group, never in the root layout.
5. Layout is `Flex`, `Stack`, `Grid` with a spacing step (`gap="4"`, `columns={3}`). No layout CSS, no inline padding or margin, no raw colors: use `--ca-*` tokens. `Row`, `GlassCard` and `GlassSurface` are deprecated (`Flex`, `Card`, `Surface`; `bunx @codeatlasdev/ui migrate --write`).
6. Svelte 5 runes only. Import from `@codeatlasdev/ui`; charts, DataTable and CodeSnippet come from the `/charts`, `/table` and `/code` subpaths. Interface sound is opt-in: `sound` props on components do nothing until the app imports `@codeatlasdev/ui/sound` and shows `SoundToggle`; never use sound as the only signal.
7. One `ThemeOptions` object in `src/lib/theme.ts`, passed to the Vite plugin and the server hook (`bunx @codeatlasdev/ui init --write`). Override tokens by declaring `--ca-*` on `:root`.
8. Mobile: heights use `var(--ca-vh)`, never `100vh`; the 44px touch target is global.
9. If the library blocks you: write `// ca-ui-workaround(#<issue>): <why>` and call `report_issue` with neutral names only.
<!-- codeatlas-ui:end -->
