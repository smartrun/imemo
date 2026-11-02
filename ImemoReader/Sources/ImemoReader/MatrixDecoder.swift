import Foundation
import CoreGraphics
import AppKit

public struct DecodedFrame: Sendable {
    public let docId: UInt16
    public let frameSeq: Int
    public let totalFrames: Int
    public let dataFrameCount: Int
    public let payloadLen: Int
    public let docCrc32: UInt32
    public let payload: Data
    public let colorMode: Int
    public let widthCells: Int
    public let heightCells: Int
}

public final class MatrixDecoder {
    public static let standardPalettes: [[[Double]]] = [
        // Mode 0: 2 colors
        [ [0,0,0], [255,255,255] ],
        // Mode 1: 4 colors
        [ [0,0,0], [0,85,255], [255,221,0], [255,255,255] ],
        // Mode 2: 8 colors
        [ [0,0,0], [0,0,255], [0,255,0], [0,255,255], [255,0,0], [255,0,255], [255,255,0], [255,255,255] ],
        // Mode 3: 16 colors
        [
            [0,0,0], [0,0,128], [0,128,0], [0,128,128], [128,0,0], [128,0,128], [128,128,0], [192,192,192],
            [128,128,128], [0,0,255], [0,255,0], [0,255,255], [255,0,0], [255,0,255], [255,255,0], [255,255,255]
        ]
    ]

