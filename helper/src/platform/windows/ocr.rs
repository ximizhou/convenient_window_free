use crate::config::OcrLanguage;
use anyhow::{bail, Context, Result};
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::core::{Interface, HSTRING};
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::Buffer;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::WinRT::{IBufferByteAccess, RoInitialize, RO_INIT_MULTITHREADED};

const CF_UNICODETEXT_FORMAT: u32 = 13;
const OCR_WORKER_NOT_STARTED: u8 = 0;
const OCR_WORKER_HEALTHY: u8 = 1;
const OCR_WORKER_FAILED: u8 = 2;
const OCR_STALL_LIMIT: Duration = Duration::from_secs(30);

static OCR_WORKER_STATE: AtomicU8 = AtomicU8::new(OCR_WORKER_NOT_STARTED);
static OCR_JOB_STARTED_MS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub enum OcrCompletion {
    Copied(usize),
    Failed(String),
}

fn ocr_completions() -> &'static Mutex<VecDeque<OcrCompletion>> {
    static COMPLETIONS: OnceLock<Mutex<VecDeque<OcrCompletion>>> = OnceLock::new();
    COMPLETIONS.get_or_init(|| Mutex::new(VecDeque::new()))
}

pub fn record_ocr_completion(result: &Result<usize>) {
    let completion = match result {
        Ok(characters) => OcrCompletion::Copied(*characters),
        Err(error) => OcrCompletion::Failed(format!("{error:#}")),
    };
    if let Ok(mut completions) = ocr_completions().lock() {
        if completions.len() >= 16 {
            completions.pop_front();
        }
        completions.push_back(completion);
    }
}

pub fn take_ocr_completion() -> Option<OcrCompletion> {
    ocr_completions().lock().ok()?.pop_front()
}

pub fn recognize_and_copy_async<F>(
    pixels: Arc<Vec<u8>>,
    width: i32,
    height: i32,
    language: OcrLanguage,
    completion: F,
) where
    F: FnOnce(Result<usize>) + Send + 'static,
{
    let job = OcrJob {
        pixels,
        width,
        height,
        language,
        completion: Box::new(completion),
    };
    if let Err(error) = ocr_jobs().try_send(job) {
        let job = match error {
            mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job) => job,
        };
        (job.completion)(Err(anyhow::anyhow!("OCR 工作队列繁忙，请稍后重试")));
    }
}

struct OcrJob {
    pixels: Arc<Vec<u8>>,
    width: i32,
    height: i32,
    language: OcrLanguage,
    completion: Box<dyn FnOnce(Result<usize>) + Send>,
}

fn ocr_jobs() -> &'static mpsc::SyncSender<OcrJob> {
    static JOBS: OnceLock<mpsc::SyncSender<OcrJob>> = OnceLock::new();
    JOBS.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<OcrJob>(8);
        match std::thread::Builder::new()
            .name("local-ocr".into())
            .spawn(move || run_ocr_worker(receiver))
        {
            Ok(_) => OCR_WORKER_STATE.store(OCR_WORKER_HEALTHY, Ordering::Release),
            Err(error) => {
                OCR_WORKER_STATE.store(OCR_WORKER_FAILED, Ordering::Release);
                crate::logging::write_line(format!("ocr: worker start failed: {error}"));
            }
        }
        sender
    })
}

fn run_ocr_worker(receiver: mpsc::Receiver<OcrJob>) {
    while let Ok(job) = receiver.recv() {
        OCR_JOB_STARTED_MS.store(ocr_clock_ms(), Ordering::Release);
        crate::logging::write_line(format!("ocr: job started {}x{}", job.width, job.height));
        let result = run_ocr_operation(|| {
            recognize_and_copy(&job.pixels, job.width, job.height, job.language)
        });
        OCR_JOB_STARTED_MS.store(0, Ordering::Release);
        crate::logging::write_line(if result.is_ok() {
            "ocr: job completed"
        } else {
            "ocr: job failed"
        });
        if catch_unwind(AssertUnwindSafe(|| (job.completion)(result))).is_err() {
            crate::logging::write_line("ocr: completion callback panicked");
        }
    }
    OCR_JOB_STARTED_MS.store(0, Ordering::Release);
    OCR_WORKER_STATE.store(OCR_WORKER_FAILED, Ordering::Release);
    crate::logging::write_line("ocr: worker channel disconnected");
}

fn run_ocr_operation<F>(operation: F) -> Result<usize>
where
    F: FnOnce() -> Result<usize>,
{
    catch_unwind(AssertUnwindSafe(operation))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("OCR 工作线程异常，任务已安全终止")))
}

pub fn ocr_worker_error() -> Option<String> {
    if OCR_WORKER_STATE.load(Ordering::Acquire) == OCR_WORKER_FAILED {
        return Some("OCR 工作线程已停止".to_string());
    }
    let started = OCR_JOB_STARTED_MS.load(Ordering::Acquire);
    let now = ocr_clock_ms();
    ocr_job_is_stalled(started, now, OCR_STALL_LIMIT)
        .then(|| "OCR 工作线程超过 30 秒没有响应".to_string())
}

