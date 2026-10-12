use crate::platform::{Point, Rect, WindowHandle, WindowInfo};
use crate::platform::{WindowDragCapture, WindowDragMode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowDragUpdate {
    pub handle: WindowHandle,
    pub rect: Rect,
    pub finished: bool,
    pub cancelled: bool,
}

#[derive(Clone, Copy, Debug)]
struct DragSession {
    sequence: u64,
    mode: WindowDragMode,
    handle: WindowHandle,
    start_cursor: Point,
    original_rect: Rect,
}

#[derive(Default)]
pub struct WindowDragController {
    session: Option<DragSession>,
    window: Option<WindowInfo>,
    last_rect: Option<Rect>,
    /// 曾被应用名单拦下的本次按下的序号。
    ///
    /// 判定后来前移到了 `platform::draggable_window_at`（必须在摘除最大化状态之前做，
    /// 否则命中名单的窗口已经被摘下来了），因此生产路径不再写入这个字段；
    /// 保留它是为了让控制器层面的拒绝语义仍有测试覆盖。
    #[cfg_attr(not(test), allow(dead_code))]
    rejected_sequence: Option<u64>,
}

impl WindowDragController {
    pub fn start(&mut self, capture: WindowDragCapture, window: &WindowInfo) {
        self.rejected_sequence = None;
        self.window = Some(window.clone());
        self.last_rect = None;
        self.session = Some(DragSession {
            sequence: capture.sequence,
            mode: capture.mode,
            handle: window.handle,
            start_cursor: capture.start,
            original_rect: window.rect,
        });
    }

    /// 记录本次按下已被应用名单拒绝，避免同一 sequence 反复查询目标窗口。
    #[cfg(test)]
    pub fn reject(&mut self, sequence: u64) {
        self.rejected_sequence = Some(sequence);
    }

    #[cfg(test)]
    pub fn rejects(&self, sequence: u64) -> bool {
        self.rejected_sequence == Some(sequence)
    }

    pub fn sequence(&self) -> Option<u64> {
        self.session.map(|session| session.sequence)
    }

    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }

    pub fn moving_window(&self) -> Option<WindowInfo> {
        let session = self.session?;
        if session.mode != WindowDragMode::Move {
            return None;
        }
        let mut window = self.window.clone()?;
        window.rect = self.last_rect.unwrap_or(session.original_rect);
        Some(window)
    }

    pub fn update(&mut self, capture: WindowDragCapture, cancel: bool) -> Option<WindowDragUpdate> {
        let session = self.session?;
        if session.sequence != capture.sequence {
            return None;
        }
        if cancel {
            self.session = None;
            return Some(WindowDragUpdate {
                handle: session.handle,
                rect: session.original_rect,
                finished: true,
                cancelled: true,
            });
        }

        let rect = match session.mode {
            WindowDragMode::Move => moved_rect(session, capture.current),
            WindowDragMode::Resize => resized_rect(session, capture.current),
        };
        let finished = capture.finished;
        if !finished && self.last_rect == Some(rect) {
            return None;
        }
        self.last_rect = Some(rect);
        if finished {
            self.session = None;
        }
        Some(WindowDragUpdate {
            handle: session.handle,
            rect,
            finished,
            cancelled: false,
        })
    }

    pub fn cancel(&mut self) -> Option<(WindowHandle, Rect)> {
        let session = self.session.take()?;
        Some((session.handle, session.original_rect))
    }
}

fn moved_rect(session: DragSession, cursor: Point) -> Rect {
    let dx = cursor.x - session.start_cursor.x;
    let dy = cursor.y - session.start_cursor.y;
    Rect {
        left: session.original_rect.left + dx,
        top: session.original_rect.top + dy,
        right: session.original_rect.right + dx,
        bottom: session.original_rect.bottom + dy,
    }
}

