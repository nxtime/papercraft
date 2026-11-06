# Website export (`export_site`)

`export_site` writes frames as a website: plain HTML, or a project for React, Vue, Angular or Astro. Ask the person which framework, render and styling they want before the first export. Recommend from what they already have.

## Recommend

1. **Framework:** the one their repo or team uses (read `package.json`). If none:
   - `html` for a finished site nobody will develop further;
   - `astro` for a content or marketing site they will grow.
2. **Render:**
   - `static` (prerendered pages) for anything public: search engines and link previews read the prerendered head;
   - `spa` only for an app behind a login, because it sets the head in the browser.
3. **Styling:**
   - what their project already uses, else `css`;
   - `scss` and `tailwind` (v4) work with every framework;
   - CSS-in-JS works with React only: `vanilla-extract` (zero runtime, recommended), `styled-components` or `emotion` (each adds about 12 KB of runtime).

Client JavaScript per page (gzip, measured on the sample site in November 2026):

| Framework | static | spa |
|---|---|---|
| html, astro | 0 | n/a |
| vue | 0 (Nuxt scripts off) | 40 KB |
| angular | 77 KB | 63 KB |
| react | 136 KB (148 KB with styled-components or emotion) | 97 KB (108 KB with styled-components or emotion) |

## What each framework writes

| Framework, render | Project | Build | Serve |
|---|---|---|---|
| html | pages, one shared stylesheet, `assets/` | none (`scss`/`tailwind`: `npm install && npm run build`) | the folder |
| astro | Astro 7: `src/layouts/Layout.astro` owns the head, one component per frame in `src/components` | `npm install && npm run build` | `dist/` |
| vue, static | Nuxt 4, one SFC per frame, every route prerendered | `npm install && npm run generate` | `.output/public/` |
| vue, spa | Vite 8, vue-router 5, @unhead/vue | `npm install && npm run build` | `dist/` with a fallback to `index.html` |
| angular, static | Angular 22 standalone components, every route prerendered | `npm install && npm run build` | `dist/site/browser/` |
| angular, spa | the same, rendered in the browser | `npm install && npm run build` | `dist/site/browser/` with a fallback to `index.html` |
| react, static | Next.js 16 static export | `npm install && npm run build` | `out/` |
| react, spa | Vite 8, React 19, React Router 8 | `npm install && npm run build` | `dist/` with a fallback to `index.html` |

The reply's `next_steps` repeats the commands for the chosen variant.

The writers share these behaviours:
- Every framework gets the same pages: semantic tags from layer names, one h1, `<img>` with alt text from the layer name, self-hosted fonts, Open Graph tags and `robots.txt`.
- Headings follow the outline: text in a header or nav (the logo) is not a heading, a short line right under a larger heading in the same group is a paragraph, and no level is skipped.
- Name a layer `Favicon` (any case; a picture or a vector) and it becomes the site icon, as SVG or the picture's own file. Without one the page links `data:,`, so the browser asks for no `/favicon.ico`.
- The page's h1 face and the face most body text uses are preloaded (two fonts at most). Custom properties are written only where a declaration reads them, and `warnings` lists every text layer below WCAG AA contrast (4.5:1, 3:1 for large text).
- A link or button with no text gets the layer name as its `aria-label`.
- A frame at least 1024 px wide takes the window's full width. A narrower frame (a phone or tablet screen) stays a centred column of its designed width. Every page is at least as tall as the window, so its background reaches the edges.
- Below the designed width, pages reflow instead of clipping: fixed widths give way, rows of cards wrap, and type of 40 px or more shrinks with the window. Rows wider than their frame (carousels) and layers that overflow on purpose keep their designed size.
- Fonts are cut down to the characters the site uses in every language and written as WOFF2, each with a fallback face of matching metrics so the swap does not move the layout. Kerning is left out when the subsetter cannot keep it; the canvas does not kern either.
- `base_url` makes canonical and hreflang URLs absolute and adds `sitemap.xml`.

## Hosting

Every export also writes the files static hosts read. `vercel.json` sits at the project root; the others go in `public/` for the npm frameworks, so the build copies them into the folder you deploy (for html, the root is that folder):

- `_headers` (Netlify, Cloudflare Pages) and `vercel.json`: `Cache-Control: public, max-age=31536000, immutable` on the content-hashed paths only (`/assets/*`, plus `/_astro/*`, `/_nuxt/*`, `/_next/static/*`, or Angular's root `/*.js` and `/*.css`). HTML gets no rule on purpose: those hosts already revalidate it, and Netlify and Cloudflare would merge a catch-all with the asset rule.
- SPA renders only: `_redirects` (`/*  /index.html  200`) and a Vercel rewrite to `/index.html`. Real files win over the fallback.
- `.nojekyll` for astro, static vue and static react, whose output folders start with `_`.
- GitHub Pages ignores `_headers` and `vercel.json` (it sets `max-age=600`); deploy there with `.nojekyll`.

## Languages

- **Input:** pass `modes` with a variable collection whose modes are languages (`en`, `pt-BR`), and bind the copy to string variables.
- **Routes:** each mode becomes `/<lang>/` routes with one JSON catalog per language, and `/` lists the languages.
- **What changes per language:** text that differs becomes a message, and styles that differ become `:lang()` rules.
- **Title and description:** `title` and `description` may be objects keyed by mode name (`{"en": "...", "pt-BR": "..."}`); each language's pages carry their own `<title>`, meta description and og/twitter tags. A language without an entry takes the base language's.
- **Language picker:** `/` carries the first language's `lang`.
- **Single language:** routes sit at the root with literal text, and `lang` sets the language tag.

## Limits

- One layout per frame. Auto-layout frames shrink to the screen; frames without auto layout stay absolutely positioned, and the export warns about them.
- Pictures keep their format (PNG, JPEG, GIF, lossless WebP), downscaled to twice their displayed size. There is no AVIF.
- Vectors and icons are SVG files shown with `<img>`.
- Without `base_url`, canonical and hreflang URLs are relative (React leaves them out and warns), and no sitemap is written. Pass `base_url` for anything public.
- After design changes, export again to the same folder: a re-export replaces only the files the previous export wrote.
