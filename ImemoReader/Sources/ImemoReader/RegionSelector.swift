import AppKit
import SwiftUI

public final class RegionSelectorWindowController: NSWindowController {
    public var onRegionSelected: ((CGRect) -> Void)?

    public static func show(onSelected: @escaping (CGRect) -> Void) {
        guard let screen = NSScreen.main else { return }
        let overlayView = SelectionOverlayView(frame: screen.frame)
        let window = NSWindow(
            contentRect: screen.frame,
            styleMask: [.borderless],
            backing: .buffered,
            defer: false,
            screen: screen
        )
        window.isOpaque = false
        window.backgroundColor = .clear
        window.level = .floating
        window.ignoresMouseEvents = false
        window.contentView = overlayView

        let controller = RegionSelectorWindowController(window: window)
        controller.onRegionSelected = onSelected
        overlayView.onFinish = { [weak controller, weak window] rect in
            window?.close()
            controller?.onRegionSelected?(rect)
        }

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
}

final class SelectionOverlayView: NSView {
    var onFinish: ((CGRect) -> Void)?
    private var startPoint: NSPoint?
    private var currentPoint: NSPoint?

    override init(frame: NSRect) {
        super.init(frame: frame)
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .crosshair)
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)

        // Semi-transparent dark overlay
        NSColor.black.withAlphaComponent(0.3).setFill()
        bounds.fill()

        guard let start = startPoint, let curr = currentPoint else {
            // Draw instruction banner
            let text = "원격 화면의 바코드 영역을 마우스로 드래그하여 선택하세요 (ESC: 취소)"
            let attrs: [NSAttributedString.Key: Any] = [
                .font: NSFont.boldSystemFont(ofSize: 18),
                .foregroundColor: NSColor.white
            ]
            let size = (text as NSString).size(withAttributes: attrs)
            let pt = NSPoint(x: (bounds.width - size.width) / 2, y: bounds.height - 100)
            (text as NSString).draw(at: pt, withAttributes: attrs)
            return
        }

        let selectionRect = NSRect(
            x: min(start.x, curr.x),
            y: min(start.y, curr.y),
            width: abs(curr.x - start.x),
            height: abs(curr.y - start.y)
        )

        // Clear hole inside selection
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current?.compositingOperation = .clear
        let path = NSBezierPath(rect: selectionRect)
        path.fill()

        // Border around selection
        NSColor.systemGreen.setStroke()
        let strokePath = NSBezierPath(rect: selectionRect)
        strokePath.lineWidth = 2.5
        strokePath.setLineDash([6.0, 4.0], count: 2, phase: 0.0)
        strokePath.stroke()

        // Size badge
        let info = "\(Int(selectionRect.width)) × \(Int(selectionRect.height)) px"
        let badgeAttrs: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedSystemFont(ofSize: 13, weight: .bold),
            .foregroundColor: NSColor.white
        ]
        let badgeSize = (info as NSString).size(withAttributes: badgeAttrs)
        let badgeRect = NSRect(
            x: selectionRect.minX,
            y: max(0, selectionRect.minY - 24),
            width: badgeSize.width + 12,
            height: 20
        )
        NSColor.black.withAlphaComponent(0.8).setFill()
        NSBezierPath(roundedRect: badgeRect, xRadius: 4, yRadius: 4).fill()
        (info as NSString).draw(at: NSPoint(x: badgeRect.minX + 6, y: badgeRect.minY + 2), withAttributes: badgeAttrs)

        NSGraphicsContext.restoreGraphicsState()
    }

    override func mouseDown(with event: NSEvent) {
        startPoint = convert(event.locationInWindow, from: nil)
        currentPoint = startPoint
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        currentPoint = convert(event.locationInWindow, from: nil)
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard let start = startPoint, let curr = currentPoint else { return }
        let nsRect = NSRect(
            x: min(start.x, curr.x),
            y: min(start.y, curr.y),
            width: abs(curr.x - start.x),
            height: abs(curr.y - start.y)
        )

        guard nsRect.width > 20 && nsRect.height > 20 else {
            // Cancel if too small
            onFinish?(CGRect.zero)
            return
        }

        // Convert NSScreen coordinates to CoreGraphics display coordinates (flip Y)
        guard let screen = window?.screen ?? NSScreen.main else { return }
        let cgY = screen.frame.height - nsRect.origin.y - nsRect.height
        let cgRect = CGRect(x: nsRect.origin.x, y: cgY, width: nsRect.width, height: nsRect.height)

        onFinish?(cgRect)
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 { // ESC
            window?.close()
        }
    }
}

