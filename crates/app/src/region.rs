//! The island window on Windows: one fixed-size transparent window (the
//! frame) with a window region around the island, so changing views never
//! moves the window. (A moved WebView2 window shows its old picture at the
//! new spot for a frame: Phase 5c's one-frame jump.) Outside the region
//! nothing is drawn and clicks reach the window underneath.
//!
//! All `unsafe` code for it is in `win` below. Any failure returns false and
//! the caller sizes the window to the island again (today's resizing), so a
//! big invisible window never catches clicks.

use std::ffi::OsString;

use tauri::WebviewWindow;

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    fn right(self) -> i32 {
        self.x + self.w
    }

    fn bottom(self) -> i32 {
        self.y + self.h
    }

    /// Whether `inner` lies wholly inside this rectangle.
    #[cfg(test)]
    fn holds(self, inner: Rect) -> bool {
        inner.x >= self.x
            && inner.y >= self.y
            && inner.right() <= self.right()
            && inner.bottom() <= self.bottom()
    }
}

/// Room kept around the box in the region, for rounding between the page's
/// pixels and the window's (the page centres the box in the window).
const SLACK: i32 = 1;

/// The window (screen pixels) and its region (window pixels) for the island's
/// box `b`, in a frame up to `frame` (w, h), inside the work `area`. The
/// window is centred on the box and top-aligned with it, as the page draws
/// the box top-centred; it never passes the work area (by more than a pixel
/// of rounding), so it can't straddle monitors: near a screen edge it narrows.
pub fn frame_around(b: Rect, frame: (i32, i32), area: Rect) -> (Rect, Rect) {
    let cx = b.x + b.w / 2;
    let half = (frame.0 / 2)
        .min(cx - area.x)
        .min(area.right() - cx)
        .max((b.w + 1) / 2);
    let h = frame.1.min(area.bottom() - b.y).max(b.h);
    let window = Rect {
        x: cx - half,
        y: b.y,
        w: 2 * half,
        h,
    };
    let left = (b.x - window.x - SLACK).max(0);
    let region = Rect {
        x: left,
        y: 0,
        w: (b.right() - window.x + SLACK).min(window.w) - left,
        h: b.h,
    };
    (window, region)
}

/// The region can be turned off in debug builds (`BOUNCER_NO_REGION`), to
/// check the fallback by hand. Release builds ignore it.
fn forced_off() -> bool {
    forced_off_with(
        std::env::var_os("BOUNCER_NO_REGION"),
        cfg!(debug_assertions),
    )
}

fn forced_off_with(var: Option<OsString>, debug: bool) -> bool {
    debug && var.is_some()
}

/// Puts the window at `window` with only `region` shown. False on any
/// failure (the caller then sizes the window to the island again).
/// Main thread only.
#[cfg(windows)]
pub fn apply(w: &WebviewWindow, window: Rect, region: Rect) -> bool {
    if forced_off() {
        return false;
    }
    match w.hwnd() {
        Ok(hwnd) => win::apply(hwnd.0, window, region),
        Err(_) => false,
    }
}

/// Removes any region (back to a plain window). Main thread only.
#[cfg(windows)]
pub fn clear(w: &WebviewWindow) {
    if let Ok(hwnd) = w.hwnd() {
        win::clear(hwnd.0);
    }
}

/// Stops Windows painting an inactive title bar into the window when it
/// loses the keyboard. The window has no decorations, but Tao keeps the
/// caption style (and hides it by other means), so the default handling of
/// `WM_NCACTIVATE` drew a pale band in the transparent margin above the
/// island. False if it couldn't be set up (then nothing changes). Main
/// thread only, once.
#[cfg(windows)]
pub fn no_caption(w: &WebviewWindow) -> bool {
    match w.hwnd() {
        Ok(hwnd) => win::no_caption(hwnd.0),
        Err(_) => false,
    }
}

#[cfg(not(windows))]
pub fn no_caption(_: &WebviewWindow) -> bool {
    false
}