fn ocr_clock_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis()
        .min(u128::from(u64::MAX - 1)) as u64
        + 1
}

fn ocr_job_is_stalled(started_ms: u64, now_ms: u64, limit: Duration) -> bool {
    started_ms != 0 && now_ms.saturating_sub(started_ms) >= limit.as_millis() as u64
}

fn recognize_and_copy(
    pixels: &[u8],
    width: i32,
    height: i32,
    language: OcrLanguage,
) -> Result<usize> {
    let text = recognize_text(pixels, width, height, language)?;
    copy_text_to_clipboard(&text)?;
    Ok(text.chars().count())
}

fn recognize_text(pixels: &[u8], width: i32, height: i32, language: OcrLanguage) -> Result<String> {
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED).context("Windows OCR 初始化失败")?;
    }
    let (pixels, width, height) = fit_image_to_ocr_limit(pixels, width, height)?;
    let buffer = Buffer::Create(pixels.len() as u32).context("OCR 图像缓冲区创建失败")?;
    let access: IBufferByteAccess = buffer.cast().context("OCR 图像缓冲区不可写")?;
    unsafe {
        let target = access.Buffer().context("OCR 图像缓冲区访问失败")?;
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), target, pixels.len());
    }
    buffer.SetLength(pixels.len() as u32)?;
    let bitmap =
        SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, width, height)
            .context("OCR 位图创建失败")?;
    let engine = create_engine(language)?;
    let result = engine
        .RecognizeAsync(&bitmap)
        .context("OCR 任务创建失败")?
        .get()
        .context("OCR 识别失败")?;
    let text = normalize_ocr_text(&result.Text()?.to_string());
    if text.is_empty() {
        bail!("未识别到文字");
    }
    Ok(text)
}

pub fn available_ocr_languages() -> &'static [String] {
    static LANGUAGES: OnceLock<Vec<String>> = OnceLock::new();
    LANGUAGES.get_or_init(|| {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        [
            (OcrLanguage::Auto, "auto"),
            (OcrLanguage::ZhHans, "zh-Hans"),
            (OcrLanguage::English, "en"),
        ]
        .into_iter()
        .filter_map(|(language, tag)| create_engine(language).ok().map(|_| tag.to_string()))
        .collect()
    })
}

fn create_engine(language: OcrLanguage) -> Result<OcrEngine> {
    match language {
        OcrLanguage::Auto => {
            OcrEngine::TryCreateFromUserProfileLanguages().context("系统没有可用的 OCR 语言")
        }
        OcrLanguage::ZhHans | OcrLanguage::English => {
            let tag = match language {
                OcrLanguage::ZhHans => "zh-Hans",
                OcrLanguage::English => "en-US",
                OcrLanguage::Auto => unreachable!(),
            };
            let language = Language::CreateLanguage(&HSTRING::from(tag))?;
            if !OcrEngine::IsLanguageSupported(&language)? {
                bail!("系统未安装 {tag} OCR 语言包");
            }
            OcrEngine::TryCreateFromLanguage(&language).context("OCR 语言初始化失败")
        }
    }
}

fn fit_image_to_ocr_limit(pixels: &[u8], width: i32, height: i32) -> Result<(Vec<u8>, i32, i32)> {
    if width <= 0 || height <= 0 || pixels.len() != width as usize * height as usize * 4 {
        bail!("OCR 图像尺寸无效");
    }
    let limit = OcrEngine::MaxImageDimension().unwrap_or(2600).max(1) as i32;
    if width <= limit && height <= limit {
        return Ok((pixels.to_vec(), width, height));
    }
    let scale = (limit as f64 / width as f64).min(limit as f64 / height as f64);
    let next_width = (width as f64 * scale).round().max(1.0) as i32;
    let next_height = (height as f64 * scale).round().max(1.0) as i32;
    let mut output = vec![0_u8; next_width as usize * next_height as usize * 4];
    for y in 0..next_height {
        let source_y = ((y as i64 * height as i64) / next_height as i64) as i32;
        for x in 0..next_width {
            let source_x = ((x as i64 * width as i64) / next_width as i64) as i32;
            let source = (source_y as usize * width as usize + source_x as usize) * 4;
            let target = (y as usize * next_width as usize + x as usize) * 4;
            output[target..target + 4].copy_from_slice(&pixels[source..source + 4]);
        }
    }
    Ok((output, next_width, next_height))
}

fn normalize_ocr_text(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\r\n")
        .trim()
        .to_string()
}

