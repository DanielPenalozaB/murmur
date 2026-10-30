// murmur-audiocap: captures one audio source and writes 16 kHz mono Float32 PCM
// (little-endian) to stdout for the Rust core.
//
//   audiocap --source system   Core Audio process tap of all system output (macOS 14.4+)
//   audiocap --source mic      default input device via AVAudioEngine
//   audiocap --watch-mic       prints the bundle IDs of processes using audio input,
//                              space-separated, one line each time the set changes
//   audiocap --permissions     prints `mic=<state> system=<state>` and exits; a state is
//                              granted, denied, undetermined or unknown
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

struct CaptureError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

// MARK: - System audio (Core Audio process tap)

final class SystemCapture {
    /// Rebuilds and watchdog checks run here, never on the audio thread.
    private let queue = DispatchQueue(label: "audiocap.system.control")
    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var procID: AudioDeviceIOProcID?
    private var watchdog: DispatchSourceTimer?
    private let lastBuffer = NSLock()
    private var lastBufferAt = DispatchTime.now()
    private var rebuilds = 0
    /// The rate buffers are interpreted at, and frames counted since `windowStart`
    /// (under `lastBuffer`), to check it against the rate audio really arrives at.
    private var rate: Double = 0
    private var windowFrames = 0
    private var windowStart = DispatchTime.now()
    /// A rate measured to differ from what the device reports, used on the next build.
    private var measuredRate: Double?

    func start() {
        queue.sync { build() }

        // The tap hangs off the output device; when the output changes (headphones,
        // Teams switching to a headset) or the device goes away, audio stops coming.
        var address = AudioObjectPropertyAddress(
            mSelector: kAudioHardwarePropertyDefaultOutputDevice, mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain)
        AudioObjectAddPropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &address, queue) {
            [weak self] _, _ in
            self?.measuredRate = nil
            self?.rebuild("output device changed")
        }

