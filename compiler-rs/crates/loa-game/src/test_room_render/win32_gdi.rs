//! Win32 GDI clear-color renderer — Phase-3 test-room visible-pixels backend.
//!
//! § STRATEGY
//!
//!   Allocates a top-down BGRA DIB (device-independent bitmap) sized to the
//!   client area, fills it with a uniform color each frame via memset, then
//!   blits to the window's HDC via `StretchDIBits`. The DIB is a plain heap
//!   allocation owned by this module — no GDI HBITMAP, so no GDI handle leak
//!   risk if Drop is missed (worst case = leaked Vec, not a process-wide GDI
//!   handle).
//!
//! § UNSAFE
//!
//!   FFI is opt-in via an explicit `unsafe` block per call-site, each with a
//!   `// SAFETY :` justification. We do NOT spread `unsafe fn` across the
//!   module. Every `windows-rs` import carries the `Win32_…` feature already
//!   pulled in by `cssl-host-window` so we don't re-declare features here.
//!
//! § PRIME-DIRECTIVE
//!
//!   The renderer NEVER blocks the close-event path. `paint_clear_color` is
//!   bounded : one `GetDC` + one buffer-fill + one `StretchDIBits` + one
//!   `ReleaseDC`. The window's pump_events still drives close-event observation
//!   on the main thread.

// ── Clippy allowances scoped to the GDI interop module ────────────────
// `cast_possible_wrap` : Win32 i32 dimensions are bounded by GetClientRect ;
//   wrapping a u32 client-area to i32 cannot occur on any real monitor (max
//   resolution << INT32_MAX). We cap the input via `width.max(1)` already.
// `borrow_as_ptr` : `&bmi as *const _` is the canonical FFI pattern for
//   passing a stack BITMAPINFO ; `addr_of!` adds verbosity without changing
//   semantics. The clippy-pedantic suggestion is preference, not safety.
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::borrow_as_ptr)]

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetDC, ReleaseDC, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use super::RenderOutcome;

/// Errors specific to the Win32 GDI renderer.
#[derive(Debug, thiserror::Error)]
pub enum GdiRenderError {
    /// HWND was 0 / null.
    #[error("invalid HWND (handle = 0)")]
    InvalidHwnd,
    /// `GetDC` returned null — usually means the window was destroyed mid-call.
    #[error("GetDC returned null (window destroyed?)")]
    GetDcFailed,
    /// Width or height was 0.
    #[error("invalid dimensions: {width}x{height}")]
    InvalidDimensions {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
    },
    /// `StretchDIBits` returned 0 (failure).
    #[error("StretchDIBits failed (returned 0)")]
    StretchFailed,
}

/// GDI-based clear-color renderer.
///
/// Owns a heap-allocated BGRA backing buffer + the target HWND. The renderer
/// caches the buffer across frames so a constant-color frame is just a memset
/// + a blit — no per-frame allocation.
pub struct GdiRenderer {
    /// Raw HWND value (`usize`). The renderer does NOT own the window's
    /// lifetime ; the parent `cssl_host_window::Window` does. Callers MUST
    /// drop the renderer before dropping the window.
    hwnd_usize: usize,
    /// Backing pixel buffer, BGRA8 (one u32 = one pixel).
    pixels: Vec<u32>,
    /// Current backbuffer width.
    width: u32,
    /// Current backbuffer height.
    height: u32,
}

impl core::fmt::Debug for GdiRenderer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GdiRenderer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("hwnd_present", &(self.hwnd_usize != 0))
            .finish()
    }
}

impl GdiRenderer {
    /// Construct a renderer targeting the given HWND with the initial backing
    /// buffer sized to `width` x `height`. The buffer auto-resizes via
    /// [`Self::resize`] when the window is resized.
    pub fn new(hwnd_usize: usize, width: u32, height: u32) -> Result<Self, GdiRenderError> {
        if hwnd_usize == 0 {
            return Err(GdiRenderError::InvalidHwnd);
        }
        if width == 0 || height == 0 {
            return Err(GdiRenderError::InvalidDimensions { width, height });
        }
        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(GdiRenderError::InvalidDimensions { width, height })?;

        Ok(Self {
            hwnd_usize,
            pixels: vec![0u32; pixel_count],
            width,
            height,
        })
    }

