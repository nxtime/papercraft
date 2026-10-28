//! Rewrites the CSS that agents write by habit but layers cannot hold into markup that builds the
//! same layout, before `write_html` makes layers: grids become rows of equal cells, margins
//! become gaps or spacers, inline boxes (chips, badges) become boxes, and text inputs show their
//! placeholder. Each rewrite matches what a browser shows; what cannot be matched is reported.

use super::html::{Element, Html};

/// Rewrite `tree` in place; returns notes on what was only approximated.
pub fn rewrite(tree: &mut [Html]) -> Vec<String> {
    let mut notes = Vec::new();
    for h in tree.iter_mut() {
        if let Html::Element(e) = h {
            element(e, &mut notes);
        }
    }
    notes
}

fn element(e: &mut Element, notes: &mut Vec<String>) {
    if e.raw.is_some() {
        return; // <svg>
    }
    input(e);
    for c in e.children.iter_mut() {
        if let Html::Element(k) = c {
            element(k, notes);
        }
    }
    inline_boxes(e);
    grid(e, notes);
    margins(e, notes);
}

/// `display: x` and the other declarations of an element's style, in order.
fn decls(e: &Element) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let src = e.attr("style").unwrap_or("");
    let (mut depth, mut cur) = (0i32, String::new());
    for c in src.chars().chain(std::iter::once(';')) {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ';' if depth == 0 => {
                if let Some((k, v)) = cur.split_once(':') {
                    out.push((k.trim().to_ascii_lowercase(), v.trim().to_owned()));
                }
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out
}

fn set_decls(e: &mut Element, d: &[(String, String)]) {
    let style = d.iter().map(|(k, v)| format!("{k}:{v}")).collect::<Vec<_>>().join(";");
    match e.attrs.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case("style")) {
        Some(s) => s.1 = style,
        None => e.attrs.push(("style".into(), style)),
    }
}

fn get<'a>(d: &'a [(String, String)], k: &str) -> Option<&'a str> {
    d.iter().rev().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

fn div(style: &str, name: &str, children: Vec<Html>) -> Html {
    Html::Element(Element {
        tag: "div".into(),
        attrs: vec![("layer-name".into(), name.into()), ("style".into(), style.into())],
        children,
        raw: None,
    })
}

/// A length in px (`16px`, `1rem`, `0`); None for `auto` and anything else.
fn px(v: &str) -> Option<f64> {
    let v = v.trim();
    if v == "0" {
        return Some(0.0);
    }
    if let Some(n) = v.strip_suffix("px") {
        return n.trim().parse().ok();
    }
    v.strip_suffix("rem").or_else(|| v.strip_suffix("em")).and_then(|n| n.trim().parse::<f64>().ok()).map(|n| n * 16.0)
}

fn is_row(d: &[(String, String)]) -> bool {
    matches!(get(d, "display"), Some("flex" | "inline-flex")) && !get(d, "flex-direction").is_some_and(|v| v.starts_with("column"))
}

/// A text input shows its value, else its placeholder in the browser's placeholder grey.
fn input(e: &mut Element) {
    let texty = e.tag == "textarea" || (e.tag == "input" && matches!(e.attr("type").unwrap_or("text"), "text" | "email" | "password" | "search" | "tel" | "url" | "number"));
    if !texty {
        return;
    }
    let (text, color) = match (e.attr("value").filter(|v| !v.is_empty()), e.attr("placeholder")) {
        (Some(v), _) => (v.to_owned(), None),
        (None, Some(p)) => (p.to_owned(), Some("#757575")),
        (None, None) => return,
    };
    let text = if e.attr("type") == Some("password") && color.is_none() { "\u{2022}".repeat(text.chars().count()) } else { text };
    if e.attr("layer-name").is_none() {
        e.attrs.push(("layer-name".into(), "Input".into()));
    }
    let mut d = decls(e);
    if get(&d, "display").is_none() {
        d.push(("display".into(), "flex".into()));
        d.push(("align-items".into(), "center".into()));
    }
    set_decls(e, &d);
    let style = color.map_or(String::new(), |c| format!("color:{c}"));
    e.tag = "div".into();
    e.children = vec![Html::Element(Element {
        tag: "span".into(),
        attrs: vec![("layer-name".into(), if color.is_some() { "Placeholder" } else { "Value" }.into()), ("style".into(), style)],
        children: vec![Html::Text(text)],
        raw: None,
    })];
}

