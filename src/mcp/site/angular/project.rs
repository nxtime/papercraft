//! The project around the pages: package.json, angular.json, tsconfig, the entry points, the
//! global stylesheet for the chosen styling and index.html. Everything here is fixed text or
//! built with `serde_json`; the only site data is the title, the first locale and the font.

use crate::mcp::site::markup::{Dialect, escape_attr, escape_text};
use crate::mcp::site::model::{Output, Render, Site, Styling};
use serde_json::{Value, json};

/// The name of the Angular project: fixed, so it is always a valid identifier and the build
/// folder is `dist/site/browser`.
const PROJECT: &str = "site";

const TSCONFIG: &str = r#"{
  "compileOnSave": false,
  "compilerOptions": {
    "strict": true,
    "noImplicitOverride": true,
    "noPropertyAccessFromIndexSignature": true,
    "noImplicitReturns": true,
    "noFallthroughCasesInSwitch": true,
    "skipLibCheck": true,
    "isolatedModules": true,
    "experimentalDecorators": true,
    "importHelpers": true,
    "target": "ES2022",
    "module": "preserve",
    "resolveJsonModule": true
  },
  "angularCompilerOptions": {
    "enableI18nLegacyMessageIdFormat": false,
    "strictInjectionParameters": true,
    "strictInputAccessModifiers": true,
    "strictTemplates": true
  },
  "files": [],
  "references": [{ "path": "./tsconfig.app.json" }]
}
"#;

const TSCONFIG_APP: &str =
    "{\n  \"extends\": \"./tsconfig.json\",\n  \"include\": [\"src/**/*.ts\"]\n}\n";

const MAIN: &str = "import { bootstrapApplication } from '@angular/platform-browser';\nimport { appConfig } from './app/app.config';\nimport { App } from './app/app';\n\nbootstrapApplication(App, appConfig).catch((err) => console.error(err));\n";

const MAIN_SERVER: &str = "import { BootstrapContext, bootstrapApplication } from '@angular/platform-browser';\nimport { App } from './app/app';\nimport { config } from './app/app.config.server';\n\nconst bootstrap = (context: BootstrapContext) => bootstrapApplication(App, config, context);\n\nexport default bootstrap;\n";

const APP: &str = "import { Component } from '@angular/core';\nimport { RouterOutlet } from '@angular/router';\n\n@Component({\n  selector: 'app-root',\n  imports: [RouterOutlet],\n  template: '<router-outlet />',\n})\nexport class App {}\n";

const CONFIG_SERVER: &str = "import { mergeApplicationConfig, ApplicationConfig } from '@angular/core';\nimport { provideServerRendering, withRoutes } from '@angular/ssr';\nimport { appConfig } from './app.config';\nimport { serverRoutes } from './app.routes.server';\n\nconst serverConfig: ApplicationConfig = {\n  providers: [provideServerRendering(withRoutes(serverRoutes))],\n};\n\nexport const config = mergeApplicationConfig(appConfig, serverConfig);\n";

const POSTCSS: &str = "{\n  \"plugins\": {\n    \"@tailwindcss/postcss\": {}\n  }\n}\n";

pub fn write(site: &Site, out: &mut Output) -> Result<(), String> {
    let ssr = site.options.render == Render::Static;
    let json = |v: &Value| {
        serde_json::to_vec_pretty(v)
            .map(|mut b| {
                b.push(b'\n');
                b
            })
            .map_err(|e| e.to_string())
    };
    out.files
        .insert("package.json".into(), json(&package_json(site))?);
    out.files
        .insert("angular.json".into(), json(&angular_json(site))?);
    out.files.insert("tsconfig.json".into(), TSCONFIG.into());
    out.files
        .insert("tsconfig.app.json".into(), TSCONFIG_APP.into());
    out.files.insert(
        ".gitignore".into(),
        ".angular/\ndist/\nnode_modules/\n".into(),
    );
    out.files.insert("src/main.ts".into(), MAIN.into());
    out.files.insert("src/app/app.ts".into(), APP.into());
    out.files
        .insert("src/app/app.config.ts".into(), app_config(ssr).into());
    out.files
        .insert("src/index.html".into(), index_html(site).into());
    if ssr {
        out.files
            .insert("src/main.server.ts".into(), MAIN_SERVER.into());
        out.files
            .insert("src/app/app.config.server.ts".into(), CONFIG_SERVER.into());
    }
    match site.options.styling {
        Styling::Scss => {
            let stems: Vec<(usize, String)> = site
                .pages
                .iter()
                .enumerate()
                .map(|(i, p)| (i, p.stem.clone()))
                .collect();
            for (path, content) in site.style.scss("/", &stems) {
                out.files
                    .insert(format!("src/styles/{path}"), content.into());
            }
        }
        Styling::Tailwind => {
            let css = format!(
                "@import \"tailwindcss\";\n{}{}{}{}",
                site.style.tailwind_theme(),
                site.style.tokens_css(),
                site.style.fonts_css("/"),
                site.style.overrides_css("/", |_| true)
            );
            out.files.insert("src/styles.css".into(), css.into());
            out.files.insert(".postcssrc.json".into(), POSTCSS.into());
        }
        _ => {
            out.files
                .insert("src/styles.css".into(), site.style.global_css("/").into());
        }
    }
    Ok(())
}

