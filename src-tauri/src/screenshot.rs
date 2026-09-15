//! Window screenshot capture using Windows PrintWindow API.

#[cfg(target_os = "windows")]
pub fn capture_window_by_hwnd(hwnd: isize) -> Result<(Vec<u8>, u32, u32), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        GetDC, GetDIBits, ReleaseDC, SelectObject,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, RGBQUAD,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    extern "system" { fn PrintWindow(hwnd: isize, hdcblt: isize, nflags: u32) -> i32; }
    const PW_RENDERFULLCONTENT: u32 = 2;

    unsafe {
        let mut rect = windows::Win32::Foundation::RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let _ = GetClientRect(HWND(hwnd as *mut _), &mut rect);
        let w = (rect.right - rect.left) as u32;
        let h = (rect.bottom - rect.top) as u32;
        if w < 10 || h < 10 {
            return Err(format!("Window too small: {w}x{h}"));
        }

        let hdc_win = GetDC(HWND(hwnd as *mut _));
        let hdc_mem = CreateCompatibleDC(hdc_win);
        let hbm = CreateCompatibleBitmap(hdc_win, w as i32, h as i32);
        let hbm_old = SelectObject(hdc_mem, hbm);

        PrintWindow(hwnd, hdc_mem.0 as isize, PW_RENDERFULLCONTENT);

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
        };
        let mut pixels = vec![0u8; (w * h * 4) as usize];
        GetDIBits(hdc_mem, hbm, 0, h, Some(pixels.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);

        SelectObject(hdc_mem, hbm_old);
        let _ = DeleteObject(hbm);
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(HWND(hwnd as *mut _), hdc_win);

        Ok((pixels, w, h))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn capture_window_by_hwnd(_hwnd: isize) -> Result<(Vec<u8>, u32, u32), String> {
    Err("Screenshot only supported on Windows".into())
}
