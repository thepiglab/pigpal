// swift-tools-version: 6.0
import PackageDescription
let package = Package(
    name: "PigPalSpeech",
    platforms: [.macOS(.v15)],
    products: [.executable(name: "pigpal-speech-macos", targets: ["SpeechHelper"])],
    dependencies: [.package(url: "https://github.com/FluidInference/FluidAudio.git", exact: "0.17.7")],
    targets: [.executableTarget(name: "SpeechHelper", dependencies: [.product(name: "FluidAudio", package: "FluidAudio")])]
)
