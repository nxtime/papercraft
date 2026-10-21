//! Document and layer parsing with precise feature diagnostics.

use super::properties::{array, fields, flag, integer, num, number, required, transform};
use super::{shapes, validate};
use crate::lottie::error::{invalid, unsupported};
use crate::lottie::model::Layer;
use crate::lottie::{Animation, Error, Marker, Result};
use serde_json::Value;

const MAX_BYTES: usize = 512 * 1024;
const MAX_LAYERS: usize = 64;

pub(crate) fn animation(json: &str) -> Result<Animation> {
    if json.len() > MAX_BYTES {
        return Err(Error::TooLarge);
    }
    let document: Value = serde_json::from_str(json).map_err(|error| Error::Json {
        message: error.to_string(),
    })?;
    fields(
        &document,
        &[
            "v", "fr", "ip", "op", "w", "h", "layers", "markers", "nm", "ddd", "assets",
        ],
        "document",
    )?;
    if num(&document, "ddd", 0.0, "document")? != 0.0 {
        return Err(unsupported("3D (ddd)", "document"));
    }
    if let Some(assets) = document.get("assets")
        && !array(assets, "document.assets")?.is_empty()
    {
        return Err(unsupported("assets", "document"));
    }
    let version = required(&document, "v", "document")?
        .as_str()
        .ok_or_else(|| invalid("version string", "document.v"))?;
    if !version.starts_with("5.") {
        return Err(unsupported("version other than Bodymovin v5", "document.v"));
    }
    let width = number(required(&document, "w", "document")?, "document.w")?;
    let height = number(required(&document, "h", "document")?, "document.h")?;
    let rate = number(required(&document, "fr", "document")?, "document.fr")?;
    let first = number(required(&document, "ip", "document")?, "document.ip")?;
    let last = number(required(&document, "op", "document")?, "document.op")?;
    if width <= 0.0
        || height <= 0.0
        || rate <= 0.0
        || last <= first
        || !((last - first) / rate).is_finite()
    {
        return Err(invalid(
            "positive dimensions/rate and ordered timeline",
            "document",
        ));
    }
    let source_layers = array(
        required(&document, "layers", "document")?,
        "document.layers",
    )?;
    if source_layers.len() > MAX_LAYERS {
        return Err(unsupported("more than 64 layers", "document.layers"));
    }
    let mut vertices = 0;
    let mut layers = Vec::with_capacity(source_layers.len());
    for (index, value) in source_layers.iter().enumerate() {
        let name = value.get("nm").and_then(Value::as_str).unwrap_or("unnamed");
        let at = format!("layer[{index}] ({name})");
        layers.push(layer(value, index, first, last, &mut vertices, &at)?);
    }
    let mut markers = Vec::new();
    if let Some(source) = document.get("markers") {
        for (index, value) in array(source, "document.markers")?.iter().enumerate() {
            let at = format!("marker[{index}]");
            fields(value, &["cm", "tm", "dr"], &at)?;
            let name = required(value, "cm", &at)?
                .as_str()
                .ok_or_else(|| invalid("marker name", &at))?
                .to_owned();
            if markers.iter().any(|marker: &Marker| marker.name == name) {
                return Err(invalid("duplicate marker name", &at));
            }
            let frame = number(required(value, "tm", &at)?, &at)?;
            let duration = number(required(value, "dr", &at)?, &at)?;
            if duration < 0.0 {
                return Err(invalid("negative marker duration", &at));
            }
            markers.push(Marker {
                name,
                frame,
                duration,
            });
        }
    }
    let animation = Animation {
        width,
        height,
        rate,
        first,
        last,
        layers,
        markers,
    };
    validate::animation(&animation)?;
    Ok(animation)
}

fn layer(
    value: &Value,
    index: usize,
    first: f32,
    last: f32,
    vertices: &mut usize,
    at: &str,
) -> Result<Layer> {
    for key in ["tt", "td", "ef", "masksProperties"] {
        if value.get(key).is_some() {
            return Err(unsupported(key, at));
        }
    }
    if let Some(has_mask) = value.get("hasMask")
        && flag(has_mask, at)?
    {
        return Err(unsupported("hasMask masks", at));
    }
    fields(
        value,
        &[
            "ty", "ks", "ip", "op", "st", "ind", "parent", "hd", "nm", "mn", "ddd", "sr", "shapes",
            "ao", "bm", "hasMask",
        ],
        at,
    )?;
    if num(value, "ddd", 0.0, at)? != 0.0 {
        return Err(unsupported("3D (ddd)", at));
    }
    if !matches!(num(value, "sr", 1.0, at)?, 1.0) {
        return Err(unsupported("time stretch other than 1", at));
    }
    if num(value, "ao", 0.0, at)? != 0.0 {
        return Err(unsupported("auto orientation", at));
    }
    if num(value, "bm", 0.0, at)? != 0.0 {
        return Err(unsupported("blend mode", at));
    }
    let kind = integer(required(value, "ty", at)?, at)?;
    if ![3, 4].contains(&kind) {
        return Err(unsupported(format!("layer type {kind}"), at));
    }
    let id = value.get("ind").map_or_else(
        || i64::try_from(index).map_err(|_| invalid("layer index", at)),
        |value| integer(value, at),
    )?;
    let parent = value
        .get("parent")
        .map(|value| integer(value, at))
        .transpose()?;
    let layer_first = num(value, "ip", first, at)?;
    let layer_last = num(value, "op", last, at)?;
    if layer_last <= layer_first {
        return Err(invalid("layer timing", at));
    }
    let transform = transform(required(value, "ks", at)?, &format!("{at}.ks"), false)?;
    let hidden = value.get("hd").map_or(Ok(false), |value| flag(value, at))?;
    let shapes = match (kind, value.get("shapes")) {
        (4, Some(source)) => shapes::parse(source, 0, vertices, at)?,
        (4, None) => return Err(invalid("missing shapes", at)),
        (_, Some(source)) if !array(source, at)?.is_empty() => {
            return Err(unsupported("shapes on null layer", at));
        }
        _ => Vec::new(),
    };
    Ok(Layer {
        id,
        parent,
        first: layer_first,
        last: layer_last,
        start: num(value, "st", 0.0, at)?,
        hidden,
        transform,
        shapes,
    })
}
