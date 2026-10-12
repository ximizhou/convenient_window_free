use crate::config::{HotzoneGeometry, HotzoneId, HotzoneSetting};
use crate::platform::{Monitor, Point, Rect};

#[cfg(test)]
pub fn hotzone_rect(id: HotzoneId, bounds: Rect, edge_size: i32) -> Rect {
    configured_hotzone_rect(id, bounds, edge_size, None)
}

/// Geometry is presentation-independent: the same rectangle drives input and hints.
pub fn configured_hotzone_rect(
    id: HotzoneId,
    bounds: Rect,
    edge_size: i32,
    geometry: Option<&HotzoneGeometry>,
) -> Rect {
    let screen_width = bounds.width().max(1);
    let screen_height = bounds.height().max(1);
    let fallback = edge_size.clamp(2, 48);
    let (width, height, thickness, percent) = match geometry {
        Some(HotzoneGeometry::Corner { width, height, .. }) => (
            (*width).clamp(2, 128),
            (*height).clamp(2, 128),
            fallback,
            40,
        ),
        Some(HotzoneGeometry::Edge {
            thickness,
            length_percent,
        }) => (
            fallback,
            fallback,
            (*thickness).clamp(2, 48),
            (*length_percent).clamp(10, 100),
        ),
        None => (fallback, fallback, fallback, 40),
    };
    let corner_width = width.min(screen_width);
    let corner_height = height.min(screen_height);
    let horizontal_thickness = thickness.min(screen_height);
    let vertical_thickness = thickness.min(screen_width);
    // Round positive dimensions like the frontend; use i64 to avoid intermediate overflow.
    let horizontal_length =
        (((screen_width as i64 * percent as i64) + 50) / 100).clamp(1, screen_width as i64) as i32;
    let vertical_length = (((screen_height as i64 * percent as i64) + 50) / 100)
        .clamp(1, screen_height as i64) as i32;
    let horizontal_start = bounds.left + (screen_width - horizontal_length) / 2;
    let vertical_start = bounds.top + (screen_height - vertical_length) / 2;
    match id {
        HotzoneId::TopLeft => Rect {
            left: bounds.left,
            top: bounds.top,
            right: bounds.left + corner_width,
            bottom: bounds.top + corner_height,
        },
        HotzoneId::TopRight => Rect {
            left: bounds.right - corner_width,
            top: bounds.top,
            right: bounds.right,
            bottom: bounds.top + corner_height,
        },
        HotzoneId::BottomLeft => Rect {
            left: bounds.left,
            top: bounds.bottom - corner_height,
            right: bounds.left + corner_width,
            bottom: bounds.bottom,
        },
        HotzoneId::BottomRight => Rect {
            left: bounds.right - corner_width,
            top: bounds.bottom - corner_height,
            right: bounds.right,
            bottom: bounds.bottom,
        },
        HotzoneId::Top => Rect {
            left: horizontal_start,
            top: bounds.top,
            right: horizontal_start + horizontal_length,
            bottom: bounds.top + horizontal_thickness,
        },
        HotzoneId::Bottom => Rect {
            left: horizontal_start,
            top: bounds.bottom - horizontal_thickness,
            right: horizontal_start + horizontal_length,
            bottom: bounds.bottom,
        },
        HotzoneId::Left => Rect {
            left: bounds.left,
            top: vertical_start,
            right: bounds.left + vertical_thickness,
            bottom: vertical_start + vertical_length,
        },
        HotzoneId::Right => Rect {
            left: bounds.right - vertical_thickness,
            top: vertical_start,
            right: bounds.right,
            bottom: vertical_start + vertical_length,
        },
    }
}

#[cfg(test)]
pub fn detect_hotzone(cursor: Point, monitors: &[Monitor], edge_size: i32) -> Option<HotzoneId> {
    detect_configured_hotzone(cursor, monitors, edge_size, &[])
}

pub fn detect_configured_hotzone(
    cursor: Point,
    monitors: &[Monitor],
    edge_size: i32,
    zones: &[HotzoneSetting],
) -> Option<HotzoneId> {
    let monitor = monitors
        .iter()
        .find(|monitor| monitor.bounds.contains(cursor))?;
    // Keep corner priority even when an edge is expanded to 100%.
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
    .find(|id| {
        configured_hotzone_rect(
            *id,
            monitor.bounds,
            edge_size,
            zones
                .iter()
                .find(|zone| zone.id == *id)
                .and_then(|zone| zone.geometry.as_ref()),
        )
        .contains(cursor)
    })
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
    fn matches_shared_preview_rectangles_and_normalization() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/hotzone-geometry.json"
        ))
        .unwrap();
        for case in fixture["rectangles"].as_array().unwrap() {
            let id: HotzoneId = serde_json::from_value(case["id"].clone()).unwrap();
            let bounds = &case["bounds"];
            let bounds = Rect {
                left: bounds["left"].as_i64().unwrap() as i32,
                top: bounds["top"].as_i64().unwrap() as i32,
                right: bounds["right"].as_i64().unwrap() as i32,
                bottom: bounds["bottom"].as_i64().unwrap() as i32,
            };
            let geometry: Option<HotzoneGeometry> = case
                .get("geometry")
                .map(|v| serde_json::from_value(v.clone()).unwrap());
            let rect = configured_hotzone_rect(
                id,
                bounds,
                case["edgeSize"].as_i64().unwrap() as i32,
                geometry.as_ref(),
            );
            assert_eq!(
                serde_json::to_value(rect).unwrap(),
                case["expected"],
                "{}",
                case["name"]
            );
        }
        for case in fixture["normalizations"].as_array().unwrap() {
            let id: HotzoneId = serde_json::from_value(case["id"].clone()).unwrap();
            let geometry: HotzoneGeometry = serde_json::from_value(case["input"].clone()).unwrap();
            assert_eq!(
                serde_json::to_value(geometry.normalized(id)).unwrap(),
                case["expected"],
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn full_edges_keep_corner_priority_and_end_exclusion() {
        let mut config = crate::config::AppConfig::default();
        for zone in &mut config.hotzones {
            if zone.id == HotzoneId::Bottom {
                zone.geometry = Some(HotzoneGeometry::Edge {
                    thickness: 16,
                    length_percent: 100,
                });
            }
        }
        assert_eq!(
            detect_configured_hotzone(Point { x: 2, y: 99 }, &[monitor()], 8, &config.hotzones),
            Some(HotzoneId::BottomLeft)
        );
        assert_eq!(
            detect_configured_hotzone(Point { x: 10, y: 99 }, &[monitor()], 8, &config.hotzones),
            Some(HotzoneId::Bottom)
        );
        assert_eq!(
            detect_configured_hotzone(Point { x: 50, y: 100 }, &[monitor()], 8, &config.hotzones),
            None
        );
        assert_eq!(
            detect_configured_hotzone(Point { x: 100, y: 99 }, &[monitor()], 8, &config.hotzones),
            None
        );
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
