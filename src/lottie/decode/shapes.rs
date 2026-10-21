//! Shape decoding, topology checks, and nesting limits.

use super::properties::{
    array, fields, flag, num, number, property, range, read_track, required, transform, values,
};
use crate::lottie::error::{invalid, unsupported};
use crate::lottie::model::{PathTrack, Shape};
use crate::lottie::{Cap, Join, Result};
use serde_json::Value;

const MAX_DEPTH: usize = 8;
const MAX_VERTICES: usize = 4096;

pub(crate) fn parse(
    value: &Value,
    depth: usize,
    vertices: &mut usize,
    at: &str,
) -> Result<Vec<Shape>> {
    if depth > MAX_DEPTH {
        return Err(unsupported("shape nesting deeper than 8", at));
    }
    let source = array(value, at)?;
    if let Some((last, children)) = source.split_last()
        && last.get("ty").and_then(Value::as_str) == Some("tr")
    {
        let transform = transform(last, at, true)?;
        let children = parse(
            &Value::Array(children.to_vec()),
            depth.saturating_add(1),
            vertices,
            at,
        )?;
        return Ok(vec![Shape::Group(children, transform)]);
    }
    let mut shapes = Vec::new();
    for (index, value) in source.iter().enumerate() {
        let location = format!("{at}.shape[{index}]");
        let kind = required(value, "ty", &location)?
            .as_str()
            .ok_or_else(|| invalid("shape type", &location))?;
        let shape = match kind {
            "gr" => group(value, depth, vertices, &location)?,
            "sh" => {
                check_fields(value, &["ks", "ind"], &location)?;
                let path = path(required(value, "ks", &location)?, &location)?;
                let count = path.track.keys.first().map_or(0, |key| key.value.len() / 6);
                add_vertices(vertices, count, &location)?;
                Shape::Path(path)
            }
            "rc" => {
                check_fields(value, &["p", "s", "r", "d"], &location)?;
                add_vertices(vertices, 8, &location)?;
                let size = property(value, "s", &[0.0, 0.0], &location, false)?;
                let radius = property(value, "r", &[0.0], &location, false)?;
                range(&size, 0.0, 1_000_000.0, &location)?;
                range(&radius, 0.0, 1_000_000.0, &location)?;
                Shape::Rectangle {
                    position: property(value, "p", &[0.0, 0.0], &location, true)?,
                    size,
                    radius,
                    reversed: direction(value, &location)?,
                }
            }
            "el" => {
                check_fields(value, &["p", "s", "d"], &location)?;
                add_vertices(vertices, 4, &location)?;
                let size = property(value, "s", &[0.0, 0.0], &location, false)?;
                range(&size, 0.0, 1_000_000.0, &location)?;
                Shape::Ellipse {
                    position: property(value, "p", &[0.0, 0.0], &location, true)?,
                    size,
                    reversed: direction(value, &location)?,
                }
            }
            "st" => {
                check_fields(value, &["w", "lc", "lj", "o", "c"], &location)?;
                colour(required(value, "c", &location)?, &location)?;
                let cap = match num(value, "lc", 1.0, &location)? {
                    1.0 => Cap::Butt,
                    2.0 => Cap::Round,
                    3.0 => Cap::Square,
                    _ => return Err(invalid("stroke cap", &location)),
                };
                let join = match num(value, "lj", 1.0, &location)? {
                    1.0 => Join::Miter,
                    2.0 => Join::Round,
                    3.0 => Join::Bevel,
                    _ => return Err(invalid("stroke join", &location)),
                };
                let width = property(value, "w", &[1.0], &location, false)?;
                range(&width, 0.0, 1_000_000.0, &location)?;
                Shape::Stroke {
                    width,
                    opacity: opacity(value, &location)?,
                    cap,
                    join,
                }
            }
            "fl" => {
                check_fields(value, &["o", "c", "r"], &location)?;
                if !matches!(num(value, "r", 1.0, &location)?, 1.0) {
                    return Err(unsupported("even-odd fill rule", &location));
                }
                colour(required(value, "c", &location)?, &location)?;
                Shape::Fill {
                    opacity: opacity(value, &location)?,
                }
            }
            "tm" => {
                check_fields(value, &["s", "e", "o", "m"], &location)?;
                if !matches!(num(value, "m", 1.0, &location)?, 1.0) {
                    return Err(unsupported(
                        "trim mode other than simultaneous (m=1)",
                        &location,
                    ));
                }
                let start = property(value, "s", &[0.0], &location, false)?;
                let end = property(value, "e", &[100.0], &location, false)?;
                range(&start, 0.0, 100.0, &location)?;
                range(&end, 0.0, 100.0, &location)?;
                Shape::Trim {
                    start,
                    end,
                    offset: property(value, "o", &[0.0], &location, false)?,
                }
            }
            "tr" => {
                return Err(unsupported(
                    "transform outside final group position",
                    &location,
                ));
            }
            other => return Err(unsupported(format!("shape {other}"), &location)),
        };
        if !value
            .get("hd")
            .map_or(Ok(false), |value| flag(value, &location))?
        {
            shapes.push(shape);
        }
    }
    Ok(shapes)
}

