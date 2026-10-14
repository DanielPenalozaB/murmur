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

    init?(from format: AVAudioFormat) {
        guard let converter = AVAudioConverter(from: format, to: targetFormat) else { return nil }
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
        if let error {
            // A format change mid-stream; the mic restarts on the next config change.
            log("conversion failed: \(error)")
            return
        }

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
        guard let writer = PCMWriter(from: tapFormat) else { fail("unsupported tap format \(tapFormat)") }
        self.writer = writer

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
    /// Restarts and watchdog checks run here, never on the audio thread.
    private let queue = DispatchQueue(label: "audiocap.mic")
    private var engine = AVAudioEngine()
    private var observer: NSObjectProtocol?
    private var watchdog: DispatchSourceTimer?
    private let lastBuffer = NSLock()
    private var lastBufferAt = DispatchTime.now()
    private var restarts = 0

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
        queue.sync {
            // At launch a missing mic is an error the app shows; later it's retried.
            if !startEngine() { fail("no microphone input available", code: 3) }
        }

        // AVAudioEngine stops silently when the input device changes (headphones
        // connect, Teams switches devices) and sometimes never delivers audio at all.
        // Restart it whenever it goes quiet: even a silent room produces buffers.
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + 2, repeating: 1)
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            self.lastBuffer.lock()
            let idle = DispatchTime.now().uptimeNanoseconds - self.lastBufferAt.uptimeNanoseconds
            self.lastBuffer.unlock()
            if idle > 2_000_000_000 { self.restart("microphone delivered no audio for 2 s") }
        }
        timer.resume()
        watchdog = timer
    }

    func stop() {
        queue.sync {
            watchdog?.cancel()
            stopEngine()
        }
    }

    /// Must run on `queue`. Returns false when no usable input device is available
    /// right now (e.g. mid device switch); the watchdog tries again.
    @discardableResult
    private func startEngine() -> Bool {
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, let writer = PCMWriter(from: format) else {
            log("no usable microphone input (\(format)); will retry")
            return false
        }
        input.installTap(onBus: 0, bufferSize: 4096, format: format) { [weak self] buffer, _ in
            writer.write(buffer)
            guard let self else { return }
            self.lastBuffer.lock()
            self.lastBufferAt = .now()
            self.lastBuffer.unlock()
        }
        observer = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil
        ) { [weak self] _ in
            self?.queue.async { self?.restart("microphone configuration changed") }
        }
        lastBuffer.lock()
        lastBufferAt = .now()
        lastBuffer.unlock()
        do { try engine.start() } catch { log("start microphone: \(error)") }
        log("capturing microphone at \(Int(format.sampleRate)) Hz")
        return true
    }

    /// Must run on `queue`.
    private func stopEngine() {
        if let observer { NotificationCenter.default.removeObserver(observer) }
        observer = nil
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
    }

    /// Must run on `queue`. A fresh engine picks up the current default input device.
    private func restart(_ reason: String) {
        restarts += 1
        log("\(reason); restarting (\(restarts))")
        stopEngine()
        engine = AVAudioEngine()
        startEngine()
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
