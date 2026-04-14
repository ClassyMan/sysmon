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
        let channels = 2_usize;

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
                .expect("failed to spawn pw-record — is pipewire installed?");

            let mut stdout = child.stdout.take().expect("no stdout from pw-record");
            let bytes_per_frame = channels * 4;
            let mut raw_buf = vec![0u8; bytes_per_frame * 1024];

            loop {
                let bytes_read = match stdout.read(&mut raw_buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(_) => break,
                };

                let mut buf = buffer_writer.lock().unwrap();

                let frame_count = bytes_read / bytes_per_frame;
                for frame_idx in 0..frame_count {
                    let mut sum = 0.0_f32;
                    for ch in 0..channels {
                        let offset = (frame_idx * channels + ch) * 4;
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
        });

        Ok(Self {
            buffer,
            sample_rate,
            device_name: sink_name,
        })
    }

    pub fn take_samples(&self, count: usize) -> Vec<f32> {
        let buf = self.buffer.lock().unwrap();
        if buf.len() >= count {
            // Return the last `count` samples WITHOUT clearing.
            // The buffer is a rolling window — the writer thread
            // trims it to 48000 max, so it doesn't grow unbounded.
            buf[buf.len() - count..].to_vec()
        } else {
            buf.clone()
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

    // First line format: "id 49, type PipeWire:Interface:Node"
    let sink_id = text.lines()
        .next()
        .and_then(|first_line| {
            first_line.split_whitespace()
                .nth(1)
                .map(|id| id.trim_end_matches(',').to_string())
        })
        .unwrap_or_else(|| "0".to_string());

    let sink_name = text.lines()
        .find(|l| l.contains("node.nick") || l.contains("node.description"))
        .and_then(|l| {
            let after_eq = l.split('=').nth(1)?;
            Some(after_eq.trim().trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "default sink".to_string());

    Ok((sink_id, sink_name))
}
