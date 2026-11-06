//! The Next.js 16 static export: config, `lib/seo.ts`, the layouts and one route file per page.

use super::component::Props;
use super::styles::Kit;
use super::{Ctx, Files, js, put};
use crate::mcp::site::model::{Output, Styling, mime};

pub fn write(cx: &Ctx, kit: &Kit, props: &[Props], files: &mut Files, out: &mut Output) {
    put(files, "tsconfig.json", TSCONFIG);
    put(files, "next.config.ts", next_config(cx.styling()));
    put(files, "scripts/flatten-prefetch.mjs", FLATTEN_PREFETCH);
    if cx.styling() == Styling::Tailwind {
        put(files, "postcss.config.mjs", POSTCSS);
    }
    put(
        files,
        ".gitignore",
        "/node_modules\n/.next/\n/out/\n*.tsbuildinfo\nnext-env.d.ts\n",
    );
    put(files, "lib/seo.ts", SEO_TS);
    if cx.multi {
        multi(cx, kit, props, files);
    } else {
        single(cx, kit, files);
    }
    if cx.site.base_url.is_none() {
        out.warnings.push("No base_url: the canonical and hreflang links are left out of the Next.js metadata (Next needs absolute addresses). Pass base_url to include them.".into());
    }
    out.next_steps.push("Run `npm install`, then `npm run build` in the output folder: the static site is written to out/ (`npm run dev` serves it with hot reload).".into());
}

fn next_config(styling: Styling) -> String {
    let (import, compiler, export) = match styling {
        Styling::VanillaExtract => (
            "import { createVanillaExtractPlugin } from '@vanilla-extract/next-plugin'\n\nconst withVanillaExtract = createVanillaExtractPlugin({ unstable_turbopack: { mode: 'auto' } })\n",
            "",
            "withVanillaExtract(nextConfig)",
        ),
        Styling::StyledComponents => (
            "",
            "  compiler: { styledComponents: true },\n",
            "nextConfig",
        ),
        Styling::Emotion => ("", "  compiler: { emotion: true },\n", "nextConfig"),
        _ => ("", "", "nextConfig"),
    };
    let gap = if import.is_empty() { "" } else { "\n" };
    format!(
        "import type {{ NextConfig }} from 'next'\n{import}{gap}const nextConfig: NextConfig = {{\n  output: 'export',\n  trailingSlash: true,\n  images: {{ unoptimized: true }},\n{compiler}}}\n\nexport default {export}\n"
    )
}

/// The font preload, once, in the layout: React dedupes it with the preload Next emits for the page.
fn preload(cx: &Ctx) -> (String, String) {
    let style = &cx.site.style;
    if style.preload.and_then(|i| style.fonts.get(i)).is_none() {
        return (String::new(), String::new());
    }
    (
        "import ReactDOM from 'react-dom'\nimport { FONT_TYPE, FONT_URL } from '@/lib/site'\n"
            .into(),
        "  ReactDOM.preload(FONT_URL, { as: 'font', type: FONT_TYPE, crossOrigin: 'anonymous' })\n"
            .into(),
    )
}

/// The page's h1 face as a `ReactDOM.preload` (import, call) when the layout does not preload
/// that face already; empty strings otherwise.
fn h1_preload(cx: &Ctx, page: usize) -> (&'static str, String) {
    let style = &cx.site.style;
    let face = style.h1_faces.get(page).copied().flatten();
    let Some(face) = face
        .filter(|i| Some(*i) != style.preload)
        .and_then(|i| style.fonts.get(i))
    else {
        return ("", String::new());
    };
    (
        "import ReactDOM from 'react-dom'\n",
        format!(
            "  ReactDOM.preload({}, {{ as: 'font', type: {}, crossOrigin: 'anonymous' }})\n",
            js(&format!("/{}", face.path)),
            js(&mime(&face.path, "font"))
        ),
    )
}

