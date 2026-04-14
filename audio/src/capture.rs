use std::sync::{Arc, Mutex};

use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;

pub struct AudioCapture {
    pub buffer: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: u32,
    _stream: Stream,
}

impl AudioCapture {
    pub fn start_monitor() -> Result<Self> {
        let host = cpal::default_host();

        // Try to find a monitor/loopback source (captures system output)
        let device = find_monitor_device(&host)
            .or_else(|| host.default_input_device())
            .ok_or_else(|| anyhow::anyhow!("no audio input device found"))?;

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let buffer_writer = buffer.clone();

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config.into(), buffer_writer, channels),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config.into(), buffer_writer, channels),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config.into(), buffer_writer, channels),
            format => anyhow::bail!("unsupported sample format: {:?}", format),
        }?;

        stream.play()?;

        Ok(Self {
            buffer,
            sample_rate,
            _stream: stream,
        })
    }

    pub fn take_samples(&self, count: usize) -> Vec<f32> {
        let mut buf = self.buffer.lock().unwrap();
        if buf.len() >= count {
            let start = buf.len() - count;
            let samples = buf[start..].to_vec();
            buf.clear();
            samples
        } else {
            let samples = buf.clone();
            buf.clear();
            samples
        }
    }
}

fn find_monitor_device(host: &cpal::Host) -> Option<cpal::Device> {
    let devices = host.input_devices().ok()?;
    for device in devices {
        let name = device.name().unwrap_or_default().to_lowercase();
        if name.contains("monitor") || name.contains("loopback") {
            return Some(device);
        }
    }
    None
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    channels: usize,
) -> Result<Stream>
where
    T: cpal::Sample + cpal::SizedSample + Into<f32>,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut buf = buffer.lock().unwrap();
            // Mix to mono
            for chunk in data.chunks(channels) {
                let sum: f32 = chunk.iter().map(|s| {
                    let val: f32 = (*s).into();
                    val
                }).sum();
                buf.push(sum / channels as f32);
            }
            // Keep buffer reasonable (max ~1 second at 48kHz)
            if buf.len() > 48000 {
                let drain_to = buf.len() - 48000;
                buf.drain(0..drain_to);
            }
        },
        |err| eprintln!("audio stream error: {}", err),
        None,
    )?;

    Ok(stream)
}
