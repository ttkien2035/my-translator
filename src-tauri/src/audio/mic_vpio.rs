//! macOS microphone capture through Apple's Voice-Processing I/O audio unit:
//! the system's own echo cancellation, noise suppression and automatic gain
//! control — tuned per Mac model and running inside CoreAudio, so it costs
//! this process almost nothing. Delivers mono f32 at `VPIO_RATE` into the
//! same DSP thread as the cpal path (`mic_pipeline`), so the rest of the
//! chain (resample, high-pass, optional GTCRN/VAD) still applies.
//!
//! Lifecycle: the returned `AudioUnit` owns the capture; coreaudio-rs's
//! `Drop` stops, uninitializes and disposes it, which also drops the
//! callback and with it the raw-frame sender.

use std::sync::mpsc;

use coreaudio::audio_unit::audio_format::LinearPcmFlags;
use coreaudio::audio_unit::render_callback::{self, data};
use coreaudio::audio_unit::{AudioUnit, Element, IOType, SampleFormat, Scope, StreamFormat};
use coreaudio::sys::{kAudioOutputUnitProperty_EnableIO, kAudioUnitProperty_StreamFormat};

/// Client-side format requested from the unit; VPIO converts from the device
/// rate internally.
pub const VPIO_RATE: u32 = 48_000;

// AudioUnitProperties.h — Voice-Processing I/O unit properties. Numeric values
// are Apple's stable ABI; spelled out so we don't depend on bindgen exposing
// the names.
const K_AU_VOICE_IO_BYPASS_VOICE_PROCESSING: u32 = 2100;
const K_AU_VOICE_IO_VOICE_PROCESSING_ENABLE_AGC: u32 = 2101;

type Args = render_callback::Args<data::NonInterleaved<f32>>;

fn ctx(what: &'static str) -> impl Fn(coreaudio::Error) -> String {
    move |err| format!("VoiceProcessingIO {what}: {err:?}")
}

/// Build, configure and start a capture-only VPIO unit.
pub fn start(raw_tx: mpsc::SyncSender<Vec<f32>>, agc: bool) -> Result<AudioUnit, String> {
    let mut unit = AudioUnit::new(IOType::VoiceProcessingIO).map_err(ctx("create"))?;
    // coreaudio-rs initializes on construction; I/O enable flags and the
    // client format can only be changed on an uninitialized unit.
    unit.uninitialize().map_err(ctx("uninitialize"))?;

    // Bus 1 = input from hardware: on. Bus 0 = output to hardware: off — we
    // only capture; voice processing still runs on the input path.
    unit.set_property(
        kAudioOutputUnitProperty_EnableIO,
        Scope::Input,
        Element::Input,
        Some(&1u32),
    )
    .map_err(ctx("enable input"))?;
    unit.set_property(
        kAudioOutputUnitProperty_EnableIO,
        Scope::Output,
        Element::Output,
        Some(&0u32),
    )
    .map_err(ctx("disable output"))?;

    // Mono f32 non-interleaved: the layout coreaudio-rs's input callback
    // supports. Set on the input bus's output scope (what we read).
    let format = StreamFormat {
        sample_rate: VPIO_RATE as f64,
        sample_format: SampleFormat::F32,
        flags: LinearPcmFlags::IS_FLOAT
            | LinearPcmFlags::IS_PACKED
            | LinearPcmFlags::IS_NON_INTERLEAVED,
        channels: 1,
    };
    unit.set_property(
        kAudioUnitProperty_StreamFormat,
        Scope::Output,
        Element::Input,
        Some(&format.to_asbd()),
    )
    .map_err(ctx("set stream format"))?;

    // Voice processing on (not bypassed); AGC per user setting. Neither is
    // fatal — the unit still captures if a property is rejected.
    if let Err(e) = unit.set_property(
        K_AU_VOICE_IO_BYPASS_VOICE_PROCESSING,
        Scope::Global,
        Element::Output,
        Some(&0u32),
    ) {
        eprintln!("[Mic] VPIO: could not clear bypass flag: {e:?}");
    }
    if let Err(e) = unit.set_property(
        K_AU_VOICE_IO_VOICE_PROCESSING_ENABLE_AGC,
        Scope::Global,
        Element::Output,
        Some(&(agc as u32)),
    ) {
        eprintln!("[Mic] VPIO: could not set AGC={agc}: {e:?}");
    }

    unit.initialize().map_err(ctx("initialize"))?;

    // Real-time callback: copy the mono channel out and hand it to the DSP
    // thread without blocking.
    unit.set_input_callback(move |args: Args| {
        let Args {
            num_frames,
            mut data,
            ..
        } = args;
        let mut frames = Vec::with_capacity(num_frames);
        if let Some(ch) = data.channels_mut().next() {
            frames.extend_from_slice(&ch[..num_frames.min(ch.len())]);
        }
        super::microphone::forward(&raw_tx, frames);
        Ok(())
    })
    .map_err(ctx("set input callback"))?;

    unit.start().map_err(ctx("start"))?;
    Ok(unit)
}
