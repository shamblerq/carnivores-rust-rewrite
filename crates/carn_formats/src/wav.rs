//! RIFF WAVE sounds (HUNTDAT\SOUNDFX), read down to 16-bit mono PCM.

use crate::reader::{FormatError, Reader, Result};

#[derive(Clone, Debug)]
pub struct Wave {
    pub rate: u32,
    pub samples: Vec<i16>,
}

impl Wave {
    pub fn load(path: &std::path::Path) -> Result<Wave> {
        let data =
            std::fs::read(path).map_err(|e| FormatError(format!("{}: {e}", path.display())))?;
        Self::parse(&data).map_err(|e| FormatError(format!("{}: {e}", path.display())))
    }

    pub fn parse(data: &[u8]) -> Result<Wave> {
        let mut r = Reader::new(data);
        if r.bytes(4)? != b"RIFF" {
            return Err(FormatError("not a RIFF file".into()));
        }
        r.u32()?;
        if r.bytes(4)? != b"WAVE" {
            return Err(FormatError("not a WAVE file".into()));
        }
        let (mut channels, mut rate, mut bits) = (1u16, 22050u32, 16u16);
        let mut samples = None;
        while r.remaining() >= 8 {
            let id = r.bytes(4)?;
            let len = r.u32()? as usize;
            let len = len.min(r.remaining());
            let body = r.bytes(len)?;
            if len & 1 == 1 && r.remaining() > 0 {
                r.skip(1)?;
            }
            match id {
                b"fmt " => {
                    let mut f = Reader::new(body);
                    let _format = f.u16()?;
                    channels = f.u16()?.max(1);
                    rate = f.u32()?;
                    f.u32()?;
                    f.u16()?;
                    bits = f.u16()?;
                }
                b"data" => {
                    let ch = channels as usize;
                    let s: Vec<i16> = match bits {
                        8 => body
                            .chunks_exact(ch)
                            .map(|c| ((c[0] as i16) - 128) << 8)
                            .collect(),
                        _ => body
                            .chunks_exact(2 * ch)
                            .map(|c| i16::from_le_bytes([c[0], c[1]]))
                            .collect(),
                    };
                    samples = Some(s);
                }
                _ => {}
            }
        }
        let samples = samples.ok_or_else(|| FormatError("no data chunk".into()))?;
        Ok(Wave { rate, samples })
    }
}
