use crate::config::HotzoneId;
use crate::platform::{Monitor, Point, Rect};

const EDGE_LENGTH_RATIO: f32 = 0.4;

pub fn hotzone_rect(id: HotzoneId, bounds: Rect, edge_size: i32) -> Rect {
    let edge = edge_size.max(1);
    let horizontal_length = ((bounds.width() as f32) * EDGE_LENGTH_RATIO).round() as i32;
    let vertical_length = ((bounds.height() as f32) * EDGE_LENGTH_RATIO).round() as i32;
    let horizontal_start = bounds.left + (bounds.width() - horizontal_length) / 2;
    let vertical_start = bounds.top + (bounds.height() - vertical_length) / 2;

    match id {
        HotzoneId::TopLeft => Rect {
            left: bounds.left,
            top: bounds.top,
            right: bounds.left + edge,
            bottom: bounds.top + edge,
        },
        HotzoneId::Top => Rect {
            left: horizontal_start,
            top: bounds.top,
            right: horizontal_start + horizontal_length,
            bottom: bounds.top + edge,
        },
        HotzoneId::TopRight => Rect {
            left: bounds.right - edge,
            top: bounds.top,
            right: bounds.right,
            bottom: bounds.top + edge,
        },
        HotzoneId::Right => Rect {
            left: bounds.right - edge,
            top: vertical_start,
            right: bounds.right,
            bottom: vertical_start + vertical_length,
        },
        HotzoneId::BottomRight => Rect {
            left: bounds.right - edge,
            top: bounds.bottom - edge,
            right: bounds.right,
            bottom: bounds.bottom,
        },
        HotzoneId::Bottom => Rect {
            left: horizontal_start,
            top: bounds.bottom - edge,
            right: horizontal_start + horizontal_length,
            bottom: bounds.bottom,
        },
        HotzoneId::BottomLeft => Rect {
            left: bounds.left,
            top: bounds.bottom - edge,
            right: bounds.left + edge,
            bottom: bounds.bottom,
        },
        HotzoneId::Left => Rect {
            left: bounds.left,
            top: vertical_start,
            right: bounds.left + edge,
            bottom: vertical_start + vertical_length,
        },
    }
}

pub fn detect_hotzone(cursor: Point, monitors: &[Monitor], edge_size: i32) -> Option<HotzoneId> {
    let monitor = monitors
        .iter()
        .find(|monitor| monitor.bounds.contains(cursor))?;
    [
        HotzoneId::TopLeft,
        HotzoneId::TopRight,
        HotzoneId::BottomRight,
        HotzoneId::BottomLeft,
        HotzoneId::Top,
        HotzoneId::Right,
        HotzoneId::Bottom,
        HotzoneId::Left,
    ]
    .into_iter()
    .find(|id| hotzone_rect(*id, monitor.bounds, edge_size).contains(cursor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Rect;

    fn monitor() -> Monitor {
        Monitor {
            bounds: Rect {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
            work_area: Rect {
                left: 0,
                top: 0,
                right: 100,
                bottom: 100,
            },
            primary: true,
            device_id: [0; 128],
        }
    }

    #[test]
    fn detects_corner_before_edge() {
        assert_eq!(
            detect_hotzone(Point { x: 2, y: 2 }, &[monitor()], 8),
            Some(HotzoneId::TopLeft)
        );
    }

    #[test]
    fn detects_edges() {
        assert_eq!(
            detect_hotzone(Point { x: 50, y: 2 }, &[monitor()], 8),
            Some(HotzoneId::Top)
        );
        assert_eq!(
            detect_hotzone(Point { x: 98, y: 50 }, &[monitor()], 8),
            Some(HotzoneId::Right)
        );
    }

    #[test]
    fn ignores_center() {
        assert_eq!(
            detect_hotzone(Point { x: 50, y: 50 }, &[monitor()], 8),
            None
        );
    }

    #[test]
    fn edge_hotzones_use_the_same_centered_forty_percent_as_the_hint() {
        assert_eq!(
            hotzone_rect(HotzoneId::Top, monitor().bounds, 8),
            Rect {
                left: 30,
                top: 0,
                right: 70,
                bottom: 8,
            }
        );
        assert_eq!(detect_hotzone(Point { x: 20, y: 2 }, &[monitor()], 8), None);
        assert_eq!(
            detect_hotzone(Point { x: 50, y: 2 }, &[monitor()], 8),
            Some(HotzoneId::Top)
        );
    }
}