fn resized_rect(session: DragSession, cursor: Point) -> Rect {
    const MIN_WIDTH: i32 = 160;
    const MIN_HEIGHT: i32 = 100;
    let dx = cursor.x - session.start_cursor.x;
    let dy = cursor.y - session.start_cursor.y;
    let center_x = (session.original_rect.left + session.original_rect.right) / 2;
    let center_y = (session.original_rect.top + session.original_rect.bottom) / 2;
    let mut rect = session.original_rect;
    if session.start_cursor.x < center_x {
        rect.left = (rect.left + dx).min(rect.right - MIN_WIDTH);
    } else {
        rect.right = (rect.right + dx).max(rect.left + MIN_WIDTH);
    }
    if session.start_cursor.y < center_y {
        rect.top = (rect.top + dy).min(rect.bottom - MIN_HEIGHT);
    } else {
        rect.bottom = (rect.bottom + dy).max(rect.top + MIN_HEIGHT);
    }
    rect
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stationary_capture_does_not_repeat_native_window_updates() {
        let mut controller = WindowDragController::default();
        let window = WindowInfo {
            handle: WindowHandle(1),
            rect: Rect {
                left: 100,
                top: 100,
                right: 700,
                bottom: 500,
            },
            title: "Test".into(),
            process_name: "test.exe".into(),
            class_name: "Test".into(),
            transient: false,
            maximized: false,
            arranged: false,
            topmost: false,
        };
        let mut capture = WindowDragCapture {
            sequence: 1,
            mode: WindowDragMode::Move,
            start: Point { x: 200, y: 200 },
            current: Point { x: 230, y: 220 },
            finished: false,
        };
        controller.start(capture, &window);
        assert!(controller.update(capture, false).is_some());
        for _ in 0..100 {
            assert!(
                controller.update(capture, false).is_none(),
                "unchanged coordinates must not repaint the target window"
            );
        }
        capture.finished = true;
        assert!(
            controller.update(capture, false).unwrap().finished,
            "release is never deduplicated"
        );
    }

    #[test]
    fn movement_uses_signed_virtual_desktop_coordinates() {
        let session = DragSession {
            sequence: 1,
            mode: WindowDragMode::Move,
            handle: WindowHandle(1),
            start_cursor: Point { x: -900, y: 300 },
            original_rect: Rect {
                left: -1000,
                top: 200,
                right: -400,
                bottom: 700,
            },
        };
        assert_eq!(
            moved_rect(session, Point { x: -1200, y: 450 }),
            Rect {
                left: -1300,
                top: 350,
                right: -700,
                bottom: 850
            }
        );
    }

    #[test]
    fn finishing_a_move_keeps_the_free_drag_geometry() {
        let mut controller = WindowDragController {
            session: Some(DragSession {
                sequence: 9,
                mode: WindowDragMode::Move,
                handle: WindowHandle(1),
                start_cursor: Point { x: -1000, y: 500 },
                original_rect: Rect {
                    left: -1200,
                    top: 300,
                    right: -600,
                    bottom: 800,
                },
            }),
            ..Default::default()
        };
        let update = controller
            .update(
                WindowDragCapture {
                    sequence: 9,
                    mode: WindowDragMode::Move,
                    start: Point { x: -1000, y: 500 },
                    current: Point { x: -1910, y: 500 },
                    finished: true,
                },
                false,
            )
            .unwrap();

        assert_eq!(
            update.rect,
            Rect {
                left: -2110,
                top: 300,
                right: -1510,
                bottom: 800,
            }
        );
        assert!(update.finished);
        assert!(!update.cancelled);
        assert!(!controller.is_active());
    }

    #[test]
    fn resize_preserves_a_minimum_surface() {
        let session = DragSession {
            sequence: 1,
            mode: WindowDragMode::Resize,
            handle: WindowHandle(1),
            start_cursor: Point { x: 10, y: 10 },
            original_rect: Rect {
                left: 0,
                top: 0,
                right: 500,
                bottom: 400,
            },
        };
        let rect = resized_rect(session, Point { x: 490, y: 390 });
        assert_eq!(rect.width(), 160);
        assert_eq!(rect.height(), 100);
    }

    fn test_window(handle: isize) -> WindowInfo {
        WindowInfo {
            handle: WindowHandle(handle),
            rect: Rect {
                left: 100,
                top: 100,
                right: 900,
                bottom: 700,
            },
            title: "doc.psd".to_string(),
            class_name: "Photoshop".to_string(),
            process_name: "photoshop.exe".to_string(),
            maximized: false,
            transient: false,
            arranged: false,
            topmost: false,
        }
    }

    fn test_capture(sequence: u64) -> WindowDragCapture {
        WindowDragCapture {
            sequence,
            mode: WindowDragMode::Move,
            start: Point { x: 300, y: 300 },
            current: Point { x: 320, y: 320 },
            finished: false,
        }
    }

    #[test]
    fn a_rejected_press_stays_rejected_and_never_starts_a_session() {
        let mut controller = WindowDragController::default();
        controller.reject(4);

        // 同一按下的后续高频移动必须一直被跳过，而不是重新判定目标窗口。
        assert!(controller.rejects(4));
        // 未记录拒绝的其它按下不受影响。
        assert!(!controller.rejects(5));
        // 被拒绝的按下不会建立会话，所以拖拽始终不生效。
        assert!(!controller.is_active());
        assert_eq!(controller.sequence(), None);
    }

    #[test]
    fn starting_a_session_clears_a_previous_rejection() {
        let mut controller = WindowDragController::default();
        controller.reject(4);
        assert!(controller.rejects(4));

        controller.start(test_capture(7), &test_window(1));
        assert_eq!(controller.sequence(), Some(7));
        assert!(controller.is_active());
        // 名单被移除后（用户删掉了该应用）新的按下必须能正常开始拖拽。
        assert!(!controller.rejects(4));

        let update = controller.update(test_capture(7), false).unwrap();
        assert_eq!(update.handle, WindowHandle(1));
    }
}
