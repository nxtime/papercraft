//! Fixed-function blend states for the canvas pipelines.

use crate::model::BlendMode;

/// GPU pipelines exist for the blend modes fixed-function blending can express.
pub(super) fn blend_key(b: BlendMode) -> usize {
    match b {
        BlendMode::Multiply => 1,
        BlendMode::Screen => 2,
        BlendMode::Darken => 3,
        BlendMode::Lighten => 4,
        _ => 0,
    }
}

pub(super) fn blend_state(key: usize) -> wgpu::BlendState {
    use wgpu::{BlendComponent as C, BlendFactor as F, BlendOperation as O};
    let alpha = C {
        src_factor: F::One,
        dst_factor: F::OneMinusSrcAlpha,
        operation: O::Add,
    };
    let color = match key {
        1 => C {
            src_factor: F::Dst,
            dst_factor: F::OneMinusSrcAlpha,
            operation: O::Add,
        },
        2 => C {
            src_factor: F::One,
            dst_factor: F::OneMinusSrc,
            operation: O::Add,
        },
        3 => C {
            src_factor: F::One,
            dst_factor: F::One,
            operation: O::Min,
        },
        4 => C {
            src_factor: F::One,
            dst_factor: F::One,
            operation: O::Max,
        },
        _ => alpha,
    };
    wgpu::BlendState { color, alpha }
}