/// Inline elements with a box (padding, background, border, inline-block) are boxes: chips,
/// badges, pills. A parent holding only such boxes lays them out in a wrapping row, as inline
/// flow does.
fn inline_boxes(e: &mut Element) {
    let mut boxes = 0;
    let mut other = false;
    for c in e.children.iter_mut() {
        match c {
            Html::Text(t) if t.trim().is_empty() => {}
            Html::Element(k) if super::html::is_inline(&k.tag) || k.tag == "button" => {
                let d = decls(k);
                let boxed = matches!(get(&d, "display"), Some("inline-block" | "inline-flex"))
                    || d.iter().any(|(n, _)| n.starts_with("padding") || n.starts_with("background") || n.starts_with("border"));
                if boxed {
                    if k.tag != "button" {
                        k.tag = "div".into();
                    }
                    boxes += 1;
                } else {
                    other = true;
                }
            }
            _ => other = true,
        }
    }
    let mut d = decls(e);
    if boxes > 1 && !other && get(&d, "display").is_none_or(|v| v == "block") {
        d.retain(|(k, _)| k != "display");
        d.extend([("display".into(), "flex".into()), ("flex-wrap".into(), "wrap".into()), ("align-items".into(), "center".into())]);
        set_decls(e, &d);
    }
}

/// A grid is laid out as rows of its columns: `fr` columns grow by their share, px columns are
/// fixed, `auto` columns hug. The last row is padded with empty cells so columns line up.
fn grid(e: &mut Element, notes: &mut Vec<String>) {
    let mut d = decls(e);
    if !matches!(get(&d, "display"), Some("grid" | "inline-grid")) {
        return;
    }
    let gaps: Vec<f64> = get(&d, "gap").map_or(vec![], |g| g.split_whitespace().filter_map(px).collect());
    let row_gap = get(&d, "row-gap").and_then(px).or(gaps.first().copied()).unwrap_or(0.0);
    let col_gap = get(&d, "column-gap").and_then(px).or(gaps.get(1).or(gaps.first()).copied()).unwrap_or(0.0);
    let width = get(&d, "width").and_then(px);
    let tracks = get(&d, "grid-template-columns").map_or_else(|| vec!["1fr".to_owned()], |t| tracks(t, width, col_gap, notes));
    d.retain(|(k, _)| !(k.starts_with("grid") || ["display", "gap", "row-gap", "column-gap", "justify-items"].contains(&k.as_str())));
    d.extend([("display".into(), "flex".into()), ("flex-direction".into(), "column".into()), ("gap".into(), format!("{row_gap}px"))]);
    set_decls(e, &d);
    let cell_style = |t: &str| -> String {
        if let Some(fr) = t.strip_suffix("fr").and_then(|f| f.trim().parse::<f64>().ok()) {
            format!("flex-grow:{fr}")
        } else if let Some(w) = px(t) {
            format!("width:{w}px")
        } else {
            String::new()
        }
    };
    if e.children.iter().any(|c| matches!(c, Html::Element(k) if k.attr("style").is_some_and(|s| s.contains("grid-column")))) {
        notes.push("grid-column spans are laid out as one cell each".into());
    }
    let cells: Vec<Html> = std::mem::take(&mut e.children).into_iter().filter(|c| !matches!(c, Html::Text(t) if t.trim().is_empty())).collect();
    let n = tracks.len().max(1);
    let mut rows = Vec::new();
    for (r, chunk) in cells.chunks(n).enumerate() {
        let mut kids = Vec::new();
        for (i, t) in tracks.iter().enumerate() {
            let style = cell_style(t);
            match chunk.get(i) {
                Some(Html::Element(k)) => {
                    let mut k = k.clone();
                    let mut kd = decls(&k);
                    kd.retain(|(n, _)| !n.starts_with("grid"));
                    if get(&kd, "width").is_none() {
                        kd.extend(style.split_once(':').map(|(a, b)| (a.to_owned(), b.to_owned())));
                    }
                    set_decls(&mut k, &kd);
                    kids.push(Html::Element(k));
                }
                Some(t @ Html::Text(_)) => kids.push(div(&style, "Cell", vec![t.clone()])),
                None if style.is_empty() => {}
                None => kids.push(div(&style, "Empty", vec![])),
            }
        }
        rows.push(div(&format!("display:flex;gap:{col_gap}px"), &format!("Row {}", r + 1), kids));
    }
    e.children = rows;
}

