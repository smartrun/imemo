import Foundation

public struct Scrambler {
    private var state: UInt32

    public init(seed: UInt32) {
        self.state = seed != 0 ? seed : 0x12345678
    }

    public mutating func nextByte() -> UInt8 {
        state ^= (state << 13)
        state ^= (state >> 17)
        state ^= (state << 5)
        return UInt8(state & 0xFF)
    }
}

