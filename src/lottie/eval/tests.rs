//! Evaluation regression tests.

use super::*;
use crate::lottie::model::{Key, Position, Track};

fn track(values: &[f32]) -> Track {
    Track {
        keys: vec![Key {
            time: 0.0,
            value: values.to_vec(),
            incoming: Vec::new(),
            outgoing: Vec::new(),
            hold: false,
        }],
    }
}

fn identity() -> Transform {
    Transform {
        anchor: track(&[0.0, 0.0]),
        position: Position::Combined(track(&[0.0, 0.0])),
        scale: track(&[100.0, 100.0]),
        rotation: track(&[0.0]),
        opacity: track(&[100.0]),
    }
}

fn rectangle() -> Shape {
    Shape::Rectangle {
        position: track(&[0.0, 0.0]),
        size: track(&[20.0, 10.0]),
        radius: track(&[0.0]),
        reversed: false,
    }
}

fn layer(id: i64, shapes: Vec<Shape>) -> Layer {
    Layer {
        id,
        parent: None,
        first: 0.0,
        last: 20.0,
        start: 0.0,
        hidden: false,
        transform: identity(),
        shapes,
    }
}

fn animation(layers: Vec<Layer>) -> Animation {
    Animation {
        width: 100.0,
        height: 100.0,
        rate: 30.0,
        first: 0.0,
        last: 20.0,
        layers,
        markers: Vec::new(),
    }
}

#[test]
fn transform_applies_anchor_scale_rotation_then_position() {
    let mut input = identity();
    input.anchor = track(&[1.0, 2.0]);
    input.scale = track(&[200.0, 300.0]);
    input.rotation = track(&[90.0]);
    input.position = Position::Combined(track(&[10.0, 20.0]));
    input.opacity = track(&[50.0]);
    let (matrix, opacity) = transform(&input, 0.0);
    let [x, y] = matrix.apply([2.0, 3.0]);
    assert!((x - 7.0).abs() < 0.0001);
    assert!((y - 22.0).abs() < 0.0001);
    assert!((opacity - 0.5).abs() < f32::EPSILON);
}

#[test]
fn group_and_style_opacities_multiply() {
    let mut group = identity();
    group.opacity = track(&[50.0]);
    let mut input = layer(
        1,
        vec![
            Shape::Group(vec![rectangle()], group),
            Shape::Fill {
                opacity: track(&[50.0]),
            },
        ],
    );
    input.transform.opacity = track(&[50.0]);
    let output = frame(&animation(vec![input]), 0.0);
    let Some(Item::Fill(fill)) = output.items.first() else {
        panic!("expected fill");
    };
    assert!((fill.opacity - 0.125).abs() < f32::EPSILON);
}

#[test]
fn later_trim_also_affects_earlier_style() {
    let input = layer(
        1,
        vec![
            rectangle(),
            Shape::Fill {
                opacity: track(&[100.0]),
            },
            Shape::Trim {
                start: track(&[0.0]),
                end: track(&[0.0]),
                offset: track(&[0.0]),
            },
        ],
    );
    assert_eq!(
        frame(&animation(vec![input]), 0.0).items,
        Vec::<Item>::new()
    );
}

#[test]
fn layer_interval_excludes_out_point_and_hidden_layers() {
    let mut input = layer(
        1,
        vec![
            rectangle(),
            Shape::Fill {
                opacity: track(&[100.0]),
            },
        ],
    );
    input.last = 10.0;
    let mut hidden = input.clone();
    hidden.hidden = true;
    assert_eq!(
        frame(&animation(vec![input, hidden]), 10.0).items,
        Vec::<Item>::new()
    );
}

#[test]
fn parent_transform_composes_without_parent_opacity() {
    let mut parent = layer(1, Vec::new());
    parent.transform.position = Position::Combined(track(&[30.0, 40.0]));
    parent.transform.opacity = track(&[0.0]);
    let mut child = layer(
        2,
        vec![
            rectangle(),
            Shape::Fill {
                opacity: track(&[100.0]),
            },
        ],
    );
    child.parent = Some(1);
    let output = frame(&animation(vec![parent, child]), 0.0);
    let Some(Item::Fill(fill)) = output.items.first() else {
        panic!("expected child fill");
    };
    assert!(
        fill.polylines
            .iter()
            .flat_map(|line| &line.points)
            .all(|point| point.first().is_some_and(|x| *x >= 20.0))
    );
    assert!((fill.opacity - 1.0).abs() < f32::EPSILON);
}

#[test]
fn parent_chain_composes_in_validated_order_without_intermediate_overflow() {
    let mut outer = layer(0, Vec::new());
    outer.transform.scale = track(&[1e-42, 1e-42]);
    let mut layers = vec![outer];
    for id in 1..=11 {
        let shapes = if id == 11 {
            vec![
                rectangle(),
                Shape::Fill {
                    opacity: track(&[100.0]),
                },
            ]
        } else {
            Vec::new()
        };
        let mut child = layer(id, shapes);
        child.parent = Some(id - 1);
        child.transform.scale = track(&[1_000_000.0, 1_000_000.0]);
        layers.push(child);
    }
    let animation = animation(layers);
    assert!(crate::lottie::decode::validate::animation(&animation).is_ok());
    let output = frame(&animation, 0.0);
    assert!(output.items.iter().all(|item| {
        match item {
            Item::Fill(fill) => fill
                .polylines
                .iter()
                .flat_map(|line| line.points.iter().flatten())
                .all(|value| value.is_finite()),
            Item::Stroke(stroke) => stroke
                .polylines
                .iter()
                .flat_map(|line| line.points.iter().flatten())
                .all(|value| value.is_finite()),
        }
    }));
}

#[test]
fn later_styles_paint_below_earlier_styles() {
    let input = layer(
        1,
        vec![
            rectangle(),
            Shape::Stroke {
                width: track(&[2.0]),
                opacity: track(&[100.0]),
                cap: crate::lottie::Cap::Butt,
                join: crate::lottie::Join::Miter,
            },
            Shape::Fill {
                opacity: track(&[100.0]),
            },
        ],
    );
    let output = frame(&animation(vec![input]), 0.0);
    assert!(matches!(
        output.items.as_slice(),
        [Item::Fill(_), Item::Stroke(_)]
    ));
}

#[test]
fn simultaneous_trim_applies_percentage_to_each_original_shape() {
    let wider = Shape::Rectangle {
        position: track(&[0.0, 0.0]),
        size: track(&[40.0, 10.0]),
        radius: track(&[0.0]),
        reversed: false,
    };
    let input = layer(
        1,
        vec![
            rectangle(),
            wider,
            Shape::Trim {
                start: track(&[0.0]),
                end: track(&[50.0]),
                offset: track(&[0.0]),
            },
            Shape::Fill {
                opacity: track(&[100.0]),
            },
        ],
    );
    let output = frame(&animation(vec![input]), 0.0);
    let Some(Item::Fill(fill)) = output.items.first() else {
        panic!("expected fill")
    };
    assert_eq!(fill.polylines.len(), 2);
    assert!(fill.polylines.iter().all(|line| {
        line.points
            .last()
            .is_some_and(|[_, y]| (*y - 5.0).abs() < 0.001)
    }));
}
