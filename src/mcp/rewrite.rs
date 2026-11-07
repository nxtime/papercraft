//! Rewrites the CSS that agents write by habit but layers cannot hold into markup that builds the
//! same layout, before `write_html` makes layers: grids become rows of equal cells, margins
//! become gaps or spacers, inline boxes (chips, badges) become boxes, and text inputs show their
//! placeholder. Each rewrite matches what a browser shows; what cannot be matched is reported.

use std::collections::BTreeSet;

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
    unmapped_container(&d, notes);
    d.retain(|(k, _)| !(k.starts_with("grid") || ["display", "gap", "row-gap", "column-gap", "justify-items"].contains(&k.as_str())));
    d.extend([("display".into(), "flex".into()), ("flex-direction".into(), "column".into()), ("gap".into(), format!("{row_gap}px"))]);
    set_decls(e, &d);
    let cells: Vec<Html> = std::mem::take(&mut e.children).into_iter().filter(|c| !matches!(c, Html::Text(t) if t.trim().is_empty())).collect();
    if cells.is_empty() {
        return;
    }
    let n = tracks.len().max(1);
    let mut unmapped = BTreeSet::new();
    // An empty cell keeps the columns of a short row aligned.
    let empty = |t: &String| {
        let style = cell_style(std::slice::from_ref(t), col_gap);
        (!style.is_empty()).then(|| div(&style, "Empty", vec![]))
    };
    let mut rows: Vec<Vec<Html>> = vec![vec![]];
    let mut used = 0;
    for cell in cells {
        let span = match &cell {
            Html::Element(k) => get(&decls(k), "grid-column").and_then(|v| column_span(v, n)).unwrap_or(1),
            Html::Text(_) => 1,
        };
        if used + span > n {
            if let Some(r) = rows.last_mut() {
                r.extend(tracks[used..].iter().filter_map(empty));
            }
            rows.push(vec![]);
            used = 0;
        }
        let style = cell_style(&tracks[used..used + span], col_gap);
        let cell = match cell {
            Html::Element(mut k) => {
                let mut kd = decls(&k);
                unmapped.extend(kd.iter().filter(|(p, _)| p.starts_with("grid") && p != "grid-column").map(|(p, _)| p.clone()));
                kd.retain(|(p, _)| !p.starts_with("grid"));
                if get(&kd, "width").is_none() {
                    kd.extend(style.split_once(':').map(|(a, b)| (a.to_owned(), b.to_owned())));
                }
                set_decls(&mut k, &kd);
                Html::Element(k)
            }
            t => div(&style, "Cell", vec![t]),
        };
        if let Some(r) = rows.last_mut() {
            r.push(cell);
        }
        used += span;
    }
    if let Some(r) = rows.last_mut() {
        r.extend(tracks[used.min(n)..].iter().filter_map(empty));
    }
    if !unmapped.is_empty() {
        notes.push(format!("grid {} not supported: cells flow row by row", unmapped.into_iter().collect::<Vec<_>>().join(", ")));
    }
    e.children = rows
        .into_iter()
        .enumerate()
        .map(|(r, kids)| div(&format!("display:flex;gap:{col_gap}px"), &format!("Row {}", r + 1), kids))
        .collect();
}

/// `grid-template-rows`, areas and auto-flow have no layer equivalent.
fn unmapped_container(d: &[(String, String)], notes: &mut Vec<String>) {
    let lost: Vec<&str> = d.iter().map(|(k, _)| k.as_str()).filter(|k| k.starts_with("grid") && !matches!(*k, "grid-template-columns" | "grid-column")).collect();
    if !lost.is_empty() {
        notes.push(format!("grid {} not supported: cells flow row by row", lost.join(", ")));
    }
}

/// The style that gives a cell spanning `tracks` their width: `fr` tracks add their shares, px
/// tracks add their widths and the gaps between them.
fn cell_style(tracks: &[String], gap: f64) -> String {
    let fr: Vec<f64> = tracks.iter().filter_map(|t| t.strip_suffix("fr").and_then(|f| f.trim().parse().ok())).collect();
    if !fr.is_empty() {
        return format!("flex-grow:{}", fr.iter().sum::<f64>());
    }
    let w: Vec<f64> = tracks.iter().filter_map(|t| px(t)).collect();
    if w.is_empty() { String::new() } else { format!("width:{}px", w.iter().sum::<f64>() + gap * (w.len() - 1) as f64) }
}

