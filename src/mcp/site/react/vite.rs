//! The Vite 8 + React Router 8 single-page app: config, `index.html`, the router, the head and
//! the language routes.

use super::styles::Kit;
use super::{Ctx, Files, js, put};
use crate::mcp::site::markup::{Dialect, escape_attr, escape_text};
use crate::mcp::site::model::{Output, Styling};

pub fn write(cx: &Ctx, kit: &Kit, files: &mut Files, out: &mut Output) {
    put(files, "index.html", index_html(cx));
    put(files, "vite.config.ts", vite_config(cx.styling()));
    put(files, "tsconfig.json", TSCONFIG);
    put(files, "tsconfig.app.json", TSCONFIG_APP);
    put(files, "tsconfig.node.json", TSCONFIG_NODE);
    put(files, ".gitignore", "node_modules\ndist\n*.tsbuildinfo\n");
    put(files, "src/main.tsx", main_tsx(kit));
    put(files, "src/router.tsx", router(cx));
    put(files, "src/lib/head.tsx", HEAD_TSX);
    if cx.multi {
        put(files, "src/pages/LangLayout.tsx", LANG_LAYOUT);
        put(files, "src/pages/LanguageIndex.tsx", LANGUAGE_INDEX);
    }
    out.next_steps.push("Run `npm install`, then `npm run build` in the output folder: the app is written to dist/ (`npm run dev` serves it with hot reload). It is a single-page app, so the host must answer every unknown path with dist/index.html.".into());
}

fn index_html(cx: &Ctx) -> String {
    let site = cx.site;
    let lang = site
        .locales
        .first()
        .and_then(|l| l.tag.as_deref())
        .map(|t| format!(" lang=\"{}\"", escape_attr(Dialect::Html, t)))
        .unwrap_or_default();
    let mut head = String::new();
    for tag in site.asset_tags(0, "/") {
        head.push_str(&format!("    {}\n", tag.to_html()));
    }
    format!(
        "<!doctype html>\n<html{lang}>\n  <head>\n    <meta charset=\"UTF-8\" />\n    <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\" />\n    <title>{}</title>\n{head}  </head>\n  <body>\n    <div id=\"root\"></div>\n    <script type=\"module\" src=\"/src/main.tsx\"></script>\n  </body>\n</html>\n",
        escape_text(Dialect::Html, &site.site_title)
    )
}

fn vite_config(styling: Styling) -> String {
    let (import, plugins) = match styling {
        Styling::Tailwind => (
            "import tailwindcss from '@tailwindcss/vite'\n",
            "react(), tailwindcss()",
        ),
        Styling::VanillaExtract => (
            "import { vanillaExtractPlugin } from '@vanilla-extract/vite-plugin'\n",
            "react(), vanillaExtractPlugin()",
        ),
        _ => ("", "react()"),
    };
    format!(
        "import react from '@vitejs/plugin-react'\nimport {{ defineConfig }} from 'vite'\n{import}\nexport default defineConfig({{\n  plugins: [{plugins}],\n}})\n"
    )
}

fn main_tsx(kit: &Kit) -> String {
    format!(
        "import {{ StrictMode }} from 'react'\nimport {{ createRoot }} from 'react-dom/client'\nimport {{ RouterProvider }} from 'react-router/dom'\nimport {{ router }} from './router'\n{}\ncreateRoot(document.getElementById('root')!).render(\n  <StrictMode>\n    {}\n  </StrictMode>,\n)\n",
        kit.global_imports(),
        kit.wrap("<RouterProvider router={router} />")
    )
}

fn router(cx: &Ctx) -> String {
    let pages = &cx.site.pages;
    let mut s = String::from("import { createBrowserRouter } from 'react-router'\n");
    for p in pages {
        s.push_str(&format!(
            "import {c} from './components/{c}'\n",
            c = p.component
        ));
    }
    if cx.multi {
        s.push_str("import { LangLayout } from './pages/LangLayout'\nimport { LanguageIndex } from './pages/LanguageIndex'\n");
    }
    s.push_str("\nexport const router = createBrowserRouter([\n");
    let route = |i: usize, indent: &str| {
        let c = &pages[i].component;
        if i == 0 && cx.multi {
            format!("{indent}{{ index: true, element: <{c} /> }},\n")
        } else if i == 0 {
            format!("{indent}{{ path: '/', element: <{c} /> }},\n")
        } else {
            let path = if cx.multi {
                pages[i].stem.clone()
            } else {
                format!("/{}", pages[i].stem)
            };
            format!("{indent}{{ path: {}, element: <{c} /> }},\n", js(&path))
        }
    };
    if cx.multi {
        s.push_str("  { path: '/', element: <LanguageIndex /> },\n  {\n    path: '/:lang',\n    element: <LangLayout />,\n    children: [\n");
        for i in 0..pages.len() {
            s.push_str(&route(i, "      "));
        }
        s.push_str("    ],\n  },\n");
    } else {
        for i in 0..pages.len() {
            s.push_str(&route(i, "  "));
        }
    }
    s.push_str("  { path: '*', element: <h1>404</h1> },\n])\n");
    s
}

