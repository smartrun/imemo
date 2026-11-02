import Foundation

public final class ReedSolomon {
    private static let primitive = 0x11D
    public static let expTable: [UInt16] = {
        var exp = [UInt16](repeating: 0, count: 512)
        var log = [UInt16](repeating: 0, count: 256)
        var x: Int = 1
        for i in 0..<255 {
            exp[i] = UInt16(x)
            log[x] = UInt16(i)
            x <<= 1
            if (x & 0x100) != 0 {
                x ^= primitive
            }
        }
        for i in 255..<512 {
            exp[i] = exp[i - 255]
        }
        return exp
    }()

    public static let logTable: [UInt16] = {
        var log = [UInt16](repeating: 0, count: 256)
        var x: Int = 1
        for i in 0..<255 {
            log[x] = UInt16(i)
            x <<= 1
            if (x & 0x100) != 0 {
                x ^= primitive
            }
        }
        return log
    }()

    @inline(__always)
    public static func gfMul(_ a: UInt8, _ b: UInt8) -> UInt8 {
        if a == 0 || b == 0 { return 0 }
        let idx = Int(logTable[Int(a)]) + Int(logTable[Int(b)])
        return UInt8(expTable[idx])
    }

    @inline(__always)
    public static func gfInv(_ a: UInt8) -> UInt8 {
        if a == 0 { return 0 }
        let idx = 255 - Int(logTable[Int(a)])
        return UInt8(expTable[idx])
    }

    public static func rsGeneratorPoly(nSym: Int) -> [UInt8] {
        var g: [UInt8] = [1]
        for i in 0..<nSym {
            let p: [UInt8] = [1, UInt8(expTable[i])]
            var ng = [UInt8](repeating: 0, count: g.count + 1)
            for j in 0..<g.count {
                ng[j] ^= g[j]
                ng[j + 1] ^= gfMul(g[j], p[1])
            }
            g = ng
        }
        return g
    }

    public static func rsEncode(msg: [UInt8], nSym: Int) -> [UInt8] {
        let gen = rsGeneratorPoly(nSym: nSym)
        var out = msg + [UInt8](repeating: 0, count: nSym)
        for i in 0..<msg.count {
            let coef = out[i]
            if coef != 0 {
                for j in 1..<gen.count {
                    out[i + j] ^= gfMul(gen[j], coef)
                }
            }
        }
        return msg + Array(out[msg.count...])
    }

    public static func calcSyndromes(_ msg: [UInt8], nSym: Int) -> [UInt8] {
        var syn = [UInt8](repeating: 0, count: nSym)
        for i in 0..<nSym {
            let a = UInt8(expTable[i])
            var v: UInt8 = 0
            for b in msg {
                v = gfMul(v, a) ^ b
            }
            syn[i] = v
        }
        return syn
    }

    public static func findErrorLocator(_ syn: [UInt8], nSym: Int) -> [UInt8] {
        var C: [UInt8] = [1]
        var B: [UInt8] = [1]
        var L = 0
        var m = 1
        var b: UInt8 = 1

        for n in 0..<nSym {
            var d = syn[n]
            if L > 0 {
                for i in 1...L {
                    if i < C.count {
                        d ^= gfMul(C[i], syn[n - i])
                    }
                }
            }
            if d == 0 {
                m += 1
            } else {
                let T = C
                let scale = gfMul(d, gfInv(b))
                var diff = [UInt8](repeating: 0, count: m)
                for x in B {
                    diff.append(gfMul(scale, x))
                }
                while C.count < diff.count {
                    C.append(0)
                }
                for i in 0..<diff.count {
                    C[i] ^= diff[i]
                }
                if 2 * L <= n {
                    L = n + 1 - L
                    B = T
                    b = d
                    m = 1
                } else {
                    m += 1
                }
            }
        }
        return C
    }

    public static func findErrors(_ C: [UInt8], msgLen: Int) -> [Int]? {
        var deg = 0
        for i in stride(from: C.count - 1, through: 0, by: -1) {
            if C[i] != 0 {
                deg = i
                break
            }
        }
        if deg == 0 { return [] }

        var errPos = [Int]()
        for i in 0..<msgLen {
            let power = (255 - (i % 255)) % 255
            let ai = UInt8(expTable[power])
            var val: UInt8 = 0
            var term: UInt8 = 1
            for j in 0..<C.count {
                val ^= gfMul(C[j], term)
                term = gfMul(term, ai)
            }
            if val == 0 {
                errPos.append(msgLen - 1 - i)
            }
        }

        if errPos.count != deg {
            return nil
        }
        return errPos
    }