fn copy_text_to_clipboard(text: &str) -> Result<()> {
    let mut utf16: Vec<u16> = text.encode_utf16().collect();
    utf16.push(0);
    let bytes = utf16.len() * std::mem::size_of::<u16>();
    let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes)? };
    let target = unsafe { GlobalLock(memory) };
    if target.is_null() {
        unsafe {
            let _ = GlobalFree(memory);
        }
        bail!("文字剪贴板内存分配失败");
    }
    unsafe {
        std::ptr::copy_nonoverlapping(utf16.as_ptr().cast::<u8>(), target.cast::<u8>(), bytes);
        let _ = GlobalUnlock(memory);
    }
    if let Err(error) = unsafe { OpenClipboard(HWND::default()) } {
        unsafe {
            let _ = GlobalFree(memory);
        }
        return Err(error).context("剪贴板正被其他程序占用");
    }
    let result = (|| -> Result<()> {
        unsafe {
            EmptyClipboard()?;
            SetClipboardData(CF_UNICODETEXT_FORMAT, HANDLE(memory.0))?;
        }
        Ok(())
    })();
    unsafe {
        let _ = CloseClipboard();
    }
    if result.is_err() {
        unsafe {
            let _ = GlobalFree(memory);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_worker_converts_panics_into_errors_and_detects_stalls() {
        let result = run_ocr_operation(|| panic!("simulated OCR panic"));
        assert!(result.unwrap_err().to_string().contains("安全终止"));
        assert!(!ocr_job_is_stalled(0, 50_000, OCR_STALL_LIMIT));
        assert!(!ocr_job_is_stalled(10_000, 39_999, OCR_STALL_LIMIT));
        assert!(ocr_job_is_stalled(10_000, 40_000, OCR_STALL_LIMIT));
    }

    #[test]
    fn text_normalization_keeps_readable_line_order() {
        assert_eq!(
            normalize_ocr_text("第一行  \nsecond\r\n"),
            "第一行\r\nsecond"
        );
    }

    #[test]
    #[ignore = "requires an installed Windows OCR language"]
    fn windows_ocr_recognizes_a_generated_bitmap() {
        let result = std::thread::spawn(|| {
            let (pixels, width, height) = render_test_text("OCR TEST 123");
            recognize_text(&pixels, width, height, OcrLanguage::Auto)
        })
        .join()
        .unwrap()
        .unwrap();
        let compact = result.replace(' ', "").to_ascii_uppercase();
        assert!(
            compact.contains("OCR") && compact.contains("123"),
            "{result}"
        );
    }

    #[test]
    #[ignore = "requires the Simplified Chinese Windows OCR language"]
    fn windows_ocr_recognizes_generated_simplified_chinese() {
        let result = std::thread::spawn(|| {
            let (pixels, width, height) = render_test_text("便捷窗口 测试 123");
            recognize_text(&pixels, width, height, OcrLanguage::ZhHans)
        })
        .join()
        .unwrap()
        .unwrap();
        let compact = result.replace(' ', "");
        assert!(
            (compact.contains("测试") || compact.contains("窗口")) && compact.contains("123"),
            "{result}"
        );
    }

    fn render_test_text(text: &str) -> (Vec<u8>, i32, i32) {
        use windows::core::w;
        use windows::Win32::Foundation::{COLORREF, HANDLE, HWND, RECT};
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateSolidBrush, DeleteDC,
            DeleteObject, FillRect, GetDC, ReleaseDC, SelectObject, SetBkColor, SetTextColor,
            TextOutW, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        };
        let width = 720;
        let height = 180;
        unsafe {
            let screen = GetDC(HWND::default());
            let dc = CreateCompatibleDC(screen);
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap =
                CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, HANDLE::default(), 0)
                    .unwrap();
            let old_bitmap = SelectObject(dc, bitmap);
            let white = CreateSolidBrush(COLORREF(0x00FFFFFF));
            FillRect(
                dc,
                &RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                },
                white,
            );
            let _ = DeleteObject(white);
            let font = CreateFontW(
                78,
                0,
                0,
                0,
                700,
                0,
                0,
                0,
                1,
                0,
                0,
                5,
                0,
                w!("Microsoft YaHei UI"),
            );
            let old_font = SelectObject(dc, font);
            let _ = SetBkColor(dc, COLORREF(0x00FFFFFF));
            let _ = SetTextColor(dc, COLORREF(0));
            let wide: Vec<u16> = text.encode_utf16().collect();
            let _ = TextOutW(dc, 28, 42, &wide);
            let pixels =
                std::slice::from_raw_parts(bits.cast::<u8>(), width as usize * height as usize * 4)
                    .to_vec();
            SelectObject(dc, old_font);
            SelectObject(dc, old_bitmap);
            let _ = DeleteObject(font);
            let _ = DeleteObject(bitmap);
            let _ = DeleteDC(dc);
            ReleaseDC(HWND::default(), screen);
            (pixels, width, height)
        }
    }

    #[test]
    fn invalid_pixel_dimensions_are_rejected_before_ocr() {
        assert!(fit_image_to_ocr_limit(&[0; 4], 2, 2).is_err());
    }
}
