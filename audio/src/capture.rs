use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;

pub struct AudioCapture {
    pub buffer: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: u32,
    pub device_name: String,
}

impl AudioCapture {
    pub fn start_monitor() -> Result<Self> {
        let sample_rate = 44100_u32;
        let channels = 2_u16;

        let (sink_id, sink_name) = find_default_sink()?;

        let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
        let buffer_writer = buffer.clone();

        thread::spawn(move || {
            let mut child = Command::new("pw-record")
                .args([
                    "--target", &sink_id,
                    "--rate", &sample_rate.to_string(),
                    "--channels", &channels.to_string(),
                    "--format", "f32",
                    "-",
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("failed to spawn pw-record");

            let stdout = child.stdout.take().expect("no stdout from pw-record");
            read_pcm_stream(stdout, buffer_writer, channels as usize);
        });

        Ok(Self {
            buffer,
            sample_rate,
            device_name: sink_name,
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

fn find_default_sink() -> Result<(String, String)> {
    let output = Command::new("wpctl")
        .args(["inspect", "@DEFAULT_AUDIO_SINK@"])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("wpctl inspect failed — is WirePlumber running?");
    }

    let text = String::from_utf8_lossy(&output.stdout);

    let sink_id = text.lines()
        .find(|l| l.contains("id:") || l.contains("object.id"))
        .and_then(|l| {
            l.split_whitespace()
                .find(|w| w.chars().all(|c| c.is_ascii_digit()))
        })
        .map(|s| s.to_string())
        .unwrap_or_else(|| "@DEFAULT_AUDIO_SINK@".to_string());

    let sink_name = text.lines()
        .find(|l| l.contains("node.nick") || l.contains("node.description"))
        .and_then(|l| l.split('=').nth(1))
        .map(|s| s.trim().trim_matches('"').to_string())
        .unwrap_or_else(|| "default sink".to_string());

    Ok((sink_id, sink_name))
}

fn read_pcm_stream(
    mut reader: impl Read + Send + 'static,
    buffer: Arc<Mutex<Vec<f32>>>,
    channels: usize,
) {
    let mut raw_buf = [0u8; 4096];

    loop {
        let bytes_read = match reader.read(&mut raw_buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };

        let float_count = bytes_read / 4;
        let mut buf = buffer.lock().unwrap();

        for chunk_idx in 0..(float_count / channels) {
            let base = chunk_idx * channels;
            let mut sum = 0.0_f32;
            for ch in 0..channels {
                let offset = (base + ch) * 4;
                if offset + 4 <= bytes_read {
                    let sample = f32::from_le_bytes([
                        raw_buf[offset],
                        raw_buf[offset + 1],
                        raw_buf[offset + 2],
                        raw_buf[offset + 3],
                    ]);
                    sum += sample;
                }
            }
            buf.push(sum / channels as f32);
        }

        if buf.len() > 48000 {
            let drain_to = buf.len() - 48000;
            buf.drain(0..drain_to);
        }
    }
}
