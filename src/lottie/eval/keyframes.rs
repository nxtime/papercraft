//! Bounded temporal easing and property interpolation.

use crate::lottie::model::{Bezier, PathTrack, Track};

impl Track {
    pub(crate) fn constant(values: &[f32]) -> Self {
        Self {
            keys: vec![crate::lottie::model::Key {
                time: 0.0,
                value: values.to_vec(),
                incoming: vec![[1.0, 1.0]],
                outgoing: vec![[0.0, 0.0]],
                hold: false,
            }],
        }
    }

    pub(crate) fn at(&self, frame: f32) -> Vec<f32> {
        let Some(first) = self.keys.first() else {
            return Vec::new();
        };
        let mut previous = first;
        for next in self.keys.iter().skip(1) {
            if frame < next.time {
                if previous.hold || frame <= previous.time {
                    return previous.value.clone();
                }
                let progress = (frame - previous.time) / (next.time - previous.time);
                return previous
                    .value
                    .iter()
                    .zip(&next.value)
                    .enumerate()
                    .map(|(dimension, (&start, &end))| {
                        let outgoing = previous
                            .outgoing
                            .get(dimension)
                            .or_else(|| previous.outgoing.first())
                            .copied()
                            .unwrap_or([0.0, 0.0]);
                        let incoming = next
                            .incoming
                            .get(dimension)
                            .or_else(|| next.incoming.first())
                            .copied()
                            .unwrap_or([1.0, 1.0]);
                        start + (end - start) * ease(progress, outgoing, incoming)
                    })
                    .collect();
            }
            previous = next;
        }
        previous.value.clone()
    }

    pub(crate) fn scalar(&self, frame: f32) -> f32 {
        self.at(frame).first().copied().unwrap_or(0.0)
    }

    pub(crate) fn bounds(&self) -> Vec<[f32; 2]> {
        let Some(first) = self.keys.first() else {
            return Vec::new();
        };
        let mut bounds: Vec<_> = first.value.iter().map(|&value| [value, value]).collect();
        for key in &self.keys {
            for (bound, &value) in bounds.iter_mut().zip(&key.value) {
                bound[0] = bound[0].min(value);
                bound[1] = bound[1].max(value);
            }
        }
        for pair in self.keys.windows(2) {
            let Some((previous, tail)) = pair.split_first() else {
                continue;
            };
            let Some(next) = tail.first() else { continue };
            if previous.hold {
                continue;
            }
            for (dimension, ((bound, &start), &end)) in bounds
                .iter_mut()
                .zip(&previous.value)
                .zip(&next.value)
                .enumerate()
            {
                let outgoing = previous
                    .outgoing
                    .get(dimension)
                    .or_else(|| previous.outgoing.first())
                    .copied()
                    .unwrap_or([0.0, 0.0]);
                let incoming = next
                    .incoming
                    .get(dimension)
                    .or_else(|| next.incoming.first())
                    .copied()
                    .unwrap_or([1.0, 1.0]);
                for weight in [
                    0.0_f32.min(outgoing[1]).min(incoming[1]),
                    1.0_f32.max(outgoing[1]).max(incoming[1]),
                ] {
                    let value = start + (end - start) * weight;
                    bound[0] = bound[0].min(value);
                    bound[1] = bound[1].max(value);
                }
            }
        }
        bounds
    }

    pub(crate) fn magnitude(&self) -> f32 {
        self.bounds()
            .iter()
            .flatten()
            .fold(0.0_f32, |maximum, value| maximum.max(value.abs()))
    }
}

impl PathTrack {
    pub(crate) fn at(&self, frame: f32) -> Bezier {
        let values = self.track.at(frame);
        let mut path = Bezier {
            closed: self.closed,
            vertices: Vec::new(),
            incoming: Vec::new(),
            outgoing: Vec::new(),
        };
        for vertex in values.as_chunks::<6>().0 {
            let [x, y, ix, iy, ox, oy] = *vertex;
            path.vertices.push([x, y]);
            path.incoming.push([ix, iy]);
            path.outgoing.push([ox, oy]);
        }
        path
    }
}

fn cubic(parameter: f32, first: f32, second: f32) -> f32 {
    let inverse = 1.0 - parameter;
    3.0 * inverse * inverse * parameter * first
        + 3.0 * inverse * parameter * parameter * second
        + parameter * parameter * parameter
}

fn ease(progress: f32, outgoing: [f32; 2], incoming: [f32; 2]) -> f32 {
    let mut lower = 0.0;
    let mut upper = 1.0;
    // Bisection remains stable for flat derivatives and all monotone timing curves.
    for _ in 0..24 {
        let middle = f32::midpoint(lower, upper);
        if cubic(middle, outgoing[0], incoming[0]) < progress {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    cubic(f32::midpoint(lower, upper), outgoing[1], incoming[1])
}

#[cfg(test)]
mod tests {
    use crate::lottie::model::{Key, Track};

    #[test]
    fn easing_and_hold_use_adjacent_keys() {
        let first = Key {
            time: 0.0,
            value: vec![0.0],
            incoming: vec![[0.0, 0.0]],
            outgoing: vec![[0.5, 0.0]],
            hold: false,
        };
        let last = Key {
            time: 10.0,
            value: vec![10.0],
            incoming: vec![[0.5, 1.0]],
            outgoing: vec![[1.0, 1.0]],
            hold: false,
        };
        let mut track = Track {
            keys: vec![first, last],
        };
        assert!((track.scalar(5.0) - 5.0).abs() < 0.001);
        if let Some(first) = track.keys.first_mut() {
            first.hold = true;
        }
        assert_eq!(track.scalar(9.0), 0.0);
        assert_eq!(track.scalar(10.0), 10.0);
    }
}
