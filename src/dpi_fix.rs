//! Windows: Fenster zwischen Monitoren mit unterschiedlicher Skalierung
//! verschieben.
//!
//! winit 0.30 setzt bei `WM_DPICHANGED` eine selbst berechnete Größe und
//! Position (Cursor-Verhältnis, Verschieben auf den neuen Monitor) statt des
//! Rechtecks, das Windows vorschlägt. Während des Ziehens arbeitet das gegen
//! die Verschiebe-Schleife von Windows: Die Skalierung pendelt alle 30–50 ms
//! zwischen beiden Monitoren, und mit jedem Hin und Her wächst das Fenster
//! um das Verhältnis der Skalierungen. Microsoft verlangt, das vorgeschlagene
//! Rechteck zu übernehmen. Deshalb hängt sich dieses Modul vor winit in die
//! Fensterprozedur: winit verarbeitet die Nachricht wie gewohnt (Skalierung,
//! Event an egui), darf das Fenster dabei aber nicht bewegen. Sonst rutscht
//! es zurück auf den alten Monitor, Windows schickt verschachtelt ein
//! zweites `WM_DPICHANGED`, und danach gälte ein veraltetes Rechteck.
//! Anschließend gilt das von Windows vorgeschlagene Rechteck.
//!
//! Aus demselben Grund setzt dieses Modul auch die Mindestgröße: eframe
//! rechnet `with_min_inner_size` einmalig in feste Pixel des Start-Monitors
//! um. Auf einem niedriger skalierten Monitor wäre das Fenster dann zu groß
//! und läge beim Wechsel wieder auf dem alten Monitor.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::Cell;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetWindowRect, IsZoomed, SetWindowPos, MINMAXINFO, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, USER_DEFAULT_SCREEN_DPI, WINDOWPOS, WM_DPICHANGED, WM_GETMINMAXINFO,
    WM_NCDESTROY, WM_WINDOWPOSCHANGING,
};

thread_local! {
    /// winit verarbeitet gerade `WM_DPICHANGED`: Größe und Position sperren
    static IN_DPI_CHANGE: Cell<bool> = const { Cell::new(false) };
}

const SUBCLASS_ID: usize = 0x6769_746d; // "gitm"

/// Hängt die Korrektur an das Hauptfenster. Liefert `false`, wenn kein
/// Win32-Fenster verfügbar ist.
pub fn install(window: &impl HasWindowHandle) -> bool {
    let Ok(handle) = window.window_handle() else {
        return false;
    };
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        return false;
    };
    let hwnd = h.hwnd.get() as HWND;
    // SAFETY: gültiges Fenster des eigenen Threads (eframe ruft `new` auf dem
    // UI-Thread auf); die Prozedur entfernt sich bei WM_NCDESTROY selbst.
    unsafe { SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0) != 0 }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_DPICHANGED => {
            // Vor winit kopieren: lparam zeigt auf das Rechteck von Windows
            let suggested = unsafe { *(lparam as *const RECT) };
            let was = IN_DPI_CHANGE.replace(true);
            let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            IN_DPI_CHANGE.set(was);
            // Maximiert bestimmt Windows die Größe selbst
            if unsafe { IsZoomed(hwnd) } == 0 {
                unsafe {
                    SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        suggested.left,
                        suggested.top,
                        suggested.right - suggested.left,
                        suggested.bottom - suggested.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    )
                };
            }
            result
        }
        WM_WINDOWPOSCHANGING if IN_DPI_CHANGE.get() => {
            // winits eigene Größe/Position verwerfen (siehe Moduldoku)
            let pos = lparam as *mut WINDOWPOS;
            unsafe { (*pos).flags |= SWP_NOMOVE | SWP_NOSIZE };
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_GETMINMAXINFO => {
            let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            if let Some((w, h)) = unsafe { min_outer_size(hwnd) } {
                let info = lparam as *mut MINMAXINFO;
                unsafe {
                    (*info).ptMinTrackSize.x = (*info).ptMinTrackSize.x.max(w);
                    (*info).ptMinTrackSize.y = (*info).ptMinTrackSize.y.max(h);
                }
            }
            result
        }
        WM_NCDESTROY => {
            unsafe { RemoveWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID) };
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

/// Mindestgröße des ganzen Fensters (mit Rahmen und Titelleiste) in Pixeln
/// bei der aktuellen Skalierung des Fensters.
unsafe fn min_outer_size(hwnd: HWND) -> Option<(i32, i32)> {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 {
        return None;
    }
    let scale = dpi as f32 / USER_DEFAULT_SCREEN_DPI as f32;
    let mut outer: RECT = unsafe { std::mem::zeroed() };
    let mut client: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut outer) } == 0
        || unsafe { GetClientRect(hwnd, &mut client) } == 0
    {
        return None;
    }
    let frame_w = (outer.right - outer.left) - (client.right - client.left);
    let frame_h = (outer.bottom - outer.top) - (client.bottom - client.top);
    let [w, h] = crate::app::MIN_WINDOW_SIZE;
    Some((
        (w * scale).round() as i32 + frame_w,
        (h * scale).round() as i32 + frame_h,
    ))
}