/// `<html ...>` attributes for a locale known at build time.
fn html_attrs(cx: &Ctx, loc: usize) -> String {
    let l = &cx.site.locales[loc];
    let mut s = String::new();
    if let Some(tag) = &l.tag {
        s.push_str(&format!(" lang={{{}}}", js(tag)));
    }
    if let Some(class) = &l.class {
        s.push_str(&format!(" className={{{}}}", js(class)));
    }
    s
}

fn root_layout(cx: &Ctx, kit: &Kit, html: &str) -> String {
    let (imports, call) = preload(cx);
    format!(
        "import type {{ ReactNode }} from 'react'\n{imports}{}\nexport default function RootLayout({{ children }}: {{ children: ReactNode }}) {{\n{call}  return (\n    <html{html}>\n      <body>{}</body>\n    </html>\n  )\n}}\n",
        kit.global_imports(),
        kit.wrap("{children}")
    )
}

fn single(cx: &Ctx, kit: &Kit, files: &mut Files) {
    put(
        files,
        "app/layout.tsx",
        root_layout(cx, kit, &html_attrs(cx, 0)),
    );
    for (i, page) in cx.site.pages.iter().enumerate() {
        let dir = if i == 0 {
            String::new()
        } else {
            format!("{}/", page.stem)
        };
        let (import, call) = h1_preload(cx, i);
        let body = format!(
            "import {c} from '@/components/{c}'\n{import}import {{ pageMetadata }} from '@/lib/seo'\n\nexport const metadata = pageMetadata(0, {i})\n\nexport default function Page() {{\n{call}  return <{c} />\n}}\n",
            c = page.component
        );
        put(files, &format!("app/{dir}page.tsx"), body);
    }
}

fn multi(cx: &Ctx, kit: &Kit, props: &[Props], files: &mut Files) {
    put(
        files,
        "app/(root)/layout.tsx",
        root_layout(cx, kit, &html_attrs(cx, 0)),
    );
    put(files, "app/(root)/page.tsx", LANGUAGE_INDEX);
    put(files, "app/[lang]/layout.tsx", lang_layout(cx, kit));
    for (i, page) in cx.site.pages.iter().enumerate() {
        let dir = if i == 0 {
            String::new()
        } else {
            format!("{}/", page.stem)
        };
        put(
            files,
            &format!("app/[lang]/{dir}page.tsx"),
            lang_page(&page.component, i, props[i], h1_preload(cx, i)),
        );
    }
}

fn lang_layout(cx: &Ctx, kit: &Kit) -> String {
    let (imports, call) = preload(cx);
    format!(
        "import type {{ ReactNode }} from 'react'\nimport {{ notFound }} from 'next/navigation'\n{imports}{}import {{ localeIndex, locales }} from '@/lib/site'\n\nexport const dynamicParams = false\nexport const generateStaticParams = () => locales.map((l) => ({{ lang: l.dir }}))\n\nexport default async function LangLayout({{\n  children,\n  params,\n}}: {{\n  children: ReactNode\n  params: Promise<{{ lang: string }}>\n}}) {{\n  const {{ lang }} = await params\n  if (!locales.some((l) => l.dir === lang)) notFound()\n  const locale = locales[localeIndex(lang)]\n{call}  return (\n    <html lang={{locale.tag ?? undefined}} className={{locale.cls ?? undefined}}>\n      <body>{}</body>\n    </html>\n  )\n}}\n",
        kit.global_imports(),
        kit.wrap("{children}")
    )
}

fn lang_page(comp: &str, page: usize, p: Props, (preload, call): (&str, String)) -> String {
    let mut imports = format!(
        "import {comp} from '@/components/{comp}'
{preload}"
    );
    let mut attrs = String::new();
    if p.loc {
        attrs.push_str(" loc={loc}");
    }
    if p.m {
        imports.push_str(
            "import { catalogs } from '@/lib/messages'
",
        );
        attrs.push_str(" m={catalogs[loc]}");
    }
    let (signature, locate) = if p.loc || p.m {
        (
            "{ params }: Props",
            "  const { lang } = await params
  const loc = localeIndex(lang)
",
        )
    } else {
        ("", "")
    };
    format!(
        "import type {{ Metadata }} from 'next'
{imports}import {{ pageMetadata }} from '@/lib/seo'
import {{ localeIndex }} from '@/lib/site'

type Props = {{ params: Promise<{{ lang: string }}> }}

export async function generateMetadata({{ params }}: Props): Promise<Metadata> {{
  const {{ lang }} = await params
  return pageMetadata(localeIndex(lang), {page})
}}

export default async function Page({signature}) {{
{locate}{call}  return <{comp}{attrs} />
}}
"
    )
}

