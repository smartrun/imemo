//! Main UI application using eframe & egui
//! Features toolbar, live HUD coordinates, slide collection chips,
//! rich HTML preview with embedded image rendering, source view, and export tools.

use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::config::{AppConfig, CaptureRect};
use crate::matrix_decoder::{DecodedFrame, MatrixDecoder};
use crate::slide_collector::{RestorationResult, SlideCollector};

#[cfg(windows)]
use crate::region_selector::select_region_modal;
#[cfg(windows)]
use crate::screen_capture::ScreenCapturer;

pub struct ImemoApp {
    config: AppConfig,
    is_capturing: Arc<AtomicBool>,
    status_message: String,
    frame_chips: Vec<bool>,
    restored_result: Option<RestorationResult>,
    view_mode: usize, // 0: Preview, 1: HTML Source
    banner_message: Option<(String, Instant)>,

    // Extracted embedded images for GUI rendering
    embedded_textures: Vec<(String, egui::TextureHandle)>,

    // Channels for background capture thread
    frame_receiver: Option<Receiver<DecodedFrame>>,
    capture_thread: Option<thread::JoinHandle<()>>,

    collector: SlideCollector,

    // Manual coordinates inputs
    coord_x_str: String,
    coord_y_str: String,
    coord_w_str: String,
    coord_h_str: String,
    show_coord_dialog: bool,
}

