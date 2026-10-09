//! Selection semantics independent of the native overlay and monitor origins.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Bounds {
    pub fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

pub fn toggle(selected: &mut Vec<isize>, window: isize) {
    if selected.contains(&window) {
        selected.retain(|id| *id != window);
    } else {
        selected.push(window);
    }
}

pub fn append(selected: &mut Vec<isize>, windows: impl IntoIterator<Item = isize>) {
    for window in windows {
        if !selected.contains(&window) {
            selected.push(window);
        }
    }
}

/// Windows arrive front to back. A click selects only the first hit.
pub fn hit_test(windows: &[(isize, Bounds)], x: i32, y: i32) -> Option<isize> {
    windows
        .iter()
        .find(|(_, bounds)| bounds.contains(x, y))
        .map(|(id, _)| *id)
}

/// Exact rectangle subtraction: hidden windows are not selected by a marquee.
pub fn intersecting(windows: &[(isize, Bounds)], drag: Bounds) -> Vec<isize> {
    let mut selected = Vec::new();
    for (index, (id, bounds)) in windows.iter().enumerate() {
        let rect = Bounds {
            left: bounds.left.max(drag.left),
            top: bounds.top.max(drag.top),
            right: bounds.right.min(drag.right),
            bottom: bounds.bottom.min(drag.bottom),
        };
        if rect.left >= rect.right || rect.top >= rect.bottom {
            continue;
        }
        let mut visible = vec![rect];
        for (_, upper) in &windows[..index] {
            visible = visible
                .into_iter()
                .flat_map(|part| subtract(part, *upper))
                .collect();
            if visible.is_empty() {
                break;
            }
        }
        if !visible.is_empty() {
            selected.push(*id);
        }
    }
    selected
}

fn subtract(rect: Bounds, cover: Bounds) -> Vec<Bounds> {
    let left = rect.left.max(cover.left);
    let top = rect.top.max(cover.top);
    let right = rect.right.min(cover.right);
    let bottom = rect.bottom.min(cover.bottom);
    if left >= right || top >= bottom {
        return vec![rect];
    }
    [
        Bounds {
            bottom: top,
            ..rect
        },
        Bounds {
            top: bottom,
            ..rect
        },
        Bounds {
            top,
            bottom,
            right: left,
            ..rect
        },
        Bounds {
            top,
            bottom,
            left: right,
            ..rect
        },
    ]
    .into_iter()
    .filter(|r| r.left < r.right && r.top < r.bottom)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Bounds {
        Bounds {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn cross_monitor_clicks_toggle_only_topmost_window() {
        let windows = [
            (1, rect(-900, 0, -100, 600)),
            (2, rect(-1000, 0, 0, 700)),
            (3, rect(0, 0, 800, 600)),
        ];
        let mut selected = Vec::new();
        toggle(&mut selected, hit_test(&windows, -500, 100).unwrap());
        toggle(&mut selected, hit_test(&windows, 100, 100).unwrap());
        assert_eq!(selected, [1, 3]);
        toggle(&mut selected, hit_test(&windows, -500, 100).unwrap());
        assert_eq!(selected, [3]);
        assert_eq!(hit_test(&windows, 1000, 100), None);
    }

    #[test]
    fn marquees_accumulate_without_duplicates_and_ignore_hidden_windows() {
        let windows = [
            (1, rect(0, 0, 200, 200)),
            (2, rect(20, 20, 80, 80)),
            (3, rect(180, 0, 300, 200)),
        ];
        let mut selected = vec![9];
        append(&mut selected, intersecting(&windows, rect(0, 0, 100, 100)));
        append(&mut selected, intersecting(&windows, rect(0, 0, 300, 200)));
        assert_eq!(selected, [9, 1, 3]);
        toggle(&mut selected, 1);
        assert_eq!(selected, [9, 3]);
    }
}
