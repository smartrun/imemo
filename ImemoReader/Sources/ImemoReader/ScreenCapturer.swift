import Foundation
import CoreGraphics
import AppKit
import ScreenCaptureKit

@MainActor
public final class ScreenCapturer: ObservableObject {
    @Published public var isCapturing: Bool = false
    @Published public var selectedRect: CGRect?
    @Published public var statusMessage: String = "캡처 대기 중"
    @Published public var frameChips: [Bool] = []
    @Published public var hasPermission: Bool = false
    @Published public var restoredResult: RestorationResult?
    @Published public var viewMode: Int = 0
    @Published public var copyBannerText: String? = nil

    private var timer: Timer?
    public let collector = SlideCollector()

    private let userDefaultsKey = "iMemoSelectedCaptureRect"

    public init() {
        checkPermission()
        loadSavedRect()
    }

    public func checkPermission() {
        hasPermission = CGPreflightScreenCaptureAccess()
    }

    public func requestPermission() {
        _ = CGRequestScreenCaptureAccess()
        checkPermission()
    }

    public func loadSavedRect() {
        if let str = UserDefaults.standard.string(forKey: userDefaultsKey) {
            let nsRect = NSRectFromString(str)
            if nsRect.width > 20 && nsRect.height > 20 {
                let rect = CGRect(x: nsRect.origin.x, y: nsRect.origin.y, width: nsRect.width, height: nsRect.height)
                self.selectedRect = rect
                self.statusMessage = "저장된 영역 로드됨: [X: \(Int(rect.origin.x)), Y: \(Int(rect.origin.y)), W: \(Int(rect.width)), H: \(Int(rect.height))]"
            }
        }
    }

    public func saveRect(_ rect: CGRect) {
        self.selectedRect = rect
        let str = NSStringFromRect(NSRect(x: rect.origin.x, y: rect.origin.y, width: rect.width, height: rect.height))
        UserDefaults.standard.set(str, forKey: userDefaultsKey)
        UserDefaults.standard.synchronize()
        self.statusMessage = "영역 자동 저장 완료: [X: \(Int(rect.origin.x)), Y: \(Int(rect.origin.y)), W: \(Int(rect.width)), H: \(Int(rect.height))]"
    }

    public func setManualRect(x: Double, y: Double, w: Double, h: Double) {
        guard w > 20 && h > 20 else { return }
        let rect = CGRect(x: x, y: y, width: w, height: h)
        saveRect(rect)
    }

    public func startCapture() {
        guard let rect = selectedRect, rect.width > 20 && rect.height > 20 else {
            statusMessage = "먼저 [영역 선택]으로 바코드 영역을 지정해주세요."
            return
        }

        checkPermission()
        if !hasPermission {
            requestPermission()
            statusMessage = "화면 기록 권한이 필요합니다. 시스템 설정에서 허용해주세요."
            return
        }

        isCapturing = true
        statusMessage = "화면 연속 캡처 및 인식 중... [X: \(Int(rect.origin.x)), Y: \(Int(rect.origin.y))]"
        collector.reset()
        updateChips()

        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            Task { @MainActor in
                await self?.captureTick()
            }
        }
    }

    public func stopCapture() {
        isCapturing = false
        timer?.invalidate()
        timer = nil
        statusMessage = "캡처 일시정지됨"
    }

    private func captureTick() async {
        guard isCapturing, let rect = selectedRect else { return }

        do {
            let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
            guard let display = content.displays.first else { return }
            let filter = SCContentFilter(display: display, excludingWindows: [])
            let config = SCStreamConfiguration()
            config.sourceRect = rect
            config.width = max(100, Int(rect.width * 2))
            config.height = max(50, Int(rect.height * 2))
            config.showsCursor = false

            let cgImage = try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: config)

            if let frame = MatrixDecoder.decodeImage(cgImage) {
                updateChips()
                if let result = collector.addFrame(frame) {
                    self.restoredResult = result
                    self.statusMessage = "문서 복원 완료! (크기: \(result.decompressedBytes.formatted()) B, 총 \(result.totalFrames)프레임)"
                    self.stopCapture()
                } else {
                    self.statusMessage = collector.progressText
                }
                updateChips()
            }
        } catch {
            self.statusMessage = "ScreenCaptureKit 오류: \(error.localizedDescription)"
        }
    }

    public func decodeSingleImage(_ cgImage: CGImage) {
        if let frame = MatrixDecoder.decodeImage(cgImage) {
            if let result = collector.addFrame(frame) {
                self.restoredResult = result
                self.statusMessage = "이미지에서 문서 복원 완료! (\(result.decompressedBytes.formatted()) B)"
            } else {
                self.statusMessage = "프레임 인식 완료: \(collector.progressText)"
            }
            updateChips()
        } else {
            self.statusMessage = "이미지에서 iMemo 코드를 검출하지 못했습니다."
        }
    }

    private func updateChips() {
        self.frameChips = collector.frameStatusList
    }

    public func selectRegion() {
        RegionSelectorWindowController.show { [weak self] rect in
            guard rect.width > 20 && rect.height > 20 else { return }
            self?.saveRect(rect)
        }
    }

    public func showBanner(_ msg: String) {
        copyBannerText = msg
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) { [weak self] in
            self?.copyBannerText = nil
        }
    }
}