    public static func solveErrors(_ msg: [UInt8], syn: [UInt8], errPos: [Int]) -> [UInt8]? {
        let m = errPos.count
        if m == 0 { return msg }

        var X = [UInt8]()
        for p in errPos {
            let power = (msg.count - 1 - p) % 255
            X.append(UInt8(expTable[power]))
        }

        var A = [[UInt8]](repeating: [UInt8](repeating: 0, count: m + 1), count: m)
        for i in 0..<m {
            for k in 0..<m {
                let p = (Int(logTable[Int(X[k])]) * i) % 255
                A[i][k] = UInt8(expTable[p])
            }
            A[i][m] = syn[i]
        }

        for col in 0..<m {
            var pivot = -1
            for row in col..<m {
                if A[row][col] != 0 {
                    pivot = row
                    break
                }
            }
            if pivot == -1 { return nil }
            if pivot != col {
                A.swapAt(col, pivot)
            }
            let inv = gfInv(A[col][col])
            for c in col...m {
                A[col][c] = gfMul(A[col][c], inv)
            }
            for row in 0..<m {
                if row != col && A[row][col] != 0 {
                    let factor = A[row][col]
                    for c in col...m {
                        A[row][c] ^= gfMul(factor, A[col][c])
                    }
                }
            }
        }

        var res = msg
        for (k, p) in errPos.enumerated() {
            res[p] ^= A[k][m]
        }
        return res
    }

    public static func decode(_ msg: [UInt8], nSym: Int) -> [UInt8]? {
        let syn = calcSyndromes(msg, nSym: nSym)
        let hasError = syn.contains { $0 != 0 }
        if !hasError {
            return msg
        }

        let C = findErrorLocator(syn, nSym: nSym)
        guard let errPos = findErrors(C, msgLen: msg.count) else {
            return nil
        }

        guard let fixed = solveErrors(msg, syn: syn, errPos: errPos) else {
            return nil
        }

        let checkSyn = calcSyndromes(fixed, nSym: nSym)
        if checkSyn.contains(where: { $0 != 0 }) {
            return nil
        }
        return fixed
    }

    // MARK: - Block Interleaved RS (GF(256) Block size <= 200)

    public static func encodeBlocks(data: [UInt8], eccBytes: Int) -> [UInt8] {
        let total = data.count + eccBytes
        let numBlocks = max(1, (total + 179) / 180)
        let eccPerBlock = eccBytes / numBlocks

        var blocksData = [[UInt8]](repeating: [], count: numBlocks)
        for bi in 0..<numBlocks {
            let start = bi * data.count / numBlocks
            let end = (bi + 1) * data.count / numBlocks
            blocksData[bi] = Array(data[start..<end])
        }

        var encodedBlocks = [[UInt8]](repeating: [], count: numBlocks)
        for bi in 0..<numBlocks {
            encodedBlocks[bi] = rsEncode(msg: blocksData[bi], nSym: eccPerBlock)
        }

        var interleaved = [UInt8]()
        let maxLen = encodedBlocks.map { $0.count }.max() ?? 0
        for col in 0..<maxLen {
            for bi in 0..<numBlocks {
                if col < encodedBlocks[bi].count {
                    interleaved.append(encodedBlocks[bi][col])
                }
            }
        }
        return interleaved
    }

    public static func decodeBlocks(interleaved: [UInt8], dataLen: Int, eccBytes: Int) -> [UInt8]? {
        let total = dataLen + eccBytes
        let numBlocks = max(1, (total + 179) / 180)
        let eccPerBlock = eccBytes / numBlocks

        // Calculate expected block sizes
        var blockDataLens = [Int](repeating: 0, count: numBlocks)
        for bi in 0..<numBlocks {
            let start = bi * dataLen / numBlocks
            let end = (bi + 1) * dataLen / numBlocks
            blockDataLens[bi] = end - start
        }

        let blockTotalLens = blockDataLens.map { $0 + eccPerBlock }
        let maxLen = blockTotalLens.max() ?? 0

        var deinterleaved = [[UInt8]](repeating: [], count: numBlocks)
        var idx = 0
        for col in 0..<maxLen {
            for bi in 0..<numBlocks {
                if col < blockTotalLens[bi] && idx < interleaved.count {
                    deinterleaved[bi].append(interleaved[idx])
                    idx += 1
                }
            }
        }

        var recovered = [UInt8]()
        for bi in 0..<numBlocks {
            guard let fixed = decode(deinterleaved[bi], nSym: eccPerBlock) else {
                print("DEBUG: decode failed on block \(bi), size \(deinterleaved[bi].count), nSym \(eccPerBlock)")
                return nil
            }
            recovered.append(contentsOf: fixed[0..<blockDataLens[bi]])
        }
        return recovered
    }
}
