// @ts-check
import svelte from "@astrojs/svelte";
import { caUI } from "@codeatlasdev/ui/astro";
import { defineConfig } from "astro/config";
import { theme } from "./src/theme.ts";

// https://astro.build/config
export default defineConfig({
  // Domínio público do site (canonical, og:url, JSON-LD). Ex.: SITE_URL=https://recortei.com.br bun run build
  site: process.env.SITE_URL || undefined,
  integrations: [svelte(), caUI(theme)],
  server: { host: "0.0.0.0", port: 4321 },
  devToolbar: { enabled: false },
});
