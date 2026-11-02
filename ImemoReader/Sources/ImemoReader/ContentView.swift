import SwiftUI
import WebKit
import UniformTypeIdentifiers

struct HTMLWebView: NSViewRepresentable {
    let htmlContent: String

    func makeNSView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        let webView = WKWebView(frame: .zero, configuration: config)
        return webView
    }

    func updateNSView(_ nsView: WKWebView, context: Context) {
        let styledHTML = """
        <!DOCTYPE html>
        <html>
        <head>
          <meta charset="utf-8">
          <style>
            body {
              font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Apple SD Gothic Neo", sans-serif;
              padding: 24px;
              color: #1e293b;
              line-height: 1.6;
              font-size: 15px;
            }
            table {
              border-collapse: collapse;
              width: 100%;
              margin: 16px 0;
            }
            th, td {
              border: 1px solid #cbd5e1;
              padding: 8px 12px;
              text-align: left;
            }
            th {
              background: #f8fafc;
            }
            img {
              max-width: 100%;
              height: auto;
              border-radius: 6px;
              box-shadow: 0 2px 8px rgba(0,0,0,0.1);
              margin: 10px 0;
            }
          </style>
        </head>
        <body>
          \(htmlContent)
        </body>
        </html>
        """
        nsView.loadHTMLString(styledHTML, baseURL: nil)
    }
}

public struct ContentView: View {
    @StateObject private var capturer = ScreenCapturer()

    public init() {}

    public var body: some View {
        VStack(spacing: 0) {
            // Top Toolbar
            toolbarView
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .background(Color(NSColor.windowBackgroundColor))
                .overlay(Rectangle().frame(height: 1).foregroundColor(Color(NSColor.separatorColor)), alignment: .bottom)

            // Status Bar & Coordinates & Frame Chips
            statusSection
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(Color(NSColor.controlBackgroundColor))
                .overlay(Rectangle().frame(height: 1).foregroundColor(Color(NSColor.separatorColor)), alignment: .bottom)

            // Main Content Area
            ZStack {
                if let result = capturer.restoredResult {
                    VStack(spacing: 0) {
                        // Mode Switcher Bar
                        HStack {
                            Picker("", selection: $capturer.viewMode) {
                                Text("서식 미리보기").tag(0)
                                Text("HTML 소스").tag(1)
                            }
                            .pickerStyle(.segmented)
                            .frame(width: 200)

                            Spacer()

                            if let banner = capturer.copyBannerText {
                                Text(banner)
                                    .font(.system(size: 12, weight: .bold))
                                    .foregroundColor(.green)
                                    .transition(.opacity)
                            }

                            Button(action: copyHTML) {
                                Label("HTML 복사", systemImage: "doc.on.doc")
                            }

                            Button(action: copyPlainText) {
                                Label("텍스트 복사", systemImage: "text.quote")
                            }

                            Button(action: saveHTMLFile) {
                                Label("저장...", systemImage: "square.and.arrow.down")
                            }
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                        .background(Color(NSColor.controlBackgroundColor))

                        Divider()

                        if capturer.viewMode == 0 {
                            HTMLWebView(htmlContent: result.html)
                        } else {
                            ScrollView {
                                Text(result.html)
                                    .font(.system(.body, design: .monospaced))
                                    .padding()
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .textSelection(.enabled)
                            }
                        }
                    }
                } else {
                    placeholderView
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .onDrop(of: [.image], isTargeted: nil) { providers in
                handleDrop(providers: providers)
            }
        }
        .frame(minWidth: 850, minHeight: 620)
    }

    // MARK: - Subviews

    private var toolbarView: some View {
        HStack(spacing: 12) {
            Text("iMemo 리더기")
                .font(.headline)
                .bold()

            Spacer()

            Button(action: { capturer.selectRegion() }) {
                Label("영역 선택", systemImage: "crop")
            }
            .help("원격 화면의 바코드 영역을 마우스로 드래그하여 지정합니다.")

            if capturer.isCapturing {
                Button(action: { capturer.stopCapture() }) {
                    Label("캡처 중지", systemImage: "stop.circle.fill")
                        .foregroundColor(.red)
                }
            } else {
                Button(action: { capturer.startCapture() }) {
                    Label("캡처 시작", systemImage: "play.circle.fill")
                        .foregroundColor(.green)
                }
            }

            Divider().frame(height: 18)

            Button(action: openImageFile) {
                Label("이미지 파일 열기...", systemImage: "folder")
            }

            Button(action: resetAll) {
                Label("초기화", systemImage: "arrow.counterclockwise")
            }
        }
    }

    private var statusSection: some View {
        HStack(spacing: 12) {
            Circle()
                .fill(capturer.isCapturing ? Color.green : (capturer.restoredResult != nil ? Color.blue : Color.gray))
                .frame(width: 10, height: 10)

            Text(capturer.statusMessage)
                .font(.system(size: 13, weight: .medium))

            Spacer()

            // Selected Coordinates Display Badge
            if let rect = capturer.selectedRect {
                HStack(spacing: 6) {
                    Image(systemName: "checkmark.seal.fill")
                        .foregroundColor(.green)
                        .font(.system(size: 11))
                    Text("좌표:")
                        .font(.system(size: 11, weight: .bold))
                    Text("X: \(Int(rect.origin.x)), Y: \(Int(rect.origin.y)), W: \(Int(rect.width)), H: \(Int(rect.height))")
                        .font(.system(size: 11, weight: .semibold, design: .monospaced))
                    Text("(자동 저장됨)")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(Color(NSColor.textBackgroundColor))
                .cornerRadius(6)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.secondary.opacity(0.2), lineWidth: 1))
            } else {
                HStack(spacing: 4) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundColor(.orange)
                        .font(.system(size: 11))
                    Text("영역 미선택 (좌표 없음)")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                }
            }

            // Frame Progress Chips
            if !capturer.frameChips.isEmpty {
                HStack(spacing: 4) {
                    ForEach(0..<capturer.frameChips.count, id: \.self) { idx in
                        let received = capturer.frameChips[idx]
                        Text("\(idx + 1)")
                            .font(.system(size: 11, weight: .bold))
                            .frame(width: 22, height: 20)
                            .background(received ? Color.green : Color.gray.opacity(0.3))
                            .foregroundColor(received ? .white : .primary)
                            .cornerRadius(4)
                    }
                }
            }
        }
    }

    private var placeholderView: some View {
        VStack(spacing: 16) {
            Image(systemName: "qrcode.viewfinder")
                .font(.system(size: 64))
                .foregroundColor(.secondary)

            Text("iMemo 비표준 매트릭스 리더기")
                .font(.title2)
                .bold()

            VStack(alignment: .leading, spacing: 8) {
                Text("1. 웹 에디터(index.html)에서 문서를 작성하고 [이미지 생성]을 클릭합니다.")
                Text("2. 상단 [영역 선택] 버튼을 눌러 원격 데스크톱 창의 코드 영역을 드래그합니다.")
                Text("   (드래그 시 화면에 실시간 좌표가 표시되며, 완료 즉시 자동 저장됩니다)")
                Text("3. [캡처 시작]을 누르면 슬라이드가 자동으로 감지되어 문서로 복원됩니다.")
            }
            .font(.system(size: 13))
            .foregroundColor(.secondary)
            .padding(16)
            .background(Color(NSColor.controlBackgroundColor))
            .cornerRadius(8)

            // Saved Coordinates Indicator in Placeholder
            if let rect = capturer.selectedRect {
                HStack(spacing: 8) {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundColor(.green)
                    Text("저장된 캡처 영역 좌표:")
                        .bold()
                    Text("X: \(Int(rect.origin.x)), Y: \(Int(rect.origin.y)), W: \(Int(rect.width)), H: \(Int(rect.height)) px")
                        .font(.system(.body, design: .monospaced))
                        .bold()
                }
                .font(.system(size: 13))
                .padding(.vertical, 8)
                .padding(.horizontal, 14)
                .background(Color.green.opacity(0.08))
                .cornerRadius(6)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(Color.green.opacity(0.2), lineWidth: 1))
            }

            HStack(spacing: 12) {
                Button("영역 선택하기") {
                    capturer.selectRegion()
                }
                .buttonStyle(.borderedProminent)

                Button("이미지 파일 열기") {
                    openImageFile()
                }
            }
        }
        .padding(40)
    }

