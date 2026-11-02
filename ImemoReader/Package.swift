// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "ImemoReader",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(
            name: "ImemoReader",
            targets: ["ImemoReader"]
        )
    ],
    targets: [
        .executableTarget(
            name: "ImemoReader",
            path: "Sources/ImemoReader"
        )
    ]
)