        // An aggregate device delivers buffers continuously, silence included, so a
        // gap means capture stalled.
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + 2, repeating: 1)
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            self.lastBuffer.lock()
            let idle = DispatchTime.now().uptimeNanoseconds - self.lastBufferAt.uptimeNanoseconds
            self.lastBuffer.unlock()
            if idle > 2_000_000_000 {
                self.rebuild("system audio delivered nothing for 2 s")
            } else {
                self.checkRate()
            }
        }
        timer.resume()
        watchdog = timer
    }

    func stop() {
        queue.sync {
            watchdog?.cancel()
            teardown()
        }
    }

    /// Must run on `queue`. Bluetooth headsets drop to 16 kHz while their mic is in
    /// use, and the tap may keep reporting 48 kHz; read at the wrong rate the audio
    /// comes out three times too fast. Counts what arrives and rebuilds at the real rate.
    private func checkRate() {
        lastBuffer.lock()
        let seconds = Double(DispatchTime.now().uptimeNanoseconds - windowStart.uptimeNanoseconds) / 1e9
        let frames = windowFrames
        if seconds >= 1.5 {
            windowFrames = 0
            windowStart = .now()
        }
        lastBuffer.unlock()
        guard seconds >= 1.5, rate > 0 else { return }
        let measured = Double(frames) / seconds
        guard abs(measured / rate - 1) > 0.15 else { return }
        let common: [Double] = [8_000, 11_025, 16_000, 22_050, 24_000, 32_000, 44_100, 48_000, 88_200, 96_000]
        let snapped = common.min { abs($0 - measured) < abs($1 - measured) }!
        guard snapped != rate else { return }
        measuredRate = snapped
        rebuild("system audio arrives at \(Int(measured)) Hz, not \(Int(rate)) Hz")
    }

    /// Must run on `queue`.
    private func rebuild(_ reason: String) {
        rebuilds += 1
        log("\(reason); restarting (\(rebuilds))")
        teardown()
        build()
    }

    /// Must run on `queue`. Failures at launch are fatal (the app shows them); later
    /// ones are logged and retried by the watchdog.
    private func build() {
        let first = rebuilds == 0
        func attempt(_ status: OSStatus, _ what: String) -> Bool {
            if status == noErr { return true }
            if first { fail("\(what) failed (OSStatus \(status))") }
            log("\(what) failed (OSStatus \(status)); will retry")
            return false
        }
        lastBuffer.lock()
        lastBufferAt = .now()
        lastBuffer.unlock()

        let description = CATapDescription(stereoGlobalTapButExcludeProcesses: [])
        description.uuid = UUID()
        description.name = "Murmur"
        description.isPrivate = true
        description.muteBehavior = .unmuted
        guard attempt(AudioHardwareCreateProcessTap(description, &tapID), "create process tap") else { return }

        var asbd: AudioStreamBasicDescription
        var outputUID: CFString
        do {
            asbd = try property(tapID, kAudioTapPropertyFormat)
            // The device apps play to, not the one for alert sounds.
            let outputDevice: AudioObjectID = try property(
                AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultOutputDevice)
            outputUID = try property(outputDevice, kAudioDevicePropertyDeviceUID)
            // For diagnosing devices without changing the user's settings.
            if let uid = ProcessInfo.processInfo.environment["AUDIOCAP_OUTPUT_UID"] { outputUID = uid as CFString }
        } catch {
            _ = attempt(-1, "\(error)")
            return
        }
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
        guard
            attempt(
                AudioHardwareCreateAggregateDevice(aggregate as CFDictionary, &aggregateID), "create aggregate device")
        else { return }

        // The tap's own format can disagree with the rate the device actually runs at.
        let reported = asbd.mSampleRate
        let nominal: Float64 = (try? property(aggregateID, kAudioDevicePropertyNominalSampleRate)) ?? 0
        asbd.mSampleRate = measuredRate ?? (nominal > 0 ? nominal : reported)
        guard let tapFormat = AVAudioFormat(streamDescription: &asbd), let writer = PCMWriter(from: tapFormat)
        else {
            _ = attempt(-1, "unsupported tap format")
            return
        }
        lastBuffer.lock()
        rate = asbd.mSampleRate
        windowFrames = 0
        windowStart = .now()
        lastBuffer.unlock()

        let audio = DispatchQueue(label: "audiocap.system", qos: .userInteractive)
        let created = AudioDeviceCreateIOProcIDWithBlock(&procID, aggregateID, audio) {
            [weak self] _, inputData, _, _, _ in
            guard let buffer = AVAudioPCMBuffer(pcmFormat: tapFormat, bufferListNoCopy: inputData, deallocator: nil)
            else { return }
            writer.write(buffer)
            guard let self else { return }
            self.lastBuffer.lock()
            self.lastBufferAt = .now()
            self.windowFrames += Int(buffer.frameLength)
            self.lastBuffer.unlock()
        }
        guard attempt(created, "create IO proc"), attempt(AudioDeviceStart(aggregateID, procID), "start aggregate device")
        else { return }
        log(
            "capturing system audio at \(Int(tapFormat.sampleRate)) Hz from \(outputUID)"
                + " (tap reports \(Int(reported)) Hz, device \(Int(nominal)) Hz)")
    }

    /// Must run on `queue`.
    private func teardown() {
        if let procID {
            AudioDeviceStop(aggregateID, procID)
            AudioDeviceDestroyIOProcID(aggregateID, procID)
        }
        procID = nil
        if aggregateID != kAudioObjectUnknown { AudioHardwareDestroyAggregateDevice(aggregateID) }
        aggregateID = AudioObjectID(kAudioObjectUnknown)
        if tapID != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(tapID) }
        tapID = AudioObjectID(kAudioObjectUnknown)
    }

    private func property<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) throws -> T {
        var address = AudioObjectPropertyAddress(
            mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var size = UInt32(MemoryLayout<T>.size)
        let value = UnsafeMutablePointer<T>.allocate(capacity: 1)
        defer { value.deallocate() }
        let status = AudioObjectGetPropertyData(object, &address, 0, nil, &size, value)
        if status != noErr { throw CaptureError("read property \(selector) failed (OSStatus \(status))") }
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
        // Named in the log, so a silent recording can be traced to its device.
        let device = AVCaptureDevice.default(for: .audio)?.localizedName ?? "unknown device"
        log("capturing microphone at \(Int(format.sampleRate)) Hz from \(device)")
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

// MARK: - Microphone users (meeting detection)

/// Polls Core Audio's process objects for the ones running audio input. Reading them
/// needs no microphone permission; a poll is cheap and catches processes that come and
/// go without wiring a listener to each one.
final class MicWatcher {
    private var timer: DispatchSourceTimer?
    private var last: String?

    func start() {
        let timer = DispatchSource.makeTimerSource(queue: .main)
        timer.schedule(deadline: .now(), repeating: 1)
        timer.setEventHandler { [weak self] in self?.poll() }
        timer.resume()
        self.timer = timer
    }

    func stop() {
        timer?.cancel()
    }

    private func poll() {
        let line = Set(processes().filter(isRunningInput).compactMap(bundleID)).sorted()
            .joined(separator: " ")
        guard line != last else { return }
        last = line
        let data = (line + "\n").data(using: .utf8)!
        // A closed pipe means Murmur is gone.
        if data.withUnsafeBytes({ Darwin.write(STDOUT_FILENO, $0.baseAddress, $0.count) }) <= 0 { exit(0) }
    }

    private func address(_ selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress(
            mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain)
    }

    private func processes() -> [AudioObjectID] {
        let system = AudioObjectID(kAudioObjectSystemObject)
        var address = address(kAudioHardwarePropertyProcessObjectList)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(system, &address, 0, nil, &size) == noErr else { return [] }
        var ids = [AudioObjectID](repeating: 0, count: Int(size) / MemoryLayout<AudioObjectID>.size)
        guard AudioObjectGetPropertyData(system, &address, 0, nil, &size, &ids) == noErr else { return [] }
        return Array(ids.prefix(Int(size) / MemoryLayout<AudioObjectID>.size))
    }

    private func isRunningInput(_ process: AudioObjectID) -> Bool {
        var address = address(kAudioProcessPropertyIsRunningInput)
        var running: UInt32 = 0
        var size = UInt32(MemoryLayout<UInt32>.size)
        return AudioObjectGetPropertyData(process, &address, 0, nil, &size, &running) == noErr && running != 0
    }

    private func bundleID(_ process: AudioObjectID) -> String? {
        var address = address(kAudioProcessPropertyBundleID)
        var value: Unmanaged<CFString>?
        var size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        guard AudioObjectGetPropertyData(process, &address, 0, nil, &size, &value) == noErr,
            let id = value?.takeRetainedValue() as String?, !id.isEmpty
        else { return nil }
        return id
    }
}

// MARK: - Permissions

/// Whether macOS lets Murmur capture each source. macOS attributes the helper's access
/// to the app that launched it, so this answers for Murmur (or the terminal in dev).
func permissions() -> String {
    let mic: String
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized: mic = "granted"
    case .notDetermined: mic = "undetermined"
    default: mic = "denied"
    }
    return "mic=\(mic) system=\(systemAudioPermission())"
}

/// System audio recording has no public status API. TCC's own preflight answers it,
/// looked up at runtime so a macOS without it reports "unknown" instead of crashing.
func systemAudioPermission() -> String {
    typealias Preflight = @convention(c) (CFString, CFDictionary?) -> Int
    guard let tcc = dlopen("/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC", RTLD_NOW),
        let symbol = dlsym(tcc, "TCCAccessPreflight")
    else { return "unknown" }
    let preflight = unsafeBitCast(symbol, to: Preflight.self)
    switch preflight("kTCCServiceAudioCapture" as CFString, nil) {
    case 0: return "granted"
    case 1: return "denied"
    case 2: return "undetermined"
    default: return "unknown"
    }
}

// MARK: - Entry point

let args = CommandLine.arguments
let stop: () -> Void
if args.contains("--permissions") {
    print(permissions())
    exit(0)
} else if args.contains("--watch-mic") {
    let watcher = MicWatcher()
    watcher.start()
    stop = watcher.stop
} else {
    guard let flag = args.firstIndex(of: "--source"), flag + 1 < args.count else {
        fail("usage: audiocap --source mic|system | --watch-mic | --permissions", code: 64)
    }
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