const TSCONFIG: &str = "{\n  \"files\": [],\n  \"references\": [\n    { \"path\": \"./tsconfig.app.json\" },\n    { \"path\": \"./tsconfig.node.json\" }\n  ]\n}\n";

const TSCONFIG_APP: &str = r#"{
  "compilerOptions": {
    "tsBuildInfoFile": "./node_modules/.tmp/tsconfig.app.tsbuildinfo",
    "target": "es2023",
    "lib": ["ES2023", "DOM"],
    "module": "esnext",
    "types": ["vite/client"],
    "allowArbitraryExtensions": true,
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "verbatimModuleSyntax": true,
    "moduleDetection": "force",
    "noEmit": true,
    "jsx": "react-jsx",
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "resolveJsonModule": true,
    "erasableSyntaxOnly": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src"]
}
"#;

const TSCONFIG_NODE: &str = r#"{
  "compilerOptions": {
    "tsBuildInfoFile": "./node_modules/.tmp/tsconfig.node.tsbuildinfo",
    "target": "es2023",
    "lib": ["ES2023"],
    "types": ["node"],
    "skipLibCheck": true,
    "module": "nodenext",
    "allowImportingTsExtensions": true,
    "verbatimModuleSyntax": true,
    "moduleDetection": "force",
    "noEmit": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "erasableSyntaxOnly": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["vite.config.ts"]
}
"#;

/// React 19 hoists `<title>`, `<meta>` and `<link>` rendered anywhere into `document.head`.
const HEAD_TSX: &str = "import { useParams } from 'react-router'
import { headData, localeIndex } from './site'

/** The locale of the current route (the `:lang` segment). */
export function useLoc() {
  const { lang } = useParams()
  return localeIndex(lang ?? '')
}

export function PageHead({ loc, page }: { loc: number; page: number }) {
  const h = headData(loc, page)
  return (
    <>
      <title>{h.title}</title>
      <meta name=\"description\" content={h.description} />
      <link rel=\"canonical\" href={h.canonical} />
      {h.alternates.map(([lang, href]) => (
        <link key={lang} rel=\"alternate\" hrefLang={lang} href={href} />
      ))}
      <meta property=\"og:title\" content={h.title} />
      <meta property=\"og:description\" content={h.description} />
      <meta property=\"og:type\" content=\"website\" />
      {h.ogUrl && <meta property=\"og:url\" content={h.ogUrl} />}
      {h.ogLocale && <meta property=\"og:locale\" content={h.ogLocale} />}
      {h.ogAlternates.map((l) => (
        <meta key={l} property=\"og:locale:alternate\" content={l} />
      ))}
      <meta name=\"twitter:card\" content=\"summary\" />
    </>
  )
}
";

const LANG_LAYOUT: &str = "import { useEffect } from 'react'
import { Outlet, useParams } from 'react-router'
import { locales } from '../lib/site'

export function LangLayout() {
  const { lang } = useParams()
  const loc = locales.findIndex((l) => l.dir === lang)
  // <html> is not a hoistable tag, so its language and class are set from an effect.
  useEffect(() => {
    if (loc < 0) return
    document.documentElement.lang = locales[loc].tag ?? ''
    document.documentElement.className = locales[loc].cls ?? ''
  }, [loc])
  return loc < 0 ? <h1>404</h1> : <Outlet />
}
";

const LANGUAGE_INDEX: &str = "import { Link } from 'react-router'
import { indexHeadData, locales } from '../lib/site'

export function LanguageIndex() {
  const h = indexHeadData()
  return (
    <main>
      <title>{h.title}</title>
      <meta name=\"description\" content={h.description} />
      <link rel=\"canonical\" href={h.canonical} />
      {h.alternates.map(([lang, href]) => (
        <link key={lang} rel=\"alternate\" hrefLang={lang} href={href} />
      ))}
      <ul>
        {locales.map((l) => (
          <li key={l.dir}>
            <Link to={`/${l.dir}/`} hrefLang={l.tag ?? undefined}>
              {l.label}
            </Link>
          </li>
        ))}
      </ul>
    </main>
  )
}
";
