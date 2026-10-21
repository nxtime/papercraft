//! Strict property decoding and numeric boundary validation.

use crate::lottie::Result;
use crate::lottie::error::{invalid, unsupported};
use crate::lottie::model::{Key, Position, Track, Transform};
use serde_json::{Map, Value};

const NUMBER_LIMIT: f32 = 1_000_000.0;

pub(crate) fn object<'a>(value: &'a Value, at: &str) -> Result<&'a Map<String, Value>> {
    value.as_object().ok_or_else(|| invalid("object", at))
}

pub(crate) fn array<'a>(value: &'a Value, at: &str) -> Result<&'a Vec<Value>> {
    value.as_array().ok_or_else(|| invalid("array", at))
}

pub(crate) fn required<'a>(value: &'a Value, key: &str, at: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| invalid(format!("missing {key}"), at))
}

pub(crate) fn number(value: &Value, at: &str) -> Result<f32> {
    if !value.is_number() {
        return Err(invalid("number", at));
    }
    let number = value
        .to_string()
        .parse::<f32>()
        .map_err(|_| invalid("finite number", at))?;
    if !number.is_finite() || number.abs() > NUMBER_LIMIT {
        return Err(invalid("number out of range", at));
    }
    Ok(number)
}

pub(crate) fn num(value: &Value, key: &str, default: f32, at: &str) -> Result<f32> {
    value
        .get(key)
        .map_or(Ok(default), |value| number(value, &format!("{at}.{key}")))
}

pub(crate) fn integer(value: &Value, at: &str) -> Result<i64> {
    value.as_i64().ok_or_else(|| invalid("integer", at))
}

pub(crate) fn flag(value: &Value, at: &str) -> Result<bool> {
    value.as_bool().ok_or_else(|| invalid("boolean", at))
}

pub(crate) fn fields(value: &Value, allowed: &[&str], at: &str) -> Result<()> {
    for key in object(value, at)?.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(unsupported(key, at));
        }
    }
    Ok(())
}

pub(crate) fn values(value: &Value, dimensions: usize, at: &str) -> Result<Vec<f32>> {
    if dimensions == 1 && value.is_number() {
        return Ok(vec![number(value, at)?]);
    }
    let components = array(value, at)?;
    if components.len() != dimensions {
        return Err(invalid("component count", at));
    }
    components
        .iter()
        .map(|component| number(component, at))
        .collect()
}

fn plane(value: &Value, neutral: f32, at: &str) -> Result<Vec<f32>> {
    let components = array(value, at)?;
    if ![2, 3].contains(&components.len()) {
        return Err(invalid("2D vector component count", at));
    }
    if components.len() == 3 {
        let z = number(
            components
                .last()
                .ok_or_else(|| invalid("z component", at))?,
            at,
        )?;
        if (z - neutral).abs() > 0.0 {
            return Err(unsupported("3D vector component", at));
        }
    }
    components
        .iter()
        .take(2)
        .map(|component| number(component, at))
        .collect()
}

pub(crate) fn track(value: &Value, dimensions: usize, at: &str, spatial: bool) -> Result<Track> {
    read_track(value, at, spatial, |value, at| {
        values(value, dimensions, at)
    })
}

pub(crate) fn read_track(
    value: &Value,
    at: &str,
    spatial: bool,
    mut read: impl FnMut(&Value, &str) -> Result<Vec<f32>>,
) -> Result<Track> {
    fields(value, &["a", "k", "ix", "l"], at)?;
    let animated = value.get("a").map_or(Ok(0), |value| integer(value, at))?;
    let data = required(value, "k", at)?;
    if animated == 0 {
        return Ok(Track::constant(&read(data, at)?));
    }
    if animated != 1 {
        return Err(invalid("animation flag", at));
    }
    let frames = array(data, at)?;
    if frames.is_empty() {
        return Err(invalid("empty keyframes", at));
    }
    let mut keys: Vec<Key> = Vec::with_capacity(frames.len());
    let mut endpoint: Option<Vec<f32>> = None;
    for (index, frame) in frames.iter().enumerate() {
        let location = format!("{at}.key[{index}]");
        fields(
            frame,
            &["t", "s", "e", "i", "o", "h", "ti", "to"],
            &location,
        )?;
        let time = number(required(frame, "t", &location)?, &location)?;
        let components = match frame.get("s") {
            Some(value) => read(value, &location)?,
            None if index.saturating_add(1) == frames.len() => endpoint
                .clone()
                .ok_or_else(|| invalid("missing terminal value", &location))?,
            None => return Err(invalid("missing s", &location)),
        };
        if let Some(expected) = endpoint.take()
            && expected
                .iter()
                .map(|value| value.to_bits())
                .ne(components.iter().map(|value| value.to_bits()))
        {
            return Err(unsupported("discontinuous keyframe endpoint", &location));
        }
        endpoint = frame
            .get("e")
            .map(|value| read(value, &location))
            .transpose()?;
        if let Some(previous) = keys.last()
            && (time <= previous.time || components.len() != previous.value.len())
        {
            return Err(invalid("keyframe order or component count", &location));
        }
        for tangent in ["ti", "to"] {
            if let Some(value) = frame.get(tangent) {
                if !spatial {
                    return Err(unsupported(format!("spatial tangent {tangent}"), &location));
                }
                if components.len() == 2 {
                    plane(value, 0.0, &location)?;
                } else {
                    values(value, components.len(), &location)?;
                }
            }
        }
        let hold = match num(frame, "h", 0.0, &location)? {
            0.0 => false,
            1.0 => true,
            _ => return Err(invalid("hold flag", &location)),
        };
        let incoming = control(frame.get("i"), [1.0, 1.0], components.len(), &location)?;
        let outgoing = control(frame.get("o"), [0.0, 0.0], components.len(), &location)?;
        keys.push(Key {
            time,
            value: components,
            incoming,
            outgoing,
            hold,
        });
    }
    if endpoint.is_some() {
        return Err(unsupported(
            "terminal endpoint without a following keyframe",
            at,
        ));
    }
    Ok(Track { keys })
}