/// The columns of `grid-template-columns`, with `repeat(n, x)` expanded. `auto-fill` and
/// `auto-fit` take as many columns as fit `width`.
fn tracks(t: &str, width: Option<f64>, gap: f64, notes: &mut Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = t.trim();
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("repeat(") {
            let Some(end) = matching_paren(r) else { break };
            let (count, track) = r[..end].split_once(',').unwrap_or(("1", "1fr"));
            let track = track.trim();
            let one = track.strip_prefix("minmax(").and_then(|m| m.strip_suffix(')')).and_then(|m| m.split_once(',')).map_or(track, |(_, max)| max.trim());
            let n = match count.trim() {
                "auto-fill" | "auto-fit" => {
                    let min = track.strip_prefix("minmax(").and_then(|m| m.split_once(',')).and_then(|(min, _)| px(min)).or(px(track));
                    match (width, min) {
                        (Some(w), Some(m)) if m > 0.0 => ((w + gap) / (m + gap)).floor().max(1.0) as usize,
                        _ => {
                            notes.push(format!("grid `{}`: give the grid a px width to know its columns; laid out as 3", t.trim()));
                            3
                        }
                    }
                }
                c => c.parse().unwrap_or(1),
            };
            out.extend(std::iter::repeat_n(one.to_owned(), n));
            rest = r[end + 1..].trim_start();
        } else if let Some(m) = rest.strip_prefix("minmax(") {
            let Some(end) = matching_paren(m) else { break };
            out.push(m[..end].split_once(',').map_or("1fr", |(_, max)| max.trim()).to_owned());
            rest = m[end + 1..].trim_start();
        } else {
            let (tok, r) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            out.push(tok.to_owned());
            rest = r.trim_start();
        }
    }
    out
}

