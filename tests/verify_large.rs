//! `verify_code` must survive a real-sized React module (a screen split over several files,
//! ~10 KB) without panicking or dropping the call; it returns a report, passed or not.

use papercraft::mcp::{Out, exec};
use papercraft::project::Store;
use serde_json::{Value, json};

fn host() -> exec::SessionHost {
    let store = Store::open(std::env::temp_dir().join(format!("papercraft-verify-{:016x}", fastrand::u64(..)))).unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Verify test").unwrap();
    h
}

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    match exec::call(h, name, &args) {
        Ok(outs) => match outs.into_iter().next() {
            Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
            _ => Value::Null,
        },
        Err(e) => panic!("{name} failed: {e}"),
    }
}

#[test]
fn verify_code_handles_a_large_multi_file_react_module() {
    let mut h = host();
    call(&mut h, "set_variables", json!({"variables": {"background": "#f6f7f3", "foreground": "#13201a", "sidebar": "#0e2a1f"}}));
    let board = call(
        &mut h,
        "create_artboard",
        json!({"name": "Entrar", "styles": {"width": "1440px", "height": "900px", "display": "flex", "backgroundColor": "var(--background)"}}),
    );
    let mut html = String::new();
    for i in 0..40 {
        html.push_str(&format!(
            r#"<div layer-name="Campo / {i}" style="display:flex;flex-direction:column;gap:8px"><div layer-name="Rótulo" style="font-size:14px;font-weight:700;line-height:20px;color:var(--foreground)">Campo {i}</div><div layer-name="Caixa" style="display:flex;height:40px;padding:0 12px;background:#ffffff;border-radius:10px"><div layer-name="Valor" style="font-size:14px;line-height:20px;color:var(--foreground)">valor</div></div></div>"#
        ));
    }
    call(&mut h, "write_html", json!({"targetNodeId": board["id"], "mode": "insert-children", "html": html}));

    // A screen the way an app repo writes it: several files, comments, imports, hooks, JSX.
    let mut code = String::from("// apps/web/src/components/auth-shell.tsx\nimport Image from \"next/image\";\nimport { Button } from \"@/components/ui/button\";\n\nexport function AuthShell({ children }: { children: React.ReactNode }) {\n  return (<div data-layer=\"Entrar\" className=\"flex min-h-screen bg-background\">{children}</div>);\n}\n\n// apps/web/src/app/(auth)/entrar/page.tsx\n\"use client\";\nexport default function EntrarPage() {\n  return (\n    <AuthShell>\n");
    for i in 0..40 {
        code.push_str(&format!(
            "      <div data-layer=\"Campo / {i}\" className=\"flex flex-col gap-2\">\n        <label data-layer=\"Rótulo\" className=\"text-sm font-bold leading-5 text-foreground\">{{t(\"campo.{i}\")}}</label>\n        <input data-layer=\"Caixa\" className=\"h-10 rounded-md bg-card px-3 text-sm leading-5\" />\n      </div>\n"
        ));
    }
    code.push_str("    </AuthShell>\n  );\n}\n");
    assert!(code.len() > 8_000, "payload is {} bytes", code.len());

    let report = call(&mut h, "verify_code", json!({"nodeId": board["id"], "target": "react", "code": code}));
    assert!(report.get("passed").is_some(), "no report: {report}");
    assert!(report["score"].is_number(), "{report}");
}
