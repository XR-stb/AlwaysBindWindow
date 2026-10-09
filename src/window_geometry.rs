/// Clamp only enough to leave the title bar reachable; do not force windows
/// fully inside a monitor or destroy intentional multi-monitor layouts.
pub fn reachable_position(
    x: i32,
    y: i32,
    width: i32,
    grip: i32,
    caption: i32,
    work: (i32, i32, i32, i32),
) -> (i32, i32) {
    let (left, top, right, bottom) = work;
    let width = width.max(1);
    let grip = grip.max(1).min(width).min((right - left).max(1));
    let min_x = left - width + grip;
    let max_x = (right - grip).max(min_x);
    (
        x.clamp(min_x, max_x),
        y.clamp(top, (bottom - caption).max(top)),
    )
}

#[cfg(test)]
mod tests {
    use super::reachable_position as clamp;

    #[test]
    fn preserves_reachable_and_partially_offscreen_windows() {
        assert_eq!(clamp(100, 200, 800, 96, 32, (0, 0, 1920, 1040)), (100, 200));
        assert_eq!(
            clamp(-600, 200, 800, 96, 32, (0, 0, 1920, 1040)),
            (-600, 200)
        );
    }

    #[test]
    fn rescues_all_four_edges() {
        let work = (0, 40, 1920, 1040);
        assert_eq!(clamp(-900, 100, 800, 96, 32, work), (-704, 100));
        assert_eq!(clamp(2100, 100, 800, 96, 32, work), (1824, 100));
        assert_eq!(clamp(100, -200, 800, 96, 32, work), (100, 40));
        assert_eq!(clamp(100, 1400, 800, 96, 32, work), (100, 1008));
    }

    #[test]
    fn supports_negative_monitors_high_dpi_and_small_work_areas() {
        assert_eq!(
            clamp(-1800, -1000, 800, 192, 64, (-1920, -1080, 0, 0)),
            (-1800, -1000)
        );
        assert_eq!(clamp(3000, 2000, 40, 192, 64, (0, 0, 80, 40)), (40, 0));
    }
}
