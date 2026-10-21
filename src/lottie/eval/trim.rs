//! Arc-length trim paths applied simultaneously across shape contours.

use crate::lottie::Polyline;

/// Trims normalized percentages over the combined length of every contour.
#[cfg(test)]
pub(crate) fn apply(paths: Vec<Polyline>, start: f32, end: f32, offset: f32) -> Vec<Polyline> {
    apply_tagged(paths.into_iter().enumerate().collect(), start, end, offset)
        .into_iter()
        .map(|(_, path)| path)
        .collect()
}

/// Preserves caller tags when trimming splits contours into visible fragments.
pub(crate) fn apply_tagged(
    paths: Vec<(usize, Polyline)>,
    start: f32,
    end: f32,
    offset: f32,
) -> Vec<(usize, Polyline)> {
    let span = (end - start).abs() / 100.0;
    if span >= 1.0 {
        return paths;
    }
    if span == 0.0 {
        return Vec::new();
    }
    let total = paths.iter().map(|(_, path)| length(path)).sum::<f32>();
    if total == 0.0 {
        return Vec::new();
    }
    let start = (start.min(end) / 100.0 + offset / 360.0).rem_euclid(1.0) * total;
    let end = start + span * total;
    let intervals = [(start, end.min(total)), (0.0, (end - total).max(0.0))];
    let mut result = Vec::new();
    let mut position = 0.0;
    for (tag, path) in paths {
        let path_length = length(&path);
        let mut pieces = Vec::new();
        for (from, to) in intervals {
            let from = from.max(position);
            let to = to.min(position + path_length);
            if from < to {
                pieces.push(piece(&path, from - position, to - position));
            }
        }
        if path.closed && pieces.len() == 2 {
            let head = pieces.pop();
            if let (Some(tail), Some(head)) = (pieces.last_mut(), head) {
                tail.points.extend(head.points.into_iter().skip(1));
            }
        }
        result.extend(pieces.into_iter().map(|piece| (tag, piece)));
        position += path_length;
    }
    result
}

fn edges(path: &Polyline) -> impl Iterator<Item = ([f32; 2], [f32; 2])> + '_ {
    let closure = path
        .points
        .first()
        .filter(|first| path.closed && Some(*first) != path.points.last());
    path.points
        .iter()
        .zip(path.points.iter().skip(1).chain(closure))
        .map(|(a, b)| (*a, *b))
}

fn distance([ax, ay]: [f32; 2], [bx, by]: [f32; 2]) -> f32 {
    (bx - ax).hypot(by - ay)
}

fn length(path: &Polyline) -> f32 {
    edges(path).map(|(a, b)| distance(a, b)).sum()
}

fn piece(path: &Polyline, start: f32, end: f32) -> Polyline {
    let mut points = Vec::new();
    let mut position = 0.0;
    for (a, b) in edges(path) {
        let length = distance(a, b);
        let from = start.max(position);
        let to = end.min(position + length);
        if from < to {
            let first = interpolate(a, b, (from - position) / length);
            if points.last() != Some(&first) {
                points.push(first);
            }
            points.push(interpolate(a, b, (to - position) / length));
        }
        position += length;
        if position >= end {
            break;
        }
    }
    Polyline {
        points,
        closed: false,
    }
}

fn interpolate([ax, ay]: [f32; 2], [bx, by]: [f32; 2], t: f32) -> [f32; 2] {
    [ax + (bx - ax) * t, ay + (by - ay) * t]
}

#[cfg(test)]
mod tests {
    use super::apply;
    use crate::lottie::Polyline;

    fn line(start: f32, end: f32) -> Polyline {
        Polyline {
            points: vec![[start, 0.0], [end, 0.0]],
            closed: false,
        }
    }

    #[test]
    fn trims_exactly_by_length_across_paths() {
        let result = apply(vec![line(0.0, 10.0), line(20.0, 50.0)], 12.5, 75.0, 0.0);
        assert_eq!(result.len(), 2);
        assert_eq!(
            result.first().map(|p| &p.points),
            Some(&vec![[5.0, 0.0], [10.0, 0.0]])
        );
        assert_eq!(
            result.last().map(|p| &p.points),
            Some(&vec![[20.0, 0.0], [40.0, 0.0]])
        );
    }

    #[test]
    fn offset_wraps_around_closed_contour() {
        let square = Polyline {
            points: vec![
                [0.0, 0.0],
                [10.0, 0.0],
                [10.0, 10.0],
                [0.0, 10.0],
                [0.0, 0.0],
            ],
            closed: true,
        };
        let result = apply(vec![square], 0.0, 50.0, 270.0);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.first().map(|p| &p.points),
            Some(&vec![[0.0, 10.0], [0.0, 0.0], [10.0, 0.0]])
        );
        assert!(result.iter().all(|p| !p.closed));
    }

    #[test]
    fn full_trim_retains_closed_path_and_zero_trim_removes_it() {
        let mut full = line(0.0, 10.0);
        full.closed = true;
        let result = apply(vec![full], 0.0, 100.0, 23.0);
        assert!(result.first().is_some_and(|p| p.closed));
        assert_eq!(
            apply(vec![line(0.0, 10.0)], 50.0, 50.0, 0.0),
            Vec::<Polyline>::new()
        );
        assert_eq!(apply(Vec::new(), 0.0, 50.0, 0.0), Vec::<Polyline>::new());
    }
}
