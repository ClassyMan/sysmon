use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;
use pipewire as pw;
use pw::properties::properties;
use pw::spa;
use spa::param::format::{MediaSubtype, MediaType};
use spa::param::format_utils;
use spa::pod::Pod;

pub struct AudioCapture {
    pub buffer: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: u32,
    device_name: Arc<Mutex<String>>,
    error: Arc<Mutex<Option<String>>>,
}

impl AudioCapture {
    pub fn start_monitor() -> Result<Self> {
        let sample_rate = 44100_u32;
        let channels = 2_usize;

        let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let device_name = Arc::new(Mutex::new("default sink".to_string()));
        let error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

        let buffer_writer = buffer.clone();
        let name_writer = device_name.clone();
        let error_writer = error.clone();

        thread::spawn(move || {
            if let Err(e) = run_capture_loop(buffer_writer, name_writer, sample_rate, channels) {
                let chain: Vec<String> = e.chain().map(|c| c.to_string()).collect();
                let message = chain.join(" -> ");
                if let Ok(mut slot) = error_writer.lock() {
                    *slot = Some(message);
                }
            }
        });

        Ok(Self {
            buffer,
            sample_rate,
            device_name,
            error,
        })
    }

    pub fn device_name(&self) -> String {
        self.device_name.lock().unwrap().clone()
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }

    pub fn buffer_len(&self) -> usize {
        self.buffer.lock().unwrap().len()
    }

    pub fn peak_amplitude(&self) -> f32 {
        let buf = self.buffer.lock().unwrap();
        let look = buf.len().min(4096);
        buf[buf.len() - look..]
            .iter()
            .copied()
            .map(f32::abs)
            .fold(0.0_f32, f32::max)
    }

    pub fn take_samples(&self, count: usize) -> Vec<f32> {
        let buf = self.buffer.lock().unwrap();
        if buf.len() >= count {
            buf[buf.len() - count..].to_vec()
        } else {
            buf.clone()
        }
    }
}

struct CaptureState {
    buffer: Arc<Mutex<Vec<f32>>>,
    device_name: Arc<Mutex<String>>,
    channels: usize,
    format: spa::param::audio::AudioInfoRaw,
}

fn run_capture_loop(
    buffer: Arc<Mutex<Vec<f32>>>,
    device_name: Arc<Mutex<String>>,
    sample_rate: u32,
    channels: usize,
) -> Result<()> {
    pw::init();

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;

    let state = CaptureState {
        buffer,
        device_name,
        channels,
        format: Default::default(),
    };

    let props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::STREAM_CAPTURE_SINK => "true",
        *pw::keys::NODE_NAME => "sysmon-audio",
        *pw::keys::NODE_DESCRIPTION => "sysmon audio spectrum",
    };

    let stream = pw::stream::StreamBox::new(&core, "sysmon-audio", props)?;

    let _listener = stream
        .add_local_listener_with_user_data(state)
        .param_changed(|_, state, id, param| {
            let Some(param) = param else { return };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }
            let (media_type, media_subtype) = match format_utils::parse_format(param) {
                Ok(v) => v,
                Err(_) => return,
            };
            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }
            let _ = state.format.parse(param);
        })
        .state_changed(|_, state, _old, new| {
            let label = match new {
                pw::stream::StreamState::Unconnected => "unconnected",
                pw::stream::StreamState::Connecting => "connecting",
                pw::stream::StreamState::Paused => "paused",
                pw::stream::StreamState::Streaming => "streaming",
                pw::stream::StreamState::Error(_) => "error",
            };
            if let Ok(mut name) = state.device_name.lock() {
                *name = label.to_string();
            }
        })
        .process(|stream, state| {
            let Some(mut pw_buf) = stream.dequeue_buffer() else { return };
            let datas = pw_buf.datas_mut();
            if datas.is_empty() {
                return;
            }
            let data = &mut datas[0];
            let chunk_size = data.chunk().size() as usize;
            let chunk_offset = data.chunk().offset() as usize;
            let Some(full_bytes) = data.data() else { return };
            let end = (chunk_offset + chunk_size).min(full_bytes.len());
            if chunk_offset >= end {
                return;
            }
            let samples_bytes = &full_bytes[chunk_offset..end];

            let bytes_per_frame = state.channels * 4;
            if bytes_per_frame == 0 {
                return;
            }
            let frame_count = samples_bytes.len() / bytes_per_frame;

            let mut buf = state.buffer.lock().unwrap();
            for frame_idx in 0..frame_count {
                let mut sum = 0.0_f32;
                for ch in 0..state.channels {
                    let offset = (frame_idx * state.channels + ch) * 4;
                    let sample = f32::from_le_bytes([
                        samples_bytes[offset],
                        samples_bytes[offset + 1],
                        samples_bytes[offset + 2],
                        samples_bytes[offset + 3],
                    ]);
                    sum += sample;
                }
                buf.push(sum / state.channels as f32);
            }
            if buf.len() > 48000 {
                let drain_to = buf.len() - 48000;
                buf.drain(0..drain_to);
            }
        })
        .register()?;

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(sample_rate);
    audio_info.set_channels(channels as u32);

    let obj = pw::spa::pod::Object {
        type_: pw::spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: pw::spa::param::ParamType::EnumFormat.as_raw(),
        properties: audio_info.into(),
    };
    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .map_err(|e| anyhow::anyhow!("serialize pod: {e}"))?
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values)
        .ok_or_else(|| anyhow::anyhow!("pod from bytes failed"))?];

    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    mainloop.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(buffer: Vec<f32>, name: &str) -> AudioCapture {
        AudioCapture {
            buffer: Arc::new(Mutex::new(buffer)),
            sample_rate: 44100,
            device_name: Arc::new(Mutex::new(name.to_string())),
            error: Arc::new(Mutex::new(None)),
        }
    }

    #[test]
    fn test_take_samples_sufficient_data() {
        let capture = fixture(vec![1.0_f32, 2.0, 3.0, 4.0, 5.0], "test");
        assert_eq!(capture.take_samples(3), vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn test_take_samples_insufficient_data() {
        let capture = fixture(vec![1.0_f32, 2.0], "test");
        assert_eq!(capture.take_samples(10), vec![1.0, 2.0]);
    }

    #[test]
    fn test_take_samples_empty_buffer() {
        let capture = fixture(Vec::<f32>::new(), "test");
        assert!(capture.take_samples(1024).is_empty());
    }

    #[test]
    fn test_take_samples_exact_count() {
        let capture = fixture(vec![10.0_f32, 20.0, 30.0], "test");
        assert_eq!(capture.take_samples(3), vec![10.0, 20.0, 30.0]);
    }

    #[test]
    fn test_device_name_getter() {
        let capture = fixture(Vec::new(), "Starship/Matisse");
        assert_eq!(capture.device_name(), "Starship/Matisse");
    }
}