fn control(
    value: Option<&Value>,
    default: [f32; 2],
    dimensions: usize,
    at: &str,
) -> Result<Vec<[f32; 2]>> {
    let Some(value) = value else {
        return Ok(vec![default]);
    };
    fields(value, &["x", "y"], at)?;
    let axis = |key: &str| -> Result<Vec<f32>> {
        let value = required(value, key, at)?;
        if value.is_number() {
            return Ok(vec![number(value, at)?]);
        }
        let values = array(value, at)?;
        if values.len() != 1 && values.len() != dimensions {
            return Err(invalid("easing component count", at));
        }
        values.iter().map(|value| number(value, at)).collect()
    };
    let xs = axis("x")?;
    let ys = axis("y")?;
    if xs.is_empty() || ys.is_empty() {
        return Err(invalid("empty easing", at));
    }
    let mut points = Vec::new();
    for dimension in 0..xs.len().max(ys.len()) {
        let x = xs
            .get(dimension)
            .or_else(|| xs.first())
            .copied()
            .ok_or_else(|| invalid("easing x", at))?;
        let y = ys
            .get(dimension)
            .or_else(|| ys.first())
            .copied()
            .ok_or_else(|| invalid("easing y", at))?;
        if !(0.0..=1.0).contains(&x) {
            return Err(invalid("easing x outside 0..1", at));
        }
        points.push([x, y]);
    }
    Ok(points)
}

pub(crate) fn property(
    value: &Value,
    key: &str,
    default: &[f32],
    at: &str,
    spatial: bool,
) -> Result<Track> {
    value.get(key).map_or_else(
        || Ok(Track::constant(default)),
        |value| track(value, default.len(), &format!("{at}.{key}"), spatial),
    )
}

fn vector(
    value: &Value,
    key: &str,
    default: &[f32],
    neutral: f32,
    at: &str,
    spatial: bool,
) -> Result<Track> {
    value.get(key).map_or_else(
        || Ok(Track::constant(default)),
        |value| {
            read_track(value, &format!("{at}.{key}"), spatial, |value, at| {
                plane(value, neutral, at)
            })
        },
    )
}

pub(crate) fn range(track: &Track, lower: f32, upper: f32, at: &str) -> Result<()> {
    if track.bounds().iter().any(|&[minimum, maximum]| {
        minimum < lower || maximum > upper || !minimum.is_finite() || !maximum.is_finite()
    }) {
        return Err(invalid(format!("property outside {lower}..{upper}"), at));
    }
    Ok(())
}

pub(crate) fn transform(value: &Value, at: &str, group: bool) -> Result<Transform> {
    let mut allowed = vec!["a", "p", "s", "r", "o", "ix"];
    if group {
        allowed.extend(["ty", "nm", "mn", "hd", "np", "cix"]);
    }
    fields(value, &allowed, at)?;
    let position = match value.get("p") {
        Some(position) if position.get("s").and_then(Value::as_bool) == Some(true) => {
            fields(position, &["s", "x", "y", "ix"], at)?;
            Position::Split(
                track(required(position, "x", at)?, 1, at, true)?,
                track(required(position, "y", at)?, 1, at, true)?,
            )
        }
        _ => Position::Combined(vector(value, "p", &[0.0, 0.0], 0.0, at, true)?),
    };
    let opacity = property(value, "o", &[100.0], at, false)?;
    range(&opacity, 0.0, 100.0, at)?;
    Ok(Transform {
        anchor: vector(value, "a", &[0.0, 0.0], 0.0, at, false)?,
        position,
        scale: vector(value, "s", &[100.0, 100.0], 100.0, at, false)?,
        rotation: property(value, "r", &[0.0], at, false)?,
        opacity,
    })
}
