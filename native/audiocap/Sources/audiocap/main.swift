// murmur-audiocap: captures one audio source and writes 16 kHz mono Float32 PCM
// (little-endian) to stdout for the Rust core.
//
//   audiocap --source system   Core Audio process tap of all system output (macOS 14.4+)
//   audiocap --source mic      default input device via AVAudioEngine
//
// Diagnostics go to stderr so stdout stays a clean PCM stream. The process exits
// when stdin closes (the parent stopped or died), on SIGINT/SIGTERM, or on a
// broken stdout pipe.

import AVFoundation
import CoreAudio
import Foundation

let targetFormat = AVAudioFormat(
    commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!

func log(_ message: String) {
    FileHandle.standardError.write(("audiocap: " + message + "\n").data(using: .utf8)!)
}

func fail(_ message: String, code: Int32 = 2) -> Never {
    log(message)
    exit(code)
}

func check(_ status: OSStatus, _ what: String) {
    if status != noErr { fail("\(what) failed (OSStatus \(status))") }
}

/// Converts arbitrary input buffers to 16 kHz mono and writes them to stdout.
final class PCMWriter {
    private let converter: AVAudioConverter

    init(from format: AVAudioFormat) {
        guard let converter = AVAudioConverter(from: format, to: targetFormat) else {
            fail("unsupported input format \(format)")
        }
        self.converter = converter
    }

    func write(_ input: AVAudioPCMBuffer) {
        let ratio = targetFormat.sampleRate / input.format.sampleRate
        let capacity = AVAudioFrameCount(Double(input.frameLength) * ratio) + 64
        guard let output = AVAudioPCMBuffer(pcmFormat: targetFormat, frameCapacity: capacity) else { return }

        var consumed = false
        var error: NSError?
        converter.convert(to: output, error: &error) { _, status in
            if consumed {
                status.pointee = .noDataNow
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return input
        }
        if let error { fail("conversion failed: \(error)") }

        guard let samples = output.floatChannelData?[0], output.frameLength > 0 else { return }
        var remaining = Int(output.frameLength) * MemoryLayout<Float32>.size
        var cursor = UnsafeRawPointer(samples)
        while remaining > 0 {
            let written = Darwin.write(STDOUT_FILENO, cursor, remaining)
            if written <= 0 { exit(0) }  // reader went away
            remaining -= written
            cursor += written
        }
    }
}

// MARK: - System audio (Core Audio process tap)

final class SystemCapture {
    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var procID: AudioDeviceIOProcID?
    private var writer: PCMWriter?

    func start() {
        let description = CATapDescription(stereoGlobalTapButExcludeProcesses: [])
        description.uuid = UUID()
        description.name = "Murmur"
        description.isPrivate = true
        description.muteBehavior = .unmuted
        check(AudioHardwareCreateProcessTap(description, &tapID), "create process tap")

        var asbd: AudioStreamBasicDescription = property(tapID, kAudioTapPropertyFormat)
        guard let tapFormat = AVAudioFormat(streamDescription: &asbd) else { fail("unreadable tap format") }
        writer = PCMWriter(from: tapFormat)

        let outputDevice: AudioObjectID = property(
            AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultSystemOutputDevice)
        let outputUID: CFString = property(outputDevice, kAudioDevicePropertyDeviceUID)

        let aggregate: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Murmur Tap",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceMainSubDeviceKey: outputUID as String,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: outputUID as String]],
            kAudioAggregateDeviceTapListKey: [
                [kAudioSubTapDriftCompensationKey: true, kAudioSubTapUIDKey: description.uuid.uuidString]
            ],
        ]
        check(AudioHardwareCreateAggregateDevice(aggregate as CFDictionary, &aggregateID), "create aggregate device")

        let queue = DispatchQueue(label: "audiocap.system", qos: .userInteractive)
        check(
            AudioDeviceCreateIOProcIDWithBlock(&procID, aggregateID, queue) { [weak self] _, inputData, _, _, _ in
                guard let self, let writer = self.writer,
                    let buffer = AVAudioPCMBuffer(pcmFormat: tapFormat, bufferListNoCopy: inputData, deallocator: nil)
                else { return }
                writer.write(buffer)
            }, "create IO proc")
        check(AudioDeviceStart(aggregateID, procID), "start aggregate device")
        log("capturing system audio at \(Int(tapFormat.sampleRate)) Hz")
    }

    func stop() {
        if let procID {
            AudioDeviceStop(aggregateID, procID)
            AudioDeviceDestroyIOProcID(aggregateID, procID)
        }
        if aggregateID != kAudioObjectUnknown { AudioHardwareDestroyAggregateDevice(aggregateID) }
        if tapID != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(tapID) }
    }

    private func property<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> T {
        var address = AudioObjectPropertyAddress(
            mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var size = UInt32(MemoryLayout<T>.size)
        let value = UnsafeMutablePointer<T>.allocate(capacity: 1)
        defer { value.deallocate() }
        check(AudioObjectGetPropertyData(object, &address, 0, nil, &size, value), "read property \(selector)")
        return value.move()
    }
}

// MARK: - Microphone (AVAudioEngine)

final class MicCapture {
    private let engine = AVAudioEngine()

    func start() {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            break
        case .notDetermined:
            log("waiting for microphone permission")
            let answered = DispatchSemaphore(value: 0)
            var granted = false
            AVCaptureDevice.requestAccess(for: .audio) {
                granted = $0
                answered.signal()
            }
            answered.wait()
            if !granted { fail("microphone permission denied", code: 3) }
        default:
            fail("microphone permission denied", code: 3)
        }
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0 else { fail("no microphone input available", code: 3) }
        let writer = PCMWriter(from: format)
        input.installTap(onBus: 0, bufferSize: 4096, format: format) { buffer, _ in writer.write(buffer) }
        do { try engine.start() } catch { fail("start microphone: \(error)") }
        log("capturing microphone at \(Int(format.sampleRate)) Hz")
    }

    func stop() {
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
    }
}

// MARK: - Entry point

let args = CommandLine.arguments
guard let flag = args.firstIndex(of: "--source"), flag + 1 < args.count else {
    fail("usage: audiocap --source mic|system", code: 64)
}

let stop: () -> Void
switch args[flag + 1] {
case "system":
    let capture = SystemCapture()
    capture.start()
    stop = capture.stop
case "mic":
    let capture = MicCapture()
    capture.start()
    stop = capture.stop
default:
    fail("unknown source \(args[flag + 1])", code: 64)
}

func shutdown() -> Never {
    stop()
    exit(0)
}

signal(SIGPIPE, SIG_IGN)
var signalSources: [DispatchSourceSignal] = []
for sig in [SIGINT, SIGTERM] {
    signal(sig, SIG_IGN)
    let source = DispatchSource.makeSignalSource(signal: sig, queue: .main)
    source.setEventHandler { shutdown() }
    source.resume()
    signalSources.append(source)
}

// Exit when the parent closes stdin, so no orphaned capture survives a crash.
Thread.detachNewThread {
    while FileHandle.standardInput.availableData.count > 0 {}
    DispatchQueue.main.async { shutdown() }
}

dispatchMain()
