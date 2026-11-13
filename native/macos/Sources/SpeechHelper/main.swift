import Foundation
// Core ML predates Sendable annotations; older Swift 6 compilers (Xcode 16)
// otherwise reject passing its MLModel values across concurrency domains.
@preconcurrency import CoreML
import FluidAudio

// A small platform adapter. The Rust app owns sessions, original audio and transcripts.
@main struct SpeechHelper {
    static func main() async {
        do {
            AppLogger.minimumLevel = .error
            AppLogger.mirrorsToConsole = false
            let args = Array(CommandLine.arguments.dropFirst())
            guard args.count >= 2 else { throw Failure.usage }
            let directory = URL(fileURLWithPath: args[1], isDirectory: true)
            guard args[0] == "download" || args[0] == "transcribe" else { throw Failure.usage }
            let config = MLModelConfiguration()
            #if arch(arm64)
            // Metal avoids ANE compiler stalls observed on this Mac. ANE remains opt-in.
            let environment = ProcessInfo.processInfo.environment
            let useANE = (environment["PIGPAL_COREML_COMPUTE"] ?? environment["SURGERYBUDDY_COREML_COMPUTE"]) == "ane"
            config.computeUnits = useANE ? .cpuAndNeuralEngine : .cpuAndGPU
            #else
            config.computeUnits = .cpuOnly
            #endif
            if args[0] == "download" {
                guard directory.lastPathComponent == "parakeet-redux" else { throw Failure.usage }
                for name in ["Preprocessor.mlmodelc", "Encoder.mlmodelc", "Decoder.mlmodelc", "JointDecisionv3.mlmodelc"] {
                    _ = try await ModelHub.loadModels(.parakeetRedux, modelNames: [name],
                        directory: directory.deletingLastPathComponent(),
                        computeUnits: name == "Preprocessor.mlmodelc" ? .cpuOnly : config.computeUnits)
                }
                let vocab = directory.appendingPathComponent("parakeet_vocab.json")
                if !FileManager.default.fileExists(atPath: vocab.path) {
                    let url = try ModelRegistry.resolveModel(Repo.parakeetRedux.rawValue, "parakeet_vocab.json")
                    try await ModelHub.fetchFile(from: url, description: "Redux vocabulary").write(to: vocab, options: .atomic)
                }
            }
            // This loader never contacts a model hub. It also verifies a completed download.
            let models = try AsrModels.loadLocal(from: directory, version: .redux, configuration: config,
                                                encoderComputeUnits: config.computeUnits)
            let manager = AsrManager(config: ASRConfig(parallelChunkConcurrency: 1))
            try await manager.loadModels(models)
            if args[0] == "download" {
                print("Redux ready locally")
                return
            }
            guard args.count == 4 else { throw Failure.usage }
            var state = try TdtDecoderState()
            let result = try await manager.transcribe(URL(fileURLWithPath: args[2]), decoderState: &state)
            let words: [[String: Any]] = (result.tokenTimings ?? []).map {
                ["text": $0.token, "start": $0.startTime, "end": $0.endTime, "confidence": $0.confidence]
            }
            let payload: [String: Any] = ["text": result.text, "words": words,
                                         "engine": "FluidAudio 0.17.7 / Core ML", "compute_units": config.computeUnits.rawValue,
                                         "raw": try JSONSerialization.jsonObject(with: JSONEncoder().encode(result))]
            try JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted,.sortedKeys])
                .write(to: URL(fileURLWithPath: args[3]), options: .atomic)
        } catch {
            FileHandle.standardError.write(Data("Speech error: \(error)\n".utf8))
            exit(1)
        }
    }
    enum Failure: Error { case usage }
}