#[cfg(not(windows))]
pub fn apply(_: &WebviewWindow, _: Rect, _: Rect) -> bool {
    let _ = forced_off();
    false
}

#[cfg(not(windows))]
pub fn clear(_: &WebviewWindow) {}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::ptr::null_mut;

    use super::Rect;

    type Handle = *mut c_void;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetWindowPos(
            hwnd: Handle,
            after: Handle,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
        fn SetWindowRgn(hwnd: Handle, region: Handle, redraw: i32) -> i32;
    }

    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateRectRgn(left: i32, top: i32, right: i32, bottom: i32) -> Handle;
        fn DeleteObject(object: Handle) -> i32;
    }

    type SubclassProc = unsafe extern "system" fn(Handle, u32, usize, isize, usize, usize) -> isize;

    #[link(name = "comctl32")]
    unsafe extern "system" {
        fn SetWindowSubclass(hwnd: Handle, proc: SubclassProc, id: usize, data: usize) -> i32;
        fn DefSubclassProc(hwnd: Handle, msg: u32, wparam: usize, lparam: isize) -> isize;
        fn RemoveWindowSubclass(hwnd: Handle, proc: SubclassProc, id: usize) -> i32;
    }

    const WM_NCACTIVATE: u32 = 0x0086;
    const WM_NCDESTROY: u32 = 0x0082;
    const SUBCLASS_ID: usize = 0x5D;

    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;

    // SAFETY (all three): `hwnd` is this app's own live window, used on the
    // main thread; the calls take no pointers but handles. A region handed
    // to SetWindowRgn belongs to the system once it's accepted, so it's only
    // freed when refused.

    pub fn apply(hwnd: Handle, window: Rect, region: Rect) -> bool {
        unsafe {
            let flags = SWP_NOZORDER | SWP_NOACTIVATE;
            if SetWindowPos(
                hwnd,
                null_mut(),
                window.x,
                window.y,
                window.w,
                window.h,
                flags,
            ) == 0
            {
                return false;
            }
            let shape = CreateRectRgn(region.x, region.y, region.x + region.w, region.y + region.h);
            if shape.is_null() {
                return false;
            }
            if SetWindowRgn(hwnd, shape, 1) == 0 {
                DeleteObject(shape);
                return false;
            }
        }
        true
    }

    pub fn clear(hwnd: Handle) {
        unsafe {
            SetWindowRgn(hwnd, null_mut(), 1);
        }
    }

    // SAFETY: the subclass is set on this app's own window, on the main
    // thread that owns it; the procedure only passes every message on.
    pub fn no_caption(hwnd: Handle) -> bool {
        unsafe { SetWindowSubclass(hwnd, quiet_caption, SUBCLASS_ID, 0) != 0 }
    }

    /// Every message goes on unchanged, except that `WM_NCACTIVATE` carries
    /// lParam -1: the window still learns it's (in)active, but Windows
    /// doesn't repaint the non-client area for it (documented for -1).
    unsafe extern "system" fn quiet_caption(
        hwnd: Handle,
        msg: u32,
        wparam: usize,
        lparam: isize,
        _id: usize,
        _data: usize,
    ) -> isize {
        let lparam = if msg == WM_NCACTIVATE { -1 } else { lparam };
        // SAFETY: called by Windows for our window with its own message; the
        // subclass is removed as the window goes, before the message is
        // passed on (DefSubclassProc still reaches the original procedure).
        unsafe {
            if msg == WM_NCDESTROY {
                RemoveWindowSubclass(hwnd, quiet_caption, SUBCLASS_ID);
            }
            DefSubclassProc(hwnd, msg, wparam, lparam)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1366 × 720 work area, as on Charan's screen.
    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        w: 1366,
        h: 720,
    };
    const FRAME: (i32, i32) = (760, 627);

    /// The island inside its box: the box has the shadow margin around it
    /// (20 / 32 px at 100%, scaled) and the 8-px top gap.
    fn island(b: Rect) -> Rect {
        Rect {
            x: b.x + 22,
            y: b.y + 9,
            w: b.w - 44,
            h: b.h - 45,
        }
    }

    /// A card's Deny / Allow row: the bottom of the island, edge to edge
    /// (the worst case: the buttons reach the island's corners).
    fn buttons(b: Rect) -> Rect {
        let i = island(b);
        Rect {
            x: i.x,
            y: i.bottom() - 48,
            w: i.w,
            h: 48,
        }
    }

    /// The region in screen pixels.
    fn on_screen(window: Rect, region: Rect) -> Rect {
        Rect {
            x: window.x + region.x,
            ..region
        }
        .moved_down(window.y)
    }

    impl Rect {
        fn moved_down(self, dy: i32) -> Rect {
            Rect {
                y: self.y + dy,
                ..self
            }
        }
    }

    /// Boxes of every view (pill, open, card, wide, strip) at many spots,
    /// near every edge too.
    fn cases() -> Vec<Rect> {
        let sizes = [
            (368, 88),
            (493, 410),
            (493, 627),
            (760, 520),
            (202, 6),
            (760, 627),
        ];
        let mut out = Vec::new();
        for (w, h) in sizes {
            // As `place` puts it: always inside the work area.
            for x in [0, 1, 7, 150, 683 - w / 2, 900, AREA.w - w - 1, AREA.w - w] {
                let x = x.min(AREA.w - w);
                for y in [0, 9, 300, AREA.h - h].map(|y| y.min(AREA.h - h)) {
                    out.push(Rect { x, y, w, h });
                }
            }
        }
        out
    }

    #[test]
    fn the_region_always_holds_the_cards_buttons() {
        for b in cases() {
            let (window, region) = frame_around(b, FRAME, AREA);
            let shown = on_screen(window, region);
            assert!(shown.holds(b), "box {b:?} not in region {shown:?}");
            // Cards are drawn in the open and wide views, not the pill or strip.
            if b.h > 100 {
                assert!(
                    shown.holds(buttons(b)),
                    "buttons of {b:?} not in region {shown:?}"
                );
            }
            assert!(
                window.holds(shown),
                "region {shown:?} outside window {window:?}"
            );
        }
    }

    #[test]
    fn the_region_is_only_the_box() {
        for b in cases() {
            let (window, region) = frame_around(b, FRAME, AREA);
            let shown = on_screen(window, region);
            assert!(
                shown.w <= b.w + 2 * SLACK && shown.h == b.h,
                "{b:?} → {shown:?}"
            );
        }
    }

    #[test]
    fn the_frame_stays_on_its_screen_and_centred_on_the_box() {
        for b in cases() {
            let (window, _) = frame_around(b, FRAME, AREA);
            let room = Rect {
                x: AREA.x - 1,
                y: AREA.y,
                w: AREA.w + 2,
                h: AREA.h,
            };
            assert!(room.holds(window), "{window:?} leaves the work area");
            // Centred, so the page (which centres the box) draws it in the region.
            assert!(
                (window.x + window.w / 2 - (b.x + b.w / 2)).abs() <= 1,
                "{b:?} {window:?}"
            );
        }
    }

    #[test]
    fn switching_views_at_the_usual_spot_never_moves_the_window() {
        // Top centre: the pill, the open list and the wide view share one window.
        let at = 683;
        let windows: Vec<Rect> = [(368, 88), (493, 410), (760, 520), (202, 6)]
            .iter()
            .map(|&(w, h)| {
                frame_around(
                    Rect {
                        x: at - w / 2,
                        y: 0,
                        w,
                        h,
                    },
                    FRAME,
                    AREA,
                )
                .0
            })
            .collect();
        assert!(
            windows
                .iter()
                .all(|w| (w.x, w.y, w.w) == (windows[0].x, 0, FRAME.0)),
            "{windows:?}"
        );
    }

    #[test]
    fn the_no_region_switch_works_in_debug_builds_only() {
        assert!(forced_off_with(Some("1".into()), true));
        assert!(!forced_off_with(Some("1".into()), false));
        assert!(!forced_off_with(None, true));
    }
}