/// Next 16 on Windows writes the prefetch files of nested routes into folders
/// (`__next.$d$lang\pricing\__PAGE__.txt`) because its export joins the segment path with the OS
/// separator, while the router asks for `__next.$d$lang.pricing.__PAGE__.txt`: a 404 per link. This
/// moves them to the flat name; where Next wrote flat names already there is nothing to move.
const FLATTEN_PREFETCH: &str = "import { readdirSync, renameSync, rmSync } from 'node:fs'
import { join } from 'node:path'

const files = (dir, parts = []) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? files(join(dir, e.name), [...parts, e.name]) : [[...parts, e.name]],
  )

function flatten(dir) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (!e.isDirectory() || e.name === '_next') continue
    const path = join(dir, e.name)
    if (!e.name.startsWith('__next.')) {
      flatten(path)
      continue
    }
    for (const parts of files(path)) {
      renameSync(join(path, ...parts), join(dir, [e.name, ...parts].join('.')))
    }
    rmSync(path, { recursive: true })
  }
}

flatten('out')
";

const TSCONFIG: &str = r#"{
  "compilerOptions": {
    "target": "ES2017",
    "lib": ["dom", "dom.iterable", "esnext"],
    "allowJs": true,
    "skipLibCheck": true,
    "strict": true,
    "noEmit": true,
    "esModuleInterop": true,
    "module": "esnext",
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "jsx": "react-jsx",
    "incremental": true,
    "plugins": [{ "name": "next" }],
    "paths": { "@/*": ["./*"] }
  },
  "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts", ".next/dev/types/**/*.ts", "**/*.mts"],
  "exclude": ["node_modules"]
}
"#;

const POSTCSS: &str = "const config = {
  plugins: { '@tailwindcss/postcss': {} },
}

export default config
";

/// Absolute alternates only: Next resolves relative ones against localhost, so without a base_url
/// the canonical and hreflang links are left out.
const SEO_TS: &str = "import type { Metadata } from 'next'
import { ICON, SITE, headData, indexHeadData } from './site'

export function pageMetadata(loc: number, page: number): Metadata {
  const h = headData(loc, page)
  return {
    title: h.title,
    description: h.description,
    icons: ICON,
    alternates: SITE ? { canonical: h.canonical, languages: Object.fromEntries(h.alternates) } : undefined,
    openGraph: {
      title: h.title,
      description: h.description,
      type: 'website',
      url: h.ogUrl ?? undefined,
      locale: h.ogLocale ?? undefined,
      alternateLocale: h.ogAlternates,
    },
    twitter: { card: 'summary' },
  }
}

export function indexMetadata(): Metadata {
  const h = indexHeadData()
  return {
    title: h.title,
    description: h.description,
    icons: ICON,
    alternates: SITE ? { canonical: h.canonical, languages: Object.fromEntries(h.alternates) } : undefined,
  }
}
";

const LANGUAGE_INDEX: &str = "import type { Metadata } from 'next'
import { indexMetadata } from '@/lib/seo'
import { locales } from '@/lib/site'

export const metadata: Metadata = indexMetadata()

export default function LanguageIndex() {
  return (
    <main>
      <ul>
        {locales.map((l) => (
          <li key={l.dir}>
            <a href={`/${l.dir}/`} hrefLang={l.tag ?? undefined}>
              {l.label}
            </a>
          </li>
        ))}
      </ul>
    </main>
  )
}
";
