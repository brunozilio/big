use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use domain::mcp::text_result;
use serde_json::{Value, json};

use crate::files;
use crate::session::Session;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub delay_ms: u32,
}

pub fn export(session: &Session, device: &str, input: &Value) -> Value {
    let options = input.get("options").cloned().unwrap_or_else(|| json!({}));
    let render = json!({ "action": "render", "tabId": input["tabId"], "options": options });
    let rendered = match session.ask(device, "gif_creator", &render, true) {
        Ok(rendered) if rendered["isError"] == true => return rendered,
        Ok(rendered) => rendered,
        Err(message) => return text_result(message, true),
    };
    let frames = match frames(&rendered) {
        Ok(frames) if frames.is_empty() => {
            return text_result(
                "No frames were recorded in this tab group. Start with gif_creator start_recording, take a screenshot, act, take a final screenshot, then export.",
                true,
            );
        }
        Ok(frames) => frames,
        Err(message) => return text_result(message, true),
    };
    let quality = options["quality"].as_f64().unwrap_or(10.0).clamp(1.0, 30.0) as i32;
    let bytes = match encode(&frames, quality) {
        Ok(bytes) => bytes,
        Err(message) => return text_result(message, true),
    };
    let filename = filename(input["filename"].as_str());
    let mut lines = vec![format!(
        "Exported GIF with {} frame{} ({}x{}, {} KB).",
        frames.len(),
        if frames.len() == 1 { "" } else { "s" },
        frames[0].width,
        frames[0].height,
        bytes.len().div_ceil(1024)
    )];
    match files::keep(&session.settings().files, &filename, &bytes) {
        Ok(path) => lines.push(format!("Saved to: {}", path.display())),
        Err(message) => lines.push(message),
    }
    let delivered = deliver(session, device, input, &filename, &bytes);
    let is_error = delivered.as_ref().is_err();
    lines.push(delivered.unwrap_or_else(|message| message));
    text_result(lines.join("\n"), is_error)
}

fn deliver(session: &Session, device: &str, input: &Value, filename: &str, bytes: &[u8]) -> Result<String, String> {
    let download = input["download"] == true;
    let coordinate = input["coordinate"].as_array().filter(|point| point.len() == 2).cloned();
    if !download && coordinate.is_none() {
        return Ok(
            "Not delivered in the browser: pass download: true to download it, or coordinate to drop it on the page."
                .to_owned(),
        );
    }
    let request = json!({
        "action": "deliver",
        "tabId": input["tabId"],
        "filename": filename,
        "data": STANDARD.encode(bytes),
        "download": download,
        "coordinate": coordinate,
    });
    let answer = session.ask(device, "gif_creator", &request, true)?;
    let text = answer["content"][0]["text"].as_str().unwrap_or_default().to_owned();
    if answer["isError"] == true { Err(text) } else { Ok(text) }
}

fn filename(asked: Option<&str>) -> String {
    let cleaned: String = asked
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '-' })
        .collect();
    let cleaned = cleaned.trim_matches(['.', '-']).to_owned();
    if cleaned.is_empty() {
        return format!("recording-{}.gif", files::stamp());
    }
    if cleaned.to_ascii_lowercase().ends_with(".gif") { cleaned } else { format!("{cleaned}.gif") }
}

fn frames(rendered: &Value) -> Result<Vec<Frame>, String> {
    let listed = rendered["frames"].as_array().cloned().unwrap_or_default();
    listed
        .iter()
        .map(|frame| {
            let data = frame["png"].as_str().unwrap_or_default();
            let bytes =
                STANDARD.decode(data).map_err(|error| format!("A recorded frame is not valid base64: {error}"))?;
            let (rgba, width, height) = decode_png(&bytes)?;
            let delay_ms = frame["delay"].as_u64().unwrap_or(800).clamp(20, 10_000) as u32;
            Ok(Frame { rgba, width, height, delay_ms })
        })
        .collect()
}