    /// Resize the backing buffer. Called from the loop when a window resize
    /// event has been observed.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), GdiRenderError> {
        if width == 0 || height == 0 {
            return Err(GdiRenderError::InvalidDimensions { width, height });
        }
        let pixel_count = (width as usize)
            .checked_mul(height as usize)
            .ok_or(GdiRenderError::InvalidDimensions { width, height })?;
        self.pixels.clear();
        self.pixels.resize(pixel_count, 0);
        self.width = width;
        self.height = height;
        Ok(())
    }

    /// Refresh the cached width/height from the window's actual client rect.
    /// Returns the new (width, height) pair for telemetry.
    ///
    /// The Win32 window backend does not yet emit Resize events through to
    /// user-code (D-axis surface), so we periodically poll. Cheap : one
    /// `GetClientRect` per call.
    pub fn refresh_dimensions(&mut self) -> Result<(u32, u32), GdiRenderError> {
        let mut rect = RECT::default();
        // SAFETY : hwnd_usize was validated non-null at construction. Even if
        // the window was destroyed mid-frame, `GetClientRect` returns FALSE
        // and leaves rect zero-initialized ; we treat that as "skip resize".
        let ok = unsafe {
            let hwnd = HWND(self.hwnd_usize as *mut _);
            GetClientRect(hwnd, &mut rect)
        };
        if ok.is_err() {
            // Don't promote to error — the window may be in a transient
            // teardown state. Caller will re-query next frame.
            return Ok((self.width, self.height));
        }
        let new_w = (rect.right - rect.left).max(1) as u32;
        let new_h = (rect.bottom - rect.top).max(1) as u32;
        if new_w != self.width || new_h != self.height {
            self.resize(new_w, new_h)?;
        }
        Ok((new_w, new_h))
    }

    /// Paint a uniform color across the full backbuffer + blit to the window.
    /// Returns `Painted` on success ; `Skipped` on a transient FFI failure
    /// (logged via `eprintln!` so the user can correlate to a flicker).
    pub fn paint_clear_color(&mut self, r: u8, g: u8, b: u8) -> RenderOutcome {
        // Build BGRA pixel : MSB→LSB = 00 R G B in u32, since BITMAPINFO is
        // BI_RGB + 32bpp top-down which matches little-endian BGRA on Win32.
        let pixel: u32 = (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);

        // Cheap fill via slice::fill — LLVM lowers to memset for u32.
        self.pixels.fill(pixel);

        let header = BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: self.width as i32,
            // Negative height = top-down DIB ; matches our buffer layout.
            biHeight: -(self.height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let bmi = BITMAPINFO {
            bmiHeader: header,
            bmiColors: [Default::default(); 1],
        };

        // SAFETY : hwnd_usize is non-null (checked at construction), pixels
        // is heap-allocated + correctly sized for biWidth*biHeight*4 bytes,
        // BITMAPINFO is fully initialized above, and we release the DC on
        // every path before returning. `windows-rs` HWND is a transparent
        // pointer wrapper.
        let outcome = unsafe {
            let hwnd = HWND(self.hwnd_usize as *mut _);
            let hdc = GetDC(hwnd);
            if hdc.is_invalid() {
                return RenderOutcome::Skipped;
            }
            let blit_result = StretchDIBits(
                hdc,
                0,
                0,
                self.width as i32,
                self.height as i32,
                0,
                0,
                self.width as i32,
                self.height as i32,
                Some(self.pixels.as_ptr().cast()),
                &bmi as *const _,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
            // Release ALWAYS, even on blit failure, to avoid GDI handle leak.
            let _ = ReleaseDC(hwnd, hdc);
            if blit_result == 0 {
                RenderOutcome::Skipped
            } else {
                RenderOutcome::Painted
            }
        };

        if matches!(outcome, RenderOutcome::Skipped) {
            // Surface to telemetry once-per-failure ; the loop continues.
            // Avoids spamming on rapid teardown.
            eprintln!("loa-game: GDI blit failed (window may be tearing down)");
        }
        outcome
    }

    /// Test-only : return the current cached buffer dimensions without
    /// querying GDI. Used by unit tests on non-windowed CI.
    #[cfg(test)]
    fn cached_dims(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Test-only : sample a backbuffer pixel (BGRA u32) at (x, y). Returns
    /// `None` if (x, y) is outside the cached `(width, height)` rect.
    #[cfg(test)]
    fn sample_pixel(&self, x: u32, y: u32) -> Option<u32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = (y as usize)
            .checked_mul(self.width as usize)?
            .checked_add(x as usize)?;
        self.pixels.get(idx).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_null_hwnd() {
        let r = GdiRenderer::new(0, 1280, 720);
        assert!(matches!(r, Err(GdiRenderError::InvalidHwnd)));
    }

    #[test]
    fn new_rejects_zero_dims() {
        let r = GdiRenderer::new(0xDEAD_BEEF, 0, 720);
        assert!(matches!(r, Err(GdiRenderError::InvalidDimensions { .. })));
        let r = GdiRenderer::new(0xDEAD_BEEF, 1280, 0);
        assert!(matches!(r, Err(GdiRenderError::InvalidDimensions { .. })));
    }

    #[test]
    fn new_with_fake_hwnd_allocates_buffer() {
        // We're allocating a backing buffer ; the HWND won't be touched until
        // a paint call (which we don't make in this test).
        let r = GdiRenderer::new(0xFEED_FACE, 16, 16).expect("alloc must succeed");
        assert_eq!(r.cached_dims(), (16, 16));
    }

    #[test]
    fn resize_updates_dims_and_buffer() {
        let mut r = GdiRenderer::new(0xFEED_FACE, 16, 16).unwrap();
        r.resize(32, 24).unwrap();
        assert_eq!(r.cached_dims(), (32, 24));
        // Buffer was actually re-sized.
        assert_eq!(r.pixels.len(), 32 * 24);
    }

    #[test]
    fn resize_rejects_zero() {
        let mut r = GdiRenderer::new(0xFEED_FACE, 16, 16).unwrap();
        assert!(r.resize(0, 16).is_err());
        assert!(r.resize(16, 0).is_err());
        // Original dims preserved on rejection.
        assert_eq!(r.cached_dims(), (16, 16));
    }

    #[test]
    fn debug_impl_does_not_leak_hwnd_value() {
        let r = GdiRenderer::new(0xFEED_FACE, 16, 16).unwrap();
        let s = format!("{r:?}");
        assert!(s.contains("hwnd_present: true"));
        assert!(!s.contains("0xFEED")); // raw value not exposed
        assert!(!s.contains("4276993774")); // decimal of FEED_FACE
    }

    #[test]
    fn sample_pixel_within_buffer() {
        let r = GdiRenderer::new(0xFEED_FACE, 4, 4).unwrap();
        // Initial buffer is all-zero.
        assert_eq!(r.sample_pixel(0, 0), Some(0));
        assert_eq!(r.sample_pixel(3, 3), Some(0));
        // Out of bounds.
        assert_eq!(r.sample_pixel(4, 0), None);
        assert_eq!(r.sample_pixel(0, 4), None);
    }
}