    public static func decodeImage(_ image: CGImage) -> DecodedFrame? {
        let width = image.width
        let height = image.height
        if width < 50 || height < 20 { return nil }

        // Extract RGBA buffer
        var rawData = [UInt8](repeating: 0, count: width * height * 4)
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo = CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue
        guard let ctx = CGContext(
            data: &rawData,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: width * 4,
            space: colorSpace,
            bitmapInfo: bitmapInfo
        ) else { return nil }

        ctx.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))

        // Find black outer bounding box (auto-trim white margins)
        guard let bounds = findBarcodeBounds(data: rawData, width: width, height: height) else {
            return nil
        }

        // Try decoding with estimated dimensions
        // Try common color modes (2, 4, 8, 16)
        for mode in [0, 1, 2, 3] {
            if let frame = tryDecodeWithBounds(data: rawData, width: width, height: height, bounds: bounds, forcedMode: mode) {
                return frame
            }
        }
        return nil
    }

    private struct BoundingBox {
        var minX: Int
        var minY: Int
        var maxX: Int
        var maxY: Int
        var width: Int { maxX - minX + 1 }
        var height: Int { maxY - minY + 1 }
    }

    private static func findBarcodeBounds(data: [UInt8], width: Int, height: Int) -> BoundingBox? {
        // Find outer dark perimeter inside the white quiet zone
        // We look for rows and columns that have a solid black line (outer border of IMC)
        var minX = width, maxX = 0, minY = height, maxY = 0

        func getLuminance(x: Int, y: Int) -> Double {
            let offset = (y * width + x) * 4
            let r = Double(data[offset])
            let g = Double(data[offset + 1])
            let b = Double(data[offset + 2])
            return 0.299 * r + 0.587 * g + 0.114 * b
        }

        var foundBlack = false
        // Sample with step 2 for speed
        for y in stride(from: 2, to: height - 2, by: 2) {
            for x in stride(from: 2, to: width - 2, by: 2) {
                if getLuminance(x: x, y: y) < 80.0 { // Dark pixel
                    foundBlack = true
                    if x < minX { minX = x }
                    if x > maxX { maxX = x }
                    if y < minY { minY = y }
                    if y > maxY { maxY = y }
                }
            }
        }

        if !foundBlack || (maxX - minX < 80) || (maxY - minY < 20) {
            return nil
        }

        // Refine edges: walk inward/outward to find exact black border edge
        return BoundingBox(minX: max(0, minX), minY: max(0, minY), maxX: min(width - 1, maxX), maxY: min(height - 1, maxY))
    }

    private static func tryDecodeWithBounds(
        data: [UInt8],
        width: Int,
        height: Int,
        bounds: BoundingBox,
        forcedMode: Int
    ) -> DecodedFrame? {
        let H = 28 // Fixed standard height in cells
        let cellH = Double(bounds.height) / Double(H)
        if cellH < 2.0 { return nil }

        // Estimate finder pattern size (7 cells high)
        // Cell width is approximately cellH (square cells)
        let estimatedW = Int(round(Double(bounds.width) / cellH))
        if estimatedW < 30 || estimatedW > 400 { return nil }

        // Test nearby W values (+-2) to account for roundoff
        for W in [estimatedW, estimatedW - 1, estimatedW + 1, estimatedW - 2, estimatedW + 2] {
            if W < 30 { continue }
            if let frame = sampleAndDecode(
                data: data,
                width: width,
                height: height,
                bounds: bounds,
                W: W,
                H: H,
                colorMode: forcedMode
            ) {
                return frame
            }
        }
        return nil
    }

    private static func sampleAndDecode(
        data: [UInt8],
        width: Int,
        height: Int,
        bounds: BoundingBox,
        W: Int,
        H: Int,
        colorMode: Int
    ) -> DecodedFrame? {
        let cellW = Double(bounds.width) / Double(W)
        let cellH = Double(bounds.height) / Double(H)

        func sampleRGB(col: Int, row: Int) -> (Double, Double, Double) {
            let cx = Double(bounds.minX) + (Double(col) + 0.5) * cellW
            let cy = Double(bounds.minY) + (Double(row) + 0.5) * cellH
            let ix = max(0, min(width - 1, Int(cx)))
            let iy = max(0, min(height - 1, Int(cy)))
            let offset = (iy * width + ix) * 4
            return (Double(data[offset]), Double(data[offset + 1]), Double(data[offset + 2]))
        }

        // Read Calibration Palette from Row 3 (cols 8 .. 8 + K - 1)
        let numColors = [2, 4, 8, 16][colorMode]
        var palette = [[Double]](repeating: [0, 0, 0], count: numColors)
        for i in 0..<numColors {
            let col = 8 + i
            if col < W - 7 {
                let (r1, g1, b1) = sampleRGB(col: col, row: 3)
                let (r2, g2, b2) = sampleRGB(col: col, row: H - 4)
                palette[i] = [(r1 + r2) / 2.0, (g1 + g2) / 2.0, (b1 + b2) / 2.0]
            } else {
                palette[i] = standardPalettes[colorMode][i]
            }
        }

        // Determine reserved cells
        var isReserved = [Bool](repeating: false, count: W * H)
        func markReserved(r: Int, c: Int) {
            if r >= 0 && r < H && c >= 0 && c < W {
                isReserved[r * W + c] = true
            }
        }

        // 1. Four 7x7 finders
        for r in 0..<7 {
            for c in 0..<7 {
                markReserved(r: r, c: c)
                markReserved(r: r, c: W - 7 + c)
                markReserved(r: H - 7 + r, c: c)
                markReserved(r: H - 7 + r, c: W - 7 + c)
            }
        }
        // 2. Borders
        for c in 0..<W {
            markReserved(r: 0, c: c)
            markReserved(r: H - 1, c: c)
        }
        for r in 0..<H {
            markReserved(r: r, c: 0)
            markReserved(r: r, c: W - 1)
        }
        // 3. Timing tracks
        for c in 7..<(W - 7) {
            markReserved(r: 1, c: c)
            markReserved(r: H - 2, c: c)
        }
        for r in 7..<(H - 7) {
            markReserved(r: r, c: 1)
            markReserved(r: r, c: W - 2)
        }
        // 4. Calibration Bars
        for i in 0..<numColors {
            markReserved(r: 3, c: 8 + i)
            markReserved(r: H - 4, c: 8 + i)
        }
        // 5. Format Metadata
        for i in 0..<8 {
            markReserved(r: 4, c: 8 + i)
            markReserved(r: H - 5, c: 8 + i)
        }

        // Collect data cell coordinates in serpentine order
        var dataCells = [(r: Int, c: Int)]()
        for r in 2..<(H - 2) {
            let isEven = (r % 2 == 0)
            for ci in 2..<(W - 2) {
                let c = isEven ? ci : (W - 1 - ci)
                if !isReserved[r * W + c] {
                    dataCells.append((r, c))
                }
            }
        }

        let bitsPerCell = [1, 2, 3, 4][colorMode]
        let totalBits = dataCells.count * bitsPerCell
        let totalBytes = totalBits / 8
        if totalBytes < 24 { return nil }

        // Classify each data cell into nearest palette index
        var symbols = [Int](repeating: 0, count: dataCells.count)
        for (idx, cell) in dataCells.enumerated() {
            let (sr, sg, sb) = sampleRGB(col: cell.c, row: cell.r)
            var bestIdx = 0
            var bestDist = Double.infinity
            for i in 0..<numColors {
                let dr = sr - palette[i][0]
                let dg = sg - palette[i][1]
                let db = sb - palette[i][2]
                let dist = dr * dr + dg * dg + db * db
                if dist < bestDist {
                    bestDist = dist
                    bestIdx = i
                }
            }
            symbols[idx] = bestIdx
        }

        // Unpack symbols to bytes
        var rawBytes = [UInt8](repeating: 0, count: totalBytes)
        var byteIdx = 0
        var bitOffset = 7

        if bitsPerCell == 1 {
            for sym in symbols {
                if byteIdx >= totalBytes { break }
                if sym == 1 {
                    rawBytes[byteIdx] |= (1 << bitOffset)
                }
                bitOffset -= 1
                if bitOffset < 0 { bitOffset = 7; byteIdx += 1 }
            }
        } else if bitsPerCell == 2 {
            for sym in symbols {
                if byteIdx >= totalBytes { break }
                rawBytes[byteIdx] |= UInt8((sym & 0x03) << (bitOffset - 1))
                bitOffset -= 2
                if bitOffset < 0 { bitOffset = 7; byteIdx += 1 }
            }
        } else if bitsPerCell == 3 {
            for sym in symbols {
                for bit in 0..<3 {
                    if byteIdx >= totalBytes { break }
                    let b = (sym >> (2 - bit)) & 1
                    if b == 1 {
                        rawBytes[byteIdx] |= (1 << bitOffset)
                    }
                    bitOffset -= 1
                    if bitOffset < 0 { bitOffset = 7; byteIdx += 1 }
                }
            }
        } else if bitsPerCell == 4 {
            for sym in symbols {
                if byteIdx >= totalBytes { break }
                rawBytes[byteIdx] |= UInt8((sym & 0x0F) << (bitOffset - 3))
                bitOffset -= 4
                if bitOffset < 0 { bitOffset = 7; byteIdx += 1 }
            }
        }

        // Unscramble all bytes with fixed non-standard keystream (0x494D434D)
        var scrambler = Scrambler(seed: 0x494D434D)
        var unscrambled = [UInt8](repeating: 0, count: totalBytes)
        for i in 0..<totalBytes {
            unscrambled[i] = rawBytes[i] ^ scrambler.nextByte()
        }

        // Multi-block Interleaved Reed-Solomon Decode
        let headerSize = 16
        let eccSize = max(10, Int(Double(totalBytes) * 0.30))
        let unencodedLen = totalBytes - eccSize

        guard let fixed = ReedSolomon.decodeBlocks(interleaved: unscrambled, dataLen: unencodedLen, eccBytes: eccSize) else {
            return nil
        }

        // Validate Header (16 bytes)
        // Byte 0..1: 'IM'
        if fixed[0] != 0x49 || fixed[1] != 0x4D {
            return nil
        }
        let dataFrameCount = Int(fixed[2])
        let docId = (UInt16(fixed[6]) << 8) | UInt16(fixed[7])
        let frameSeq = Int(fixed[8])
        let totalFrames = Int(fixed[9])
        let payloadLen = (Int(fixed[10]) << 8) | Int(fixed[11])
        let docCrc32 = (UInt32(fixed[12]) << 24) | (UInt32(fixed[13]) << 16) | (UInt32(fixed[14]) << 8) | UInt32(fixed[15])

        if payloadLen <= 0 || (headerSize + payloadLen) > (totalBytes - eccSize) {
            return nil
        }

        let payloadData = Data(fixed[headerSize..<(headerSize + payloadLen)])
        return DecodedFrame(
            docId: docId,
            frameSeq: frameSeq,
            totalFrames: totalFrames,
            dataFrameCount: dataFrameCount > 0 ? dataFrameCount : totalFrames,
            payloadLen: payloadLen,
            docCrc32: docCrc32,
            payload: payloadData,
            colorMode: colorMode,
            widthCells: W,
            heightCells: H
        )
    }
}