impl ImemoApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        #[cfg(windows)]
        ScreenCapturer::enable_dpi_awareness();

        // Custom fonts and styling
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(10.0, 6.0);
        cc.egui_ctx.set_style(style);

        let config = AppConfig::load();
        let last_rect = config.last_rect.unwrap_or_default();

        Self {
            config,
            is_capturing: Arc::new(AtomicBool::new(false)),
            status_message: "캡처 대기 중".to_string(),
            frame_chips: Vec::new(),
            restored_result: None,
            view_mode: 0,
            banner_message: None,
            embedded_textures: Vec::new(),
            frame_receiver: None,
            capture_thread: None,
            collector: SlideCollector::new(),
            coord_x_str: last_rect.x.to_string(),
            coord_y_str: last_rect.y.to_string(),
            coord_w_str: last_rect.width.to_string(),
            coord_h_str: last_rect.height.to_string(),
            show_coord_dialog: false,
        }
    }

    fn show_banner(&mut self, text: impl Into<String>) {
        self.banner_message = Some((text.into(), Instant::now()));
    }

    fn reset_all(&mut self) {
        self.stop_capture();
        self.collector.reset();
        self.restored_result = None;
        self.frame_chips.clear();
        self.embedded_textures.clear();
        self.status_message = "초기화되었습니다.".to_string();
    }

    fn start_capture(&mut self) {
        let rect = match self.config.last_rect {
            Some(r) if r.width > 20 && r.height > 20 => r,
            _ => {
                self.status_message = "먼저 [영역 선택]으로 바코드 영역을 지정해주세요.".to_string();
                return;
            }
        };

        if self.is_capturing.load(Ordering::SeqCst) {
            return;
        }

        self.collector.reset();
        self.frame_chips.clear();
        self.is_capturing.store(true, Ordering::SeqCst);
        self.status_message = format!(
            "화면 연속 캡처 및 인식 중... [X: {}, Y: {}, W: {}, H: {}]",
            rect.x, rect.y, rect.width, rect.height
        );

        let (tx, rx) = channel();
        self.frame_receiver = Some(rx);

        let is_capturing_clone = self.is_capturing.clone();
        let handle = thread::spawn(move || {
            while is_capturing_clone.load(Ordering::SeqCst) {
                #[cfg(windows)]
                {
                    if let Ok(rgba) = ScreenCapturer::capture_rect(rect.x, rect.y, rect.width, rect.height) {
                        if let Some(frame) = MatrixDecoder::decode_rgba_image(&rgba, rect.width as usize, rect.height as usize) {
                            let _ = tx.send(frame);
                        }
                    }
                }
                thread::sleep(Duration::from_millis(250));
            }
        });

        self.capture_thread = Some(handle);
    }

    fn stop_capture(&mut self) {
        self.is_capturing.store(false, Ordering::SeqCst);
        if let Some(handle) = self.capture_thread.take() {
            let _ = handle.join();
        }
        self.frame_receiver = None;
        self.status_message = "캡처 일시정지됨".to_string();
    }

    fn handle_decoded_frame(&mut self, ctx: &egui::Context, frame: DecodedFrame) {
        if let Some(result) = self.collector.add_frame(&frame) {
            self.stop_capture();
            self.status_message = format!(
                "문서 복원 완료! (크기: {} B, 총 {}프레임)",
                result.decompressed_bytes, result.total_frames
            );
            self.frame_chips = self.collector.frame_status_list();
            self.load_embedded_images(ctx, &result.html);
            self.restored_result = Some(result);
        } else {
            self.status_message = self.collector.progress_text();
            self.frame_chips = self.collector.frame_status_list();
        }
    }

    fn open_image_file(&mut self, ctx: &egui::Context) {
        let file = rfd::FileDialog::new()
            .add_filter("Image Files", &["png", "jpg", "jpeg", "webp", "bmp"])
            .pick_file();

        if let Some(path) = file {
            match image::open(&path) {
                Ok(dyn_img) => {
                    let rgba = dyn_img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    if let Some(frame) = MatrixDecoder::decode_rgba_image(&rgba, w as usize, h as usize) {
                        self.handle_decoded_frame(ctx, frame);
                        self.show_banner("이미지 파일에서 iMemo 코드를 검출했습니다!");
                    } else {
                        self.status_message = "이미지에서 iMemo 코드를 검출하지 못했습니다.".to_string();
                    }
                }
                Err(err) => {
                    self.status_message = format!("이미지 열기 실패: {}", err);
                }
            }
        }
    }

    fn paste_from_clipboard(&mut self, ctx: &egui::Context) {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if let Ok(img_data) = clipboard.get_image() {
                let w = img_data.width;
                let h = img_data.height;
                let rgba = img_data.bytes.into_owned();
                if let Some(frame) = MatrixDecoder::decode_rgba_image(&rgba, w, h) {
                    self.handle_decoded_frame(ctx, frame);
                    self.show_banner("클립보드 이미지에서 문서를 복원했습니다!");
                } else {
                    self.status_message = "클립보드 이미지에서 iMemo 코드를 검출하지 못했습니다.".to_string();
                }
            } else {
                self.status_message = "클립보드에 이미지 데이터가 없습니다.".to_string();
            }
        }
    }

    fn select_region(&mut self) {
        #[cfg(windows)]
        {
            if let Some(rect) = select_region_modal() {
                if rect.width > 20 && rect.height > 20 {
                    self.config.last_rect = Some(rect);
                    self.config.save();
                    self.coord_x_str = rect.x.to_string();
                    self.coord_y_str = rect.y.to_string();
                    self.coord_w_str = rect.width.to_string();
                    self.coord_h_str = rect.height.to_string();
                    self.status_message = format!(
                        "영역 자동 저장 완료: [X: {}, Y: {}, W: {}, H: {}]",
                        rect.x, rect.y, rect.width, rect.height
                    );
                    self.show_banner("캡처 영역이 저장되었습니다!");
                }
            }
        }
    }

    fn copy_html(&mut self) {
        if let Some(res) = &self.restored_result {
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(&res.html);
                self.show_banner("HTML이 클립보드에 복사되었습니다!");
            }
        }
    }

    fn copy_plain_text(&mut self) {
        if let Some(res) = &self.restored_result {
            let plain = Self::html_to_plain_text(&res.html);
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                let _ = clipboard.set_text(plain);
                self.show_banner("일반 텍스트가 클립보드에 복사되었습니다!");
            }
        }
    }

    fn save_html_file(&mut self) {
        if let Some(res) = &self.restored_result {
            let file = rfd::FileDialog::new()
                .set_file_name("iMemo_Restored.html")
                .add_filter("HTML Document", &["html", "htm"])
                .save_file();

            if let Some(path) = file {
                let styled = format!(
                    r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>iMemo Restored Document</title>
  <style>
    body {{
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Noto Sans KR", sans-serif;
      padding: 32px;
      color: #1e293b;
      line-height: 1.6;
      font-size: 15px;
      max-width: 900px;
      margin: 0 auto;
    }}
    table {{
      border-collapse: collapse;
      width: 100%;
      margin: 16px 0;
    }}
    th, td {{
      border: 1px solid #cbd5e1;
      padding: 8px 12px;
      text-align: left;
    }}
    th {{
      background: #f8fafc;
    }}
    img {{
      max-width: 100%;
      height: auto;
      border-radius: 6px;
      box-shadow: 0 2px 8px rgba(0,0,0,0.1);
      margin: 10px 0;
    }}
  </style>
</head>
<body>
{}
</body>
</html>"#,
                    res.html
                );
                if std::fs::write(&path, styled).is_ok() {
                    self.show_banner(format!("파일 저장 완료: {}", path.display()));
                }
            }
        }
    }

    fn open_in_browser(&mut self) {
        if let Some(res) = &self.restored_result {
            let temp_dir = std::env::temp_dir();
            let file_path = temp_dir.join(format!("imemo_doc_{}.html", res.doc_id));
            let styled = format!(
                r#"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>iMemo Doc #{}</title>
  <style>
    body {{
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Noto Sans KR", sans-serif;
      padding: 40px;
      color: #1e293b;
      line-height: 1.6;
      font-size: 16px;
      max-width: 960px;
      margin: 0 auto;
    }}
    table {{ border-collapse: collapse; width: 100%; margin: 16px 0; }}
    th, td {{ border: 1px solid #cbd5e1; padding: 8px 12px; text-align: left; }}
    th {{ background: #f8fafc; }}
    img {{ max-width: 100%; height: auto; border-radius: 6px; box-shadow: 0 2px 8px rgba(0,0,0,0.1); margin: 12px 0; display: block; }}
  </style>
</head>
<body>
{}
</body>
</html>"#,
                res.doc_id, res.html
            );
            if std::fs::write(&file_path, styled).is_ok() {
                let _ = open::that(&file_path);
                self.show_banner("기본 브라우저에서 문서를 열었습니다!");
            }
        }
    }

    fn html_to_plain_text(html: &str) -> String {
        let mut out = String::new();
        let mut in_tag = false;
        let mut tag_buf = String::new();

        for ch in html.chars() {
            if ch == '<' {
                in_tag = true;
                tag_buf.clear();
            } else if ch == '>' {
                in_tag = false;
                let lower = tag_buf.to_lowercase();
                if lower.starts_with("img") {
                    out.push_str(" [첨부 이미지] ");
                } else if lower == "p" || lower == "/p" || lower == "br" || lower == "tr" || lower == "/tr" {
                    out.push('\n');
                }
            } else if in_tag {
                tag_buf.push(ch);
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn load_embedded_images(&mut self, ctx: &egui::Context, html: &str) {
        self.embedded_textures.clear();
        let mut search_idx = 0;

        while let Some(pos) = html[search_idx..].find("src=\"data:image/") {
            let actual_pos = search_idx + pos + 5; // skip src="
            if let Some(end_pos) = html[actual_pos..].find('\"') {
                let data_uri = &html[actual_pos..(actual_pos + end_pos)];
                search_idx = actual_pos + end_pos;

                if let Some(base64_idx) = data_uri.find(";base64,") {
                    let base64_str = &data_uri[(base64_idx + 8)..];
                    // Base64 decode
                    if let Ok(bytes) = base64_decode(base64_str) {
                        if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                            let rgba = dyn_img.to_rgba8();
                            let (w, h) = rgba.dimensions();
                            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                [w as usize, h as usize],
                                &rgba.into_raw(),
                            );
                            let texture = ctx.load_texture(
                                format!("img_{}", self.embedded_textures.len()),
                                color_image,
                                egui::TextureOptions::LINEAR,
                            );
                            self.embedded_textures.push((data_uri.to_string(), texture));
                        }
                    }
                }
            } else {
                break;
            }
        }
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
    const TABLE: [i8; 256] = {
        let mut t = [-1i8; 256];
        let mut i = 0u8;
        while i < 26 {
            t[(b'A' + i) as usize] = i as i8;
            t[(b'a' + i) as usize] = (i + 26) as i8;
            i += 1;
        }
        let mut i = 0u8;
        while i < 10 {
            t[(b'0' + i) as usize] = (i + 52) as i8;
            i += 1;
        }
        t[b'+' as usize] = 62;
        t[b'/' as usize] = 63;
        t
    };

    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;

    for &b in input.as_bytes() {
        if b == b'=' || b.is_ascii_whitespace() {
            continue;
        }
        let val = TABLE[b as usize];
        if val < 0 {
            continue;
        }
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

impl eframe::App for ImemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll background capture receiver
        let frames: Vec<DecodedFrame> = if let Some(rx) = &self.frame_receiver {
            let mut list = Vec::new();
            while let Ok(frame) = rx.try_recv() {
                list.push(frame);
            }
            list
        } else {
            Vec::new()
        };
        for frame in frames {
            self.handle_decoded_frame(ctx, frame);
        }

        // Global Keyboard Shortcuts
        if ctx.input(|i| i.key_pressed(egui::Key::F5) || (i.modifiers.ctrl && i.key_pressed(egui::Key::Space))) {
            if self.is_capturing.load(Ordering::SeqCst) {
                self.stop_capture();
            } else {
                self.start_capture();
            }
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::V)) {
            self.paste_from_clipboard(ctx);
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::O)) {
            self.open_image_file(ctx);
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::R)) {
            self.reset_all();
        }

        // Top Toolbar
        egui::TopBottomPanel::top("top_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("iMemo 리더기").strong().size(18.0).color(Color32::from_rgb(37, 99, 235)));
                ui.label(RichText::new("Windows v1.0").size(12.0).color(Color32::GRAY));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🔄 초기화 (Ctrl+R)").clicked() {
                        self.reset_all();
                    }

                    if ui.button("📁 이미지 열기 (Ctrl+O)").clicked() {
                        self.open_image_file(ctx);
                    }

                    if ui.button("📋 클립보드 읽기 (Ctrl+V)").clicked() {
                        self.paste_from_clipboard(ctx);
                    }

                    let is_cap = self.is_capturing.load(Ordering::SeqCst);
                    if is_cap {
                        if ui.button(RichText::new("⏹ 캡처 중지 (F5)").color(Color32::from_rgb(239, 68, 68)).strong()).clicked() {
                            self.stop_capture();
                        }
                    } else {
                        if ui.button(RichText::new("▶ 캡처 시작 (F5)").color(Color32::from_rgb(34, 197, 94)).strong()).clicked() {
                            self.start_capture();
                        }
                    }

                    if ui.button("✂️ 영역 선택").on_hover_text("마우스로 화면의 바코드 영역을 드래그하여 지정합니다.").clicked() {
                        self.select_region();
                    }
                });
            });
        });

        // Status & Coordinates Bar
        egui::TopBottomPanel::top("status_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let is_cap = self.is_capturing.load(Ordering::SeqCst);
                let (indicator_color, _) = if is_cap {
                    (Color32::from_rgb(34, 197, 94), "캡처 중")
                } else if self.restored_result.is_some() {
                    (Color32::from_rgb(59, 130, 246), "완료")
                } else {
                    (Color32::GRAY, "대기")
                };

                let (response, painter) = ui.allocate_painter(Vec2::new(12.0, 12.0), egui::Sense::hover());
                painter.circle_filled(response.rect.center(), 5.0, indicator_color);

                ui.label(RichText::new(&self.status_message).strong());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Frame progress chips
                    if !self.frame_chips.is_empty() {
                        ui.horizontal(|ui| {
                            for (idx, &received) in self.frame_chips.iter().enumerate() {
                                let (bg, fg) = if received {
                                    (Color32::from_rgb(34, 197, 94), Color32::WHITE)
                                } else {
                                    (Color32::from_gray(60), Color32::from_gray(200))
                                };
                                let text = RichText::new(format!("{}", idx + 1)).size(11.0).strong().color(fg);
                                egui::Frame::none()
                                    .fill(bg)
                                    .rounding(4.0)
                                    .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                                    .show(ui, |ui| {
                                        ui.label(text);
                                    });
                            }
                        });
                    }

                    // Selected Coordinates Badge
                    if let Some(rect) = self.config.last_rect {
                        let text = format!("📍 X: {}, Y: {}, W: {} × H: {} px", rect.x, rect.y, rect.width, rect.height);
                        let btn = ui.button(RichText::new(text).size(12.0).monospace().color(Color32::from_rgb(34, 197, 94)));
                        if btn.clicked() {
                            self.show_coord_dialog = !self.show_coord_dialog;
                        }
                    }
                });
            });

            // Optional Manual Coordinate adjustment panel
            if self.show_coord_dialog {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("수동 좌표 조정:");
                    ui.label("X:");
                    ui.add(egui::TextEdit::singleline(&mut self.coord_x_str).desired_width(50.0));
                    ui.label("Y:");
                    ui.add(egui::TextEdit::singleline(&mut self.coord_y_str).desired_width(50.0));
                    ui.label("W:");
                    ui.add(egui::TextEdit::singleline(&mut self.coord_w_str).desired_width(60.0));
                    ui.label("H:");
                    ui.add(egui::TextEdit::singleline(&mut self.coord_h_str).desired_width(60.0));

                    if ui.button("적용").clicked() {
                        if let (Ok(x), Ok(y), Ok(w), Ok(h)) = (
                            self.coord_x_str.parse::<i32>(),
                            self.coord_y_str.parse::<i32>(),
                            self.coord_w_str.parse::<u32>(),
                            self.coord_h_str.parse::<u32>(),
                        ) {
                            let rect = CaptureRect { x, y, width: w, height: h };
                            self.config.last_rect = Some(rect);
                            self.config.save();
                            self.show_banner("좌표가 수동 적용되었습니다!");
                        }
                    }
                });
            }
        });

        // Main Document Content Area
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some((banner, start_time)) = &self.banner_message {
                if start_time.elapsed().as_secs_f32() < 3.0 {
                    egui::Frame::none()
                        .fill(Color32::from_rgb(22, 101, 52))
                        .rounding(6.0)
                        .inner_margin(egui::Margin::symmetric(14.0, 8.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("✓").strong().size(15.0).color(Color32::WHITE));
                                ui.label(RichText::new(banner).size(14.0).color(Color32::WHITE));
                            });
                        });
                    ui.add_space(8.0);
                }
            }

            if let Some(res) = self.restored_result.clone() {
                // Action row
                let mut save_clicked = false;
                let mut browser_clicked = false;
                let mut copy_text_clicked = false;
                let mut copy_html_clicked = false;

                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.view_mode, 0, "📄 서식 미리보기");
                    ui.selectable_value(&mut self.view_mode, 1, "💻 HTML 소스");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("💾 HTML 저장...").clicked() {
                            save_clicked = true;
                        }
                        if ui.button("🌐 브라우저에서 열기").on_hover_text("시스템 기본 브라우저에서 완전한 서식으로 즉시 열람합니다.").clicked() {
                            browser_clicked = true;
                        }
                        if ui.button("📝 텍스트 복사").clicked() {
                            copy_text_clicked = true;
                        }
                        if ui.button("📋 HTML 복사").clicked() {
                            copy_html_clicked = true;
                        }
                    });
                });

                if save_clicked { self.save_html_file(); }
                if browser_clicked { self.open_in_browser(); }
                if copy_text_clicked { self.copy_plain_text(); }
                if copy_html_clicked { self.copy_html(); }

                ui.separator();

                if self.view_mode == 0 {
                    // Preview mode with text and rendered embedded images
                    let mut open_browser_from_link = false;
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);

                        // Render rich preview
                        let plain = Self::html_to_plain_text(&res.html);
                        for line in plain.lines() {
                            if line.contains("[첨부 이미지]") {
                                ui.label(RichText::new("🖼️ [첨부 이미지]").strong().color(Color32::from_rgb(59, 130, 246)));
                                // Render embedded textures if available
                                for (_, tex) in &self.embedded_textures {
                                    let max_w = (ui.available_width() - 32.0).min(700.0);
                                    let size = tex.size_vec2();
                                    let scale = (max_w / size.x).min(1.0);
                                    let display_size = size * scale;
                                    ui.image((tex.id(), display_size));
                                }
                            } else {
                                ui.label(RichText::new(line).size(15.0));
                            }
                        }

                        ui.add_space(20.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("💡 서식(표, 굵게, 스타일 등)을 100% 동일하게 보려면 ").size(12.0).color(Color32::GRAY));
                            if ui.link("🌐 브라우저에서 열기").clicked() {
                                open_browser_from_link = true;
                            }
                        });
                    });
                    if open_browser_from_link {
                        self.open_in_browser();
                    }
                } else {
                    // HTML Source view
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let mut source = res.html.clone();
                        ui.add(
                            egui::TextEdit::multiline(&mut source)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(24),
                        );
                    });
                }
            } else {
                // Placeholder
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(RichText::new("iMemo 비표준 매트릭스 리더기").size(24.0).strong());
                    ui.label(RichText::new("Windows 네이티브 고속 캡처 & 서식 복원 브리지").size(14.0).color(Color32::GRAY));

                    ui.add_space(24.0);
                    egui::Frame::none()
                        .fill(Color32::from_gray(30))
                        .rounding(8.0)
                        .stroke(Stroke::new(1.0_f32, Color32::from_gray(50)))
                        .inner_margin(egui::Margin::same(20.0))
                        .show(ui, |ui| {
                            ui.set_max_width(520.0);
                            ui.label(RichText::new("사용 방법").strong().size(16.0));
                            ui.add_space(8.0);
                            ui.label("1. 웹 에디터(index.html)에서 문서를 작성하고 [이미지 생성]을 누릅니다.");
                            ui.label("2. 상단 [✂️ 영역 선택] 버튼을 눌러 원격 데스크톱/브라우저의 바코드 영역을 드래그합니다.");
                            ui.label("   (선택 즉시 실시간 좌표가 자동 저장되어 다음에도 그대로 재사용됩니다)");
                            ui.label("3. [▶ 캡처 시작]을 누르면 슬라이드가 자동 수집되어 서식 문서로 즉시 복원됩니다.");
                            ui.label("4. 캡처 도구로 복사한 이미지는 [클립보드 읽기 (Ctrl+V)]로 바로 인식할 수도 있습니다.");
                        });

                    ui.add_space(24.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.horizontal(|ui| {
                                if ui.button(RichText::new("✂️ 영역 선택하기").strong()).clicked() {
                                    self.select_region();
                                }
                                if ui.button(RichText::new("📋 클립보드 이미지 읽기 (Ctrl+V)").strong()).clicked() {
                                    self.paste_from_clipboard(ctx);
                                }
                                if ui.button("📁 이미지 파일 열기...").clicked() {
                                    self.open_image_file(ctx);
                                }
                            });
                        });
                    });
                });
            }
        });

        // Continuously repaint when capturing or showing banner
        if self.is_capturing.load(Ordering::SeqCst) || self.banner_message.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