/// Columns a `grid-column` value covers: `span 2`, `1 / 3` or `1 / -1`, at most `max`.
fn column_span(v: &str, max: usize) -> Option<usize> {
    let span = if let Some((_, n)) = v.rsplit_once("span") {
        n.trim().parse().ok()?
    } else {
        let (a, b) = v.split_once('/')?;
        let (a, b): (i64, i64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
        if b < 0 { max as i64 + b + 2 - a } else { b - a }
    };
    usize::try_from(span).ok().filter(|s| *s >= 1).map(|s| s.min(max))
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

/// An element's padding as top, right, bottom, left when every part is a px length.
fn padding(d: &[(String, String)]) -> Option<[f64; 4]> {
    let mut p = [0.0; 4];
    for (k, v) in d.iter().filter(|(k, _)| k.starts_with("padding")) {
        let parts: Vec<f64> = v.split_whitespace().map(px).collect::<Option<_>>()?;
        match (k.as_str(), parts.as_slice()) {
            ("padding", [a]) => p = [*a; 4],
            ("padding", [a, b]) => p = [*a, *b, *a, *b],
            ("padding", [a, b, c]) => p = [*a, *b, *c, *b],
            ("padding", [a, b, c, l]) => p = [*a, *b, *c, *l],
            ("padding-top", [a]) => p[0] = *a,
            ("padding-right", [a]) => p[1] = *a,
            ("padding-bottom", [a]) => p[2] = *a,
            ("padding-left", [a]) => p[3] = *a,
            _ => return None,
        }
    }
    Some(p)
}

/// Layers cannot overlap by pulling a sibling in, so a negative margin is dropped, with a note.
fn drop_negative(k: &mut Element, notes: &mut Vec<String>) {
    let mut d = decls(k);
    let mut found = false;
    for (_, v) in d.iter_mut().filter(|(n, _)| n.starts_with("margin")) {
        let parts: Vec<&str> = v.split_whitespace().collect();
        if parts.iter().any(|t| px(t).is_some_and(|x| x < 0.0)) {
            found = true;
            *v = parts.iter().map(|t| if px(t).is_some_and(|x| x < 0.0) { "0" } else { t }).collect::<Vec<_>>().join(" ");
        }
    }
    if found {
        notes.push(format!("negative margin dropped on `{}`", k.attr("layer-name").unwrap_or(&k.tag)));
        set_decls(k, &d);
    }
}

/// Children's margins along the parent's flow become its gap when they are all one size, else
/// spacers of their size; the margins at either end become the parent's padding; `auto` margins become growing spacers (they take the free space). A
/// block's `margin: 0 auto` centres it.
fn margins(e: &mut Element, notes: &mut Vec<String>) {
    for c in e.children.iter_mut() {
        if let Html::Element(k) = c {
            drop_negative(k, notes);
        }
    }
    let d = decls(e);
    let row = is_row(&d);
    let flex = matches!(get(&d, "display"), Some("flex" | "inline-flex"));
    let gap = get(&d, "gap").and_then(|g| g.split_whitespace().next().and_then(px)).unwrap_or(0.0);
    // An absolute box is out of the flow: its margin offsets it (see `margin.rs`).
    let in_flow = |k: &Element| !matches!(get(&decls(k), "position"), Some("absolute" | "fixed"));
    let kids: Vec<usize> = e.children.iter().enumerate().filter(|(_, c)| matches!(c, Html::Element(k) if in_flow(k))).map(|(i, _)| i).collect();
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
    // Margins at either end of the flow pad the parent, when its padding is plain px.
    let (lead, trail) = (space[0], space[m.len()]);
    let pad = padding(&d).filter(|_| lead > 0.0 || trail > 0.0);
    // Without that padding the end margins need spacers, which the uniform gap does not make.
    let uniform = !between.is_empty() && between.iter().all(|s| (s - between[0]).abs() < 1e-9) && (pad.is_some() || (lead == 0.0 && trail == 0.0));
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
        if !uniform && extra > 0.0 && !(j == 0 && pad.is_some()) {
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
        } else {
            // The builder lays the rest out as padding around the box and `auto` alignment.
            let (a, z) = if row { ("top", "bottom") } else { ("left", "right") };
            for (name, side) in [(a, cs), (z, ce)] {
                match side {
                    Some(None) => kd.push((format!("margin-{name}"), "auto".into())),
                    Some(Some(v)) if v > 0.0 => kd.push((format!("margin-{name}"), format!("{v}px"))),
                    _ => {}
                }
            }
        }
        set_decls(&mut k, &kd);
        out.push(Html::Element(k));
        if auto(end) {
            out.push(grow());
        }
    }
    if !uniform && trail > 0.0 && pad.is_none() {
        out.push(spacer(trail));
    }
    let gapped = uniform && between[0] > 0.0;
    if gapped {
        pd.retain(|(k, _)| k != "gap");
        pd.push(("gap".into(), format!("{}px", gap + between[0])));
    }
    if let Some(mut p) = pad {
        let (s, t) = if row { (3, 1) } else { (0, 2) };
        p[s] += lead;
        p[t] += trail;
        pd.retain(|(k, _)| !k.starts_with("padding"));
        pd.push(("padding".into(), format!("{}px {}px {}px {}px", p[0], p[1], p[2], p[3])));
    }
    if gapped || pad.is_some() {
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
    fn margins_across_the_flow_and_on_absolute_boxes_are_kept_for_the_builder() {
        let out = run("<div><p style=\"margin:0 12px\">a</p><p style=\"position:absolute;margin-top:6px\">b</p></div>");
        assert!(out.contains("margin-left:12px") && out.contains("margin-right:12px"), "{out}");
        assert!(out.contains("margin-top:6px") && !out.contains("Spacer"), "{out}");
    }

    #[test]
    fn outer_margins_pad_the_parent_and_auto_centres() {
        let out = run("<div style=\"padding:8px\"><p style=\"margin:16px 0\">a</p><p style=\"margin:16px 0\">b</p></div>");
        assert!(out.starts_with("<div gap:16px;padding:24px 8px 24px 8px>") && !out.contains("margin") && !out.contains("Spacer"), "{out}");
        let out = run("<div style=\"display:flex\"><p style=\"margin-left:12px\">a</p></div>");
        assert!(out.contains("padding:0px 0px 0px 12px"), "{out}");
        let out = run("<div><div style=\"width:200px;margin:0 auto\">a</div></div>");
        assert!(out.contains("align-self:center") && !out.contains("margin"), "{out}");
    }

    #[test]
    fn end_margins_survive_a_parent_whose_padding_is_not_plain_px() {
        for pad in ["5%", "0 auto"] {
            let out = run(&format!("<div style=\"padding:{pad}\"><p style=\"margin:16px 0\">a</p><p style=\"margin:16px 0\">b</p></div>"));
            assert!(out.matches("height:16px").count() >= 2 && !out.contains("margin"), "{pad}: {out}");
        }
    }

    #[test]
    fn an_empty_grid_makes_no_rows() {
        let out = run("<div style=\"display:grid;grid-template-columns:1fr 1fr\"></div>");
        assert!(!out.contains("Row 1"), "{out}");
    }

    #[test]
    fn negative_margins_are_dropped_with_a_note() {
        let mut t = super::super::html::parse("<div><p style=\"margin-top:-8px\">a</p></div>");
        let notes = rewrite(&mut t);
        assert_eq!(notes, ["negative margin dropped on `p`"]);
    }

    #[test]
    fn grid_spans_cover_columns_and_the_rest_is_reported() {
        let out = run("<div style=\"display:grid;grid-template-columns:1fr 1fr 2fr;gap:8px\"><p style=\"grid-column:span 2\">a</p><p>b</p><p style=\"grid-column:1 / -1\">c</p></div>");
        assert!(out.contains("<p flex-grow:2>a") && out.contains("<p flex-grow:2>b") && out.contains("<p flex-grow:4>c"), "{out}");
        assert_eq!(out.matches("display:flex;gap:8px").count(), 2, "{out}");
        let mut t = super::super::html::parse("<div style=\"display:grid;grid-template-columns:100px 100px;grid-template-rows:auto\"><p style=\"grid-row:2\">a</p></div>");
        let notes = rewrite(&mut t);
        assert!(notes.iter().any(|n| n.contains("grid-template-rows")) && notes.iter().any(|n| n.contains("grid-row")), "{notes:?}");
    }

    #[test]
    fn inline_boxes_and_inputs_become_boxes() {
        let out = run("<div><span style=\"padding:4px\">A</span><span style=\"padding:4px\">B</span></div>");
        assert!(out.starts_with("<div display:flex;flex-wrap:wrap") && out.contains("<div padding:4px>A"), "{out}");
        let out = run("<input placeholder=\"you@example.com\" style=\"padding:8px\"/>");
        assert!(out.contains("<span color:#757575>you@example.com"), "{out}");
    }
}
