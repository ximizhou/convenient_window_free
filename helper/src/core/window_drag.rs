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
}

impl WindowDragController {
    pub fn start(&mut self, capture: WindowDragCapture, window: &WindowInfo) {
        self.session = Some(DragSession {
            sequence: capture.sequence,
            mode: capture.mode,
            handle: window.handle,
            start_cursor: capture.start,
            original_rect: window.rect,
        });
    }

    pub fn sequence(&self) -> Option<u64> {
        self.session.map(|session| session.sequence)
    }

    pub fn is_active(&self) -> bool {
        self.session.is_some()
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
}
