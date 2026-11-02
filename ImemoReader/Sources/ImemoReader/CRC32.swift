import Foundation

public struct CRC32 {
    private static let table: [UInt32] = {
        (0..<256).map { i -> UInt32 in
            var c = UInt32(i)
            for _ in 0..<8 {
                c = (c & 1 != 0) ? (0xEDB88320 ^ (c >> 1)) : (c >> 1)
            }
            return c
        }
    }()

    public static func checksum(_ data: Data) -> UInt32 {
        var crc: UInt32 = 0xFFFFFFFF
        for b in data {
            let idx = Int((crc ^ UInt32(b)) & 0xFF)
            crc = table[idx] ^ (crc >> 8)
        }
        return (crc ^ 0xFFFFFFFF)
    }

    public static func checksum(_ bytes: [UInt8]) -> UInt32 {
        var crc: UInt32 = 0xFFFFFFFF
        for b in bytes {
            let idx = Int((crc ^ UInt32(b)) & 0xFF)
            crc = table[idx] ^ (crc >> 8)
        }
        return (crc ^ 0xFFFFFFFF)
    }
}

