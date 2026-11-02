import Foundation
import Compression

public struct RestorationResult: Sendable {
    public let html: String
    public let docId: UInt16
    public let totalFrames: Int
    public let compressedBytes: Int
    public let decompressedBytes: Int
}

public final class SlideCollector: @unchecked Sendable {
    private let lock = NSLock()
    public private(set) var currentDocId: UInt16?
    public private(set) var totalFrames: Int = 0
    public private(set) var dataFrameCount: Int = 0
    public private(set) var docCrc32: UInt32 = 0
    public private(set) var receivedFrames: [Int: Data] = [:]

    public init() {}

    public func reset() {
        lock.lock()
        defer { lock.unlock() }
        currentDocId = nil
        totalFrames = 0
        dataFrameCount = 0
        docCrc32 = 0
        receivedFrames.removeAll()
    }

    public var progressText: String {
        lock.lock()
        defer { lock.unlock() }
        guard let _ = currentDocId, totalFrames > 0 else {
            return "대기 중 (코드 미검출)"
        }
        let count = receivedFrames.count
        return "프레임 수집 중: \(count) / \(totalFrames) (데이터: \(dataFrameCount)장)"
    }

    public var frameStatusList: [Bool] {
        lock.lock()
        defer { lock.unlock() }
        guard totalFrames > 0 else { return [] }
        return (0..<totalFrames).map { receivedFrames[$0] != nil }
    }

    public func addFrame(_ frame: DecodedFrame) -> RestorationResult? {
        lock.lock()
        defer { lock.unlock() }

        // If new document arrived, reset
        if currentDocId != frame.docId {
            currentDocId = frame.docId
            totalFrames = frame.totalFrames
            dataFrameCount = frame.dataFrameCount
            docCrc32 = frame.docCrc32
            receivedFrames.removeAll()
        }

        // Store this frame payload
        receivedFrames[frame.frameSeq] = frame.payload

        // Check if we can assemble the document
        // Case 1: All data frames received (0 ..< dataFrameCount)
        var missingDataSeq: [Int] = []
        for i in 0..<dataFrameCount {
            if receivedFrames[i] == nil {
                missingDataSeq.append(i)
            }
        }

        // Case 2: Exactly 1 data frame missing, but parity frame is available!
        // Parity frame is index `dataFrameCount`
        if missingDataSeq.count == 1, let parity = receivedFrames[dataFrameCount] {
            let missingIdx = missingDataSeq[0]
            var recovered = [UInt8](parity)
            for d in 0..<dataFrameCount {
                if d != missingIdx, let otherData = receivedFrames[d] {
                    for k in 0..<min(recovered.count, otherData.count) {
                        recovered[k] ^= otherData[k]
                    }
                }
            }
            // Trim to match payload length or let decompressor handle
            receivedFrames[missingIdx] = Data(recovered)
            missingDataSeq.removeAll()
        }

        if missingDataSeq.isEmpty {
            // Concatenate all data chunks
            var fullData = Data()
            for d in 0..<dataFrameCount {
                if let chunk = receivedFrames[d] {
                    fullData.append(chunk)
                }
            }

            // Verify CRC32
            let computedCrc = CRC32.checksum(fullData)
            if computedCrc == docCrc32 {
                // Decompress
                if let htmlString = decompressToHTML(fullData) {
                    return RestorationResult(
                        html: htmlString,
                        docId: frame.docId,
                        totalFrames: frame.totalFrames,
                        compressedBytes: fullData.count,
                        decompressedBytes: htmlString.utf8.count
                    )
                }
            }
        }

        return nil
    }

    private func decompressToHTML(_ data: Data) -> String? {
        // Try Apple NSData decompression (handles raw deflate)
        if let decomp = try? (data as NSData).decompressed(using: .zlib) as Data,
           let str = String(data: decomp, encoding: .utf8) {
            return str
        }

        // If data has RFC 1950 header (0x78), strip 2-byte header and 4-byte trailer
        if data.count > 6 && data[0] == 0x78 {
            let stripped = data.subdata(in: 2..<(data.count - 4))
            if let decomp = try? (stripped as NSData).decompressed(using: .zlib) as Data,
               let str = String(data: decomp, encoding: .utf8) {
                return str
            }
        }

        // Try direct buffer decompression
        let destSize = max(2097152, data.count * 15)
        let dest = UnsafeMutablePointer<UInt8>.allocate(capacity: destSize)
        defer { dest.deallocate() }

        let written = data.withUnsafeBytes { src -> Int in
            guard let base = src.bindMemory(to: UInt8.self).baseAddress else { return 0 }
            return compression_decode_buffer(dest, destSize, base, data.count, nil, COMPRESSION_ZLIB)
        }

        if written > 0 {
            let decompData = Data(bytes: dest, count: written)
            if let str = String(data: decompData, encoding: .utf8) {
                return str
            }
        }

        return nil
    }
}