    // MARK: - Actions

    private func resetAll() {
        capturer.stopCapture()
        capturer.collector.reset()
        capturer.restoredResult = nil
        capturer.statusMessage = "초기화되었습니다."
    }

    private func copyHTML() {
        guard let html = capturer.restoredResult?.html else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(html, forType: .string)
        showBanner("HTML이 클립보드에 복사되었습니다!")
    }

    private func copyPlainText() {
        guard let html = capturer.restoredResult?.html else { return }
        var plain = html.replacingOccurrences(of: "<img[^>]*>", with: " [첨부 이미지] ", options: .regularExpression, range: nil)
        plain = plain.replacingOccurrences(of: "<[^>]+>", with: "", options: .regularExpression, range: nil)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(plain, forType: .string)
        showBanner("일반 텍스트가 복사되었습니다!")
    }

    private func showBanner(_ msg: String) {
        capturer.showBanner(msg)
    }

    private func saveHTMLFile() {
        guard let html = capturer.restoredResult?.html else { return }
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.html]
        panel.nameFieldStringValue = "iMemo_Restored.html"
        panel.begin { response in
            if response == .OK, let url = panel.url {
                try? html.write(to: url, atomically: true, encoding: .utf8)
            }
        }
    }

    private func openImageFile() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = false
        panel.begin { response in
            if response == .OK, let url = panel.url, let image = NSImage(contentsOf: url) {
                if let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) {
                    capturer.decodeSingleImage(cgImage)
                }
            }
        }
    }

    private func handleDrop(providers: [NSItemProvider]) -> Bool {
        guard let provider = providers.first else { return false }
        provider.loadDataRepresentation(forTypeIdentifier: UTType.image.identifier) { data, error in
            guard let data = data, let image = NSImage(data: data),
                  let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else { return }
            DispatchQueue.main.async {
                capturer.decodeSingleImage(cgImage)
            }
        }
        return true
    }
}
