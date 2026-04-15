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
    Ok(parse_wpctl_inspect(&text))
}

fn parse_wpctl_inspect(text: &str) -> (String, String) {
    let sink_id = text
        .lines()
        .next()
        .and_then(|first_line| {
            first_line
                .split_whitespace()
                .nth(1)
                .map(|id| id.trim_end_matches(',').to_string())
        })
        .unwrap_or_else(|| "0".to_string());

    let sink_name = text
        .lines()
        .find(|l| l.contains("node.nick") || l.contains("node.description"))
        .and_then(|l| {
            let after_eq = l.split('=').nth(1)?;
            Some(after_eq.trim().trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "default sink".to_string());

    (sink_id, sink_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const WPCTL_OUTPUT: &str = "\
id 49, type PipeWire:Interface:Node
    State: \"suspended\"
    Peers: 0
    Properties:
      * object.serial = \"49\"
      * node.name = \"alsa_output.pci-0000_0c_00.4\"
      * node.nick = \"Starship/Matisse HD Audio\"
      * node.description = \"Starship/Matisse HD Audio Controller Analog Stereo\"
      * media.class = \"Audio/Sink\"";

    #[test]
    fn test_parse_wpctl_inspect_extracts_id() {
        let (sink_id, _) = parse_wpctl_inspect(WPCTL_OUTPUT);
        assert_eq!(sink_id, "49");
    }

    #[test]
    fn test_parse_wpctl_inspect_extracts_nick() {
        let (_, sink_name) = parse_wpctl_inspect(WPCTL_OUTPUT);
        assert_eq!(sink_name, "Starship/Matisse HD Audio");
    }

    #[test]
    fn test_parse_wpctl_inspect_empty_input() {
        let (sink_id, sink_name) = parse_wpctl_inspect("");
        assert_eq!(sink_id, "0");
        assert_eq!(sink_name, "default sink");
    }

    #[test]
    fn test_parse_wpctl_inspect_no_nick_falls_to_description() {
        let input = "\
id 50, type PipeWire:Interface:Node
    Properties:
      * node.description = \"USB Audio Device\"";
        let (_, sink_name) = parse_wpctl_inspect(input);
        assert_eq!(sink_name, "USB Audio Device");
    }

    #[test]
    fn test_take_samples_sufficient_data() {
        let buffer = Arc::new(Mutex::new(vec![1.0_f32, 2.0, 3.0, 4.0, 5.0]));
        let capture = AudioCapture {
            buffer,
            sample_rate: 44100,
            device_name: "test".to_string(),
        };
        let samples = capture.take_samples(3);
        assert_eq!(samples, vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn test_take_samples_insufficient_data() {
        let buffer = Arc::new(Mutex::new(vec![1.0_f32, 2.0]));
        let capture = AudioCapture {
            buffer,
            sample_rate: 44100,
            device_name: "test".to_string(),
        };
        let samples = capture.take_samples(10);
        assert_eq!(samples, vec![1.0, 2.0]);
    }

    #[test]
    fn test_take_samples_empty_buffer() {
        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let capture = AudioCapture {
            buffer,
            sample_rate: 44100,
            device_name: "test".to_string(),
        };
        let samples = capture.take_samples(1024);
        assert!(samples.is_empty());
    }

    #[test]
    fn test_take_samples_exact_count() {
        let buffer = Arc::new(Mutex::new(vec![10.0_f32, 20.0, 30.0]));
        let capture = AudioCapture {
            buffer,
            sample_rate: 44100,
            device_name: "test".to_string(),
        };
        let samples = capture.take_samples(3);
        assert_eq!(samples, vec![10.0, 20.0, 30.0]);
    }
}