pub fn decode_png(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|error| format!("A recorded frame is not a PNG: {error}"))?;
    let size = reader.output_buffer_size().ok_or("A recorded frame is too large.")?;
    let mut buffer = vec![0; size];
    let info =
        reader.next_frame(&mut buffer).map_err(|error| format!("A recorded frame could not be read: {error}"))?;
    buffer.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer.as_chunks::<3>().0.iter().flat_map(|[red, green, blue]| [*red, *green, *blue, 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            buffer.as_chunks::<2>().0.iter().flat_map(|[gray, alpha]| [*gray, *gray, *gray, *alpha]).collect()
        }
        png::ColorType::Grayscale => buffer.iter().flat_map(|gray| [*gray, *gray, *gray, 255]).collect(),
        png::ColorType::Indexed => return Err("A recorded frame uses an unsupported PNG palette.".to_owned()),
    };
    Ok((rgba, info.width, info.height))
}

pub fn encode(frames: &[Frame], quality: i32) -> Result<Vec<u8>, String> {
    let first = frames.first().ok_or("There are no frames to encode.")?;
    let (width, height) = (first.width, first.height);
    let wide = u16::try_from(width).map_err(|_| "The frames are too wide for a GIF.")?;
    let tall = u16::try_from(height).map_err(|_| "The frames are too tall for a GIF.")?;
    let mut out = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut out, wide, tall, &[]).map_err(|error| error.to_string())?;
        encoder.set_repeat(gif::Repeat::Infinite).map_err(|error| error.to_string())?;
        for frame in frames.iter().filter(|frame| frame.width == width && frame.height == height) {
            let mut rgba = frame.rgba.clone();
            let mut encoded = gif::Frame::from_rgba_speed(wide, tall, &mut rgba, quality.clamp(1, 30));
            encoded.delay = u16::try_from(frame.delay_ms / 10).unwrap_or(u16::MAX).max(2);
            encoder.write_frame(&encoded).map_err(|error| error.to_string())?;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_of(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let pixels: Vec<u8> = (0..width * height).flat_map(|_| color).collect();
            if let Ok(mut writer) = encoder.write_header() {
                let _ = writer.write_image_data(&pixels);
            }
        }
        out
    }

    #[test]
    fn rendered_frames_become_an_animated_gif_that_loops() {
        let rendered = json!({ "frames": [
            { "png": STANDARD.encode(png_of(8, 6, [255, 0, 0, 255])), "delay": 500 },
            { "png": STANDARD.encode(png_of(8, 6, [0, 0, 255, 255])), "delay": 1200 }
        ]});
        let frames = frames(&rendered).unwrap_or_default();
        assert_eq!(frames.len(), 2);
        assert_eq!((frames[0].width, frames[0].height, frames[0].delay_ms), (8, 6, 500));
        assert_eq!(&frames[0].rgba[..4], &[255, 0, 0, 255]);
        let bytes = encode(&frames, 10).unwrap_or_default();
        assert!(bytes.starts_with(b"GIF89a"));
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(std::io::Cursor::new(bytes)).unwrap_or_else(|_| unreachable!());
        let mut delays = Vec::new();
        while let Ok(Some(frame)) = decoder.read_next_frame() {
            delays.push(frame.delay);
        }
        assert_eq!(delays, [50, 120]);
    }

    #[test]
    fn bad_frames_and_no_frames_are_reported() {
        assert!(frames(&json!({ "frames": [{ "png": "!!" }] })).is_err());
        assert!(frames(&json!({ "frames": [{ "png": STANDARD.encode(b"nope") }] })).is_err());
        assert!(encode(&[], 10).is_err());
    }

    #[test]
    fn filenames_are_cleaned_and_end_in_gif() {
        assert_eq!(filename(Some("login flow")), "login-flow.gif");
        assert_eq!(filename(Some("demo.GIF")), "demo.GIF");
        assert_eq!(filename(Some("../../etc/passwd")), "etc-passwd.gif");
        assert!(filename(None).starts_with("recording-"));
    }
}