fn matching_paren(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(i),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Margins of an element along an axis: (start, end), each a px value or `auto` (None).
fn margin(d: &[(String, String)], row: bool) -> (Option<Option<f64>>, Option<Option<f64>>) {
    let val = |v: &str| if v.trim() == "auto" { Some(None) } else { px(v).map(Some) };
    let mut m: [Option<Option<f64>>; 4] = [None; 4]; // top, right, bottom, left
    for (k, v) in d {
        match k.as_str() {
            "margin" => {
                let p: Vec<&str> = v.split_whitespace().collect();
                let at = |i: usize| p.get(i).copied();
                let sides = match p.len() {
                    1 => [at(0), at(0), at(0), at(0)],
                    2 => [at(0), at(1), at(0), at(1)],
                    3 => [at(0), at(1), at(2), at(1)],
                    _ => [at(0), at(1), at(2), at(3)],
                };
                for (s, v) in m.iter_mut().zip(sides) {
                    *s = v.and_then(val);
                }
            }
            "margin-top" => m[0] = val(v),
            "margin-right" => m[1] = val(v),
            "margin-bottom" => m[2] = val(v),
            "margin-left" => m[3] = val(v),
            _ => {}
        }
    }
    if row { (m[3], m[1]) } else { (m[0], m[2]) }
}

/// Children's margins along the parent's flow become its gap when they are all one size, else
/// spacers of their size; `auto` margins become growing spacers (they take the free space). A
/// block's `margin: 0 auto` centres it.
fn margins(e: &mut Element, notes: &mut Vec<String>) {
    let d = decls(e);
    let row = is_row(&d);
    let flex = matches!(get(&d, "display"), Some("flex" | "inline-flex"));
    let gap = get(&d, "gap").and_then(|g| g.split_whitespace().next().and_then(px)).unwrap_or(0.0);
    let kids: Vec<usize> = e.children.iter().enumerate().filter(|(_, c)| matches!(c, Html::Element(_))).map(|(i, _)| i).collect();
    let m: Vec<_> = kids
        .iter()
        .map(|i| match &e.children[*i] {
            Html::Element(k) => margin(&decls(k), row),
            Html::Text(_) => (None, None),
        })
        .collect();
    if m.iter().all(|(a, b)| a.is_none() && b.is_none()) {
        return;
    }
    let len = |x: Option<Option<f64>>| x.flatten().unwrap_or(0.0);
    let auto = |x: Option<Option<f64>>| x == Some(None);
    // Space before each child (and after the last): block margins collapse, flex margins add.
    let mut space: Vec<f64> = Vec::new();
    for j in 0..=m.len() {
        let before = if j > 0 { len(m[j - 1].1) } else { 0.0 };
        let after = m.get(j).map_or(0.0, |x| len(x.0));
        space.push(if flex { before + after } else { before.max(after) });
    }
    let between = &space[1..m.len()];
    let uniform = !between.is_empty() && between.iter().all(|s| (s - between[0]).abs() < 1e-9) && space[0] == 0.0 && space[m.len()] == 0.0;
    let mut pd = d.clone();
    let mut out: Vec<Html> = Vec::new();
    let size = if row { "width" } else { "height" };
    let spacer = |px: f64| div(&format!("{size}:{px}px;flex-shrink:0"), "Spacer", vec![]);
    let grow = || div("flex-grow:1", "Spacer", vec![]);
    for (i, c) in std::mem::take(&mut e.children).into_iter().enumerate() {
        let Some(j) = kids.iter().position(|k| *k == i) else {
            out.push(c);
            continue;
        };
        let Html::Element(mut k) = c else { unreachable!() };
        let (start, end) = m[j];
        let extra = if j > 0 { space[j] - gap } else { space[j] };
        if !uniform && extra > 0.0 {
            out.push(spacer(extra));
        }
        if auto(start) {
            out.push(grow());
        }
        // Margins along the flow are now spacers; across it, `auto` on both sides centres.
        let mut kd = decls(&k);
        let (cs, ce) = margin(&kd, !row);
        kd.retain(|(n, _)| !n.starts_with("margin"));
        if auto(cs) && auto(ce) {
            kd.push(("align-self".into(), "center".into()));
        } else if len(cs) > 0.0 || len(ce) > 0.0 {
            notes.push(format!("margins across the flow are dropped (`{}`): pad the parent instead", k.attr("layer-name").unwrap_or(&k.tag)));
        }
        set_decls(&mut k, &kd);
        out.push(Html::Element(k));
        if auto(end) {
            out.push(grow());
        }
    }
    if !uniform && space[m.len()] > 0.0 {
        out.push(spacer(space[m.len()]));
    }
    if uniform && between[0] > 0.0 {
        pd.retain(|(k, _)| k != "gap");
        pd.push(("gap".into(), format!("{}px", gap + between[0])));
        set_decls(e, &pd);
    }
    e.children = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> String {
        let mut t = super::super::html::parse(src);
        rewrite(&mut t);
        fn show(h: &Html) -> String {
            match h {
                Html::Text(t) => t.clone(),
                Html::Element(e) => format!("<{} {}>{}</>", e.tag, e.attr("style").unwrap_or(""), e.children.iter().map(show).collect::<String>()),
            }
        }
        t.iter().map(show).collect()
    }

    #[test]
    fn a_grid_becomes_rows_of_equal_cells() {
        let out = run("<div style=\"display:grid;grid-template-columns:repeat(3,1fr);gap:16px\"><p>a</p><p>b</p><p>c</p><p>d</p></div>");
        assert_eq!(out.matches("<div display:flex;gap:16px>").count(), 2, "{out}");
        assert_eq!(out.matches("flex-grow:1").count(), 6, "4 cells and 2 empty ones keep the columns: {out}");
        assert!(out.starts_with("<div display:flex;flex-direction:column;gap:16px>"), "{out}");
    }

    #[test]
    fn auto_fill_takes_the_columns_that_fit() {
        let mut notes = Vec::new();
        assert_eq!(tracks("repeat(auto-fill, minmax(200px, 1fr))", Some(640.0), 16.0, &mut notes).len(), 3);
        assert_eq!(tracks("240px 1fr", None, 0.0, &mut notes), ["240px", "1fr"]);
        assert!(notes.is_empty());
    }

    #[test]
    fn margins_become_a_gap_or_spacers() {
        let out = run("<div><p style=\"margin-bottom:16px\">a</p><p style=\"margin-bottom:16px\">b</p><p>c</p></div>");
        assert!(out.starts_with("<div gap:16px>") && !out.contains("margin"), "{out}");
        let out = run("<div style=\"display:flex\"><span>Title</span><button style=\"margin-left:auto\">Save</button></div>");
        assert!(out.contains("<div flex-grow:1></><button >Save"), "auto margin takes the free space: {out}");
        let out = run("<div><p style=\"margin-bottom:8px\">a</p><p style=\"margin-top:24px\">b</p><p>c</p></div>");
        assert!(out.contains("height:24px") && !out.contains("height:8px"), "block margins collapse: {out}");
    }

    #[test]
    fn inline_boxes_and_inputs_become_boxes() {
        let out = run("<div><span style=\"padding:4px\">A</span><span style=\"padding:4px\">B</span></div>");
        assert!(out.starts_with("<div display:flex;flex-wrap:wrap") && out.contains("<div padding:4px>A"), "{out}");
        let out = run("<input placeholder=\"you@example.com\" style=\"padding:8px\"/>");
        assert!(out.contains("<span color:#757575>you@example.com"), "{out}");
    }
}