fn package_json(site: &Site) -> Value {
    let mut deps = json!({
        "@angular/common": "^22.2.0",
        "@angular/compiler": "^22.2.0",
        "@angular/core": "^22.2.0",
        "@angular/platform-browser": "^22.2.0",
        "@angular/router": "^22.2.0",
        "rxjs": "~7.8.0",
        "tslib": "^2.3.0"
    });
    if site.options.render == Render::Static {
        deps["@angular/platform-server"] = json!("^22.2.0");
        deps["@angular/ssr"] = json!("^22.2.6");
    }
    let mut dev = json!({
        "@angular/build": "^22.2.6",
        "@angular/cli": "^22.2.6",
        "@angular/compiler-cli": "^22.2.0",
        "typescript": "~6.0.2"
    });
    if site.options.styling == Styling::Tailwind {
        dev["@tailwindcss/postcss"] = json!("4.3.4");
        dev["postcss"] = json!("^8.5.32");
        dev["tailwindcss"] = json!("4.3.4");
    }
    json!({
        "name": PROJECT,
        "version": "0.0.0",
        "private": true,
        "scripts": {"ng": "ng", "start": "ng serve", "build": "ng build", "watch": "ng build --watch --configuration development"},
        "dependencies": deps,
        "devDependencies": dev
    })
}

fn angular_json(site: &Site) -> Value {
    let style = if site.options.styling == Styling::Scss {
        "src/styles/main.scss"
    } else {
        "src/styles.css"
    };
    let mut options = json!({
        "browser": "src/main.ts",
        "tsConfig": "tsconfig.app.json",
        "assets": [{"glob": "**/*", "input": "public"}],
        "styles": [style]
    });
    if site.options.render == Render::Static {
        options["server"] = json!("src/main.server.ts");
        options["outputMode"] = json!("static");
        options["security"] = json!({"allowedHosts": []});
    }
    json!({
        "$schema": "./node_modules/@angular/cli/lib/config/schema.json",
        "version": 1,
        "cli": {"packageManager": "npm"},
        "newProjectRoot": "projects",
        "projects": {PROJECT: {
            "projectType": "application",
            "root": "",
            "sourceRoot": "src",
            "prefix": "app",
            "architect": {
                "build": {
                    "builder": "@angular/build:application",
                    "options": options,
                    "configurations": {
                        "production": {"outputHashing": "all"},
                        "development": {"optimization": false, "extractLicenses": false, "sourceMap": true}
                    },
                    "defaultConfiguration": "production"
                },
                "serve": {
                    "builder": "@angular/build:dev-server",
                    "configurations": {
                        "production": {"buildTarget": format!("{PROJECT}:build:production")},
                        "development": {"buildTarget": format!("{PROJECT}:build:development")}
                    },
                    "defaultConfiguration": "development"
                }
            }
        }}
    })
}

fn app_config(ssr: bool) -> String {
    let (hydration_import, hydration) = if ssr {
        (
            "import { provideClientHydration } from '@angular/platform-browser';\n",
            "    provideClientHydration(),\n",
        )
    } else {
        ("", "")
    };
    format!(
        "import {{ ApplicationConfig, provideBrowserGlobalErrorListeners }} from '@angular/core';\nimport {{ provideRouter, withComponentInputBinding, UrlSerializer }} from '@angular/router';\n{hydration_import}import {{ routes }} from './app.routes';\nimport {{ TrailingSlashSerializer }} from './trailing-slash';\n\nexport const appConfig: ApplicationConfig = {{\n  providers: [\n    provideBrowserGlobalErrorListeners(),\n    provideRouter(routes, withComponentInputBinding()),\n    {{ provide: UrlSerializer, useClass: TrailingSlashSerializer }},\n{hydration}  ],\n}};\n"
    )
}

fn index_html(site: &Site) -> String {
    let lang = site
        .locales
        .first()
        .and_then(|l| l.tag.as_deref())
        .map(|t| format!(" lang=\"{}\"", escape_attr(Dialect::Html, t)))
        .unwrap_or_default();
    let tags: String = site
        .asset_tags(0, "/")
        .iter()
        .map(|t| format!("  {}\n", t.to_html()))
        .collect();
    format!(
        "<!doctype html>\n<html{lang}>\n<head>\n  <meta charset=\"utf-8\">\n  <title>{}</title>\n  <base href=\"/\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n{}</head>\n<body>\n  <app-root></app-root>\n</body>\n</html>\n",
        escape_text(Dialect::Html, &site.site_title),
        tags
    )
}