fn group(value: &Value, depth: usize, vertices: &mut usize, at: &str) -> Result<Shape> {
    check_fields(value, &["it", "np", "cix", "bm"], at)?;
    if num(value, "bm", 0.0, at)? != 0.0 {
        return Err(unsupported("blend mode", at));
    }
    let items = array(required(value, "it", at)?, at)?;
    let Some((last, children)) = items.split_last() else {
        return Err(invalid("group requires final transform", at));
    };
    if last.get("ty").and_then(Value::as_str) != Some("tr") {
        return Err(invalid("group requires final transform", at));
    }
    let transform = transform(last, at, true)?;
    let children = parse(
        &Value::Array(children.to_vec()),
        depth.saturating_add(1),
        vertices,
        at,
    )?;
    Ok(Shape::Group(children, transform))
}

fn check_fields(value: &Value, extra: &[&str], at: &str) -> Result<()> {
    let mut allowed = vec!["ty", "nm", "mn", "ix", "hd"];
    allowed.extend(extra);
    fields(value, &allowed, at)
}

fn direction(value: &Value, at: &str) -> Result<bool> {
    match num(value, "d", 1.0, at)? {
        1.0 | 2.0 => Ok(false),
        3.0 => Ok(true),
        _ => Err(invalid("path direction", at)),
    }
}

fn add_vertices(vertices: &mut usize, count: usize, at: &str) -> Result<()> {
    *vertices = vertices.saturating_add(count);
    if *vertices > MAX_VERTICES {
        return Err(unsupported("more than 4096 vertices", at));
    }
    Ok(())
}

fn opacity(value: &Value, at: &str) -> Result<crate::lottie::model::Track> {
    let opacity = property(value, "o", &[100.0], at, false)?;
    range(&opacity, 0.0, 100.0, at)?;
    Ok(opacity)
}

fn colour(value: &Value, at: &str) -> Result<()> {
    // Lottie exports either RGB or RGBA. Both are validated, then host-tinted.
    let colour = read_track(value, at, false, |value, at| {
        let channels = array(value, at)?;
        if ![3, 4].contains(&channels.len()) {
            return Err(invalid("RGB/RGBA colour", at));
        }
        channels.iter().map(|value| number(value, at)).collect()
    })?;
    range(&colour, 0.0, 1.0, at)
}

fn path(value: &Value, at: &str) -> Result<PathTrack> {
    let mut closed = None;
    let mut count = None;
    let track = read_track(value, at, false, |value, at| {
        let data = if value.is_array() {
            let values = array(value, at)?;
            if values.len() != 1 {
                return Err(invalid("path keyframe", at));
            }
            values.first().ok_or_else(|| invalid("path keyframe", at))?
        } else {
            value
        };
        fields(data, &["c", "v", "i", "o"], at)?;
        let is_closed = flag(required(data, "c", at)?, at)?;
        let vertices = array(required(data, "v", at)?, at)?;
        let incoming = array(required(data, "i", at)?, at)?;
        let outgoing = array(required(data, "o", at)?, at)?;
        if vertices.len() > MAX_VERTICES {
            return Err(unsupported("more than 4096 vertices", at));
        }
        if vertices.is_empty()
            || vertices.len() != incoming.len()
            || vertices.len() != outgoing.len()
        {
            return Err(invalid("path vertex/tangent count", at));
        }
        if closed.is_some_and(|previous| previous != is_closed)
            || count.is_some_and(|previous| previous != vertices.len())
        {
            return Err(invalid("path keyframe topology", at));
        }
        closed = Some(is_closed);
        count = Some(vertices.len());
        let mut coordinates = Vec::with_capacity(vertices.len().saturating_mul(6));
        for ((vertex, incoming), outgoing) in vertices.iter().zip(incoming).zip(outgoing) {
            coordinates.extend(values(vertex, 2, at)?);
            coordinates.extend(values(incoming, 2, at)?);
            coordinates.extend(values(outgoing, 2, at)?);
        }
        Ok(coordinates)
    })?;
    Ok(PathTrack {
        track,
        closed: closed.unwrap_or(false),
    })
}
