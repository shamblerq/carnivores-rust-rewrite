//! Models: the meshes inside an area's objects, characters (.CAR) and the
//! loose .3DF files (binoculars, compass, sun).
//!
//! A model is a list of vertices and triangles with texture coordinates into
//! one 256-pixel-wide 16-bit texture. On disk the vertices are at half size
//! with z pointing the other way; every loader doubled them and flipped z,
//! and so does this one.

use crate::reader::{Reader, Result};

/// Drawn from both sides (not back-face culled, and left out of lighting).
pub const SF_DOUBLE_SIDE: u16 = 1;
pub const SF_DARK_BACK: u16 = 2;
/// Cut out: texel 0 is transparent.
pub const SF_OPACITY: u16 = 4;
pub const SF_TRANSPARENT: u16 = 8;
/// Hitting this face is a kill (a creature's head or heart).
pub const SF_MORTAL: u16 = 0x10;
pub const SF_PHONG: u16 = 0x30;
pub const SF_ENVMAP: u16 = 0x50;
pub const SF_DARK: u16 = 0x8000;

pub const MAX_VERTICES: usize = 4096;
pub const MAX_FACES: usize = 8192;

#[derive(Clone, Debug)]
pub struct Face {
    pub v: [u32; 3],
    /// Texture coordinates, 0..1 across the 256x256 texture.
    pub uv: [[f32; 2]; 3],
    pub flags: u16,
    pub dmask: u16,
    pub distant: i32,
    pub next: i32,
    pub group: i32,
}

impl Face {
    /// Drawn with the colour key: cut-out or see-through faces.
    pub fn keyed(&self) -> bool {
        self.flags & (SF_OPACITY | SF_TRANSPARENT) != 0
    }

    /// Seen from the front only. Every loader marked each face that is not
    /// double-sided, and every renderer then left such a face out when it
    /// turned away from the eye.
    pub fn one_sided(&self) -> bool {
        self.flags & SF_DOUBLE_SIDE == 0
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Vertex {
    pub pos: [f32; 3],
    /// The bone (object) the vertex belongs to, for the hit tests.
    pub owner: i16,
    pub hide: i16,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub vertices: Vec<Vertex>,
    pub faces: Vec<Face>,
    /// 256 pixels wide, `texture.len() / 256` rows, 5-5-5.
    pub texture: Vec<u16>,
}

impl Model {
    pub fn texture_height(&self) -> usize {
        self.texture.len() / 256
    }

    /// Whether any face is cut out, which is what makes texel 0 transparent
    /// everywhere in the texture.
    pub fn has_cutouts(&self) -> bool {
        self.faces.iter().any(|f| f.flags & SF_OPACITY != 0)
    }

    /// A vertex as it was in the file, before the loader's scaling: the
    /// frame the original lighting was worked out in.
    pub fn raw_pos(&self, v: usize) -> [f32; 3] {
        let p = self.vertices[v].pos;
        [p[0] * 0.5, p[1] * 0.5, -p[2] * 0.5]
    }

    fn read_faces(r: &mut Reader, n: usize) -> Result<Vec<Face>> {
        let mut faces = Vec::with_capacity(n);
        for _ in 0..n {
            let v1 = r.i32()?;
            let v2 = r.i32()?;
            let v3 = r.i32()?;
            // Stored as whole texels; the Direct3D loader divided by 256.
            let mut t = [0f32; 6];
            for c in t.iter_mut() {
                *c = r.i32()? as f32 / 256.0;
            }
            let flags = r.u16()?;
            let dmask = r.u16()?;
            let distant = r.i32()?;
            let next = r.i32()?;
            let group = r.i32()?;
            r.skip(12)?;
            // Order in the file: tax, tbx, tcx, tay, tby, tcy.
            faces.push(Face {
                v: [v1.max(0) as u32, v2.max(0) as u32, v3.max(0) as u32],
                uv: [[t[0], t[3]], [t[1], t[4]], [t[2], t[5]]],
                flags,
                dmask,
                distant,
                next,
                group,
            });
        }
        Ok(faces)
    }

    fn read_vertices(r: &mut Reader, n: usize) -> Result<Vec<Vertex>> {
        let mut verts = Vec::with_capacity(n);
        for _ in 0..n {
            let x = r.f32()?;
            let y = r.f32()?;
            let z = r.f32()?;
            let owner = r.i16()?;
            let hide = r.i16()?;
            verts.push(Vertex {
                pos: [x * 2.0, y * 2.0, -z * 2.0],
                owner,
                hide,
            });
        }
        Ok(verts)
    }

    fn finish(vertices: Vec<Vertex>, faces: Vec<Face>, texture: Vec<u16>) -> Result<Model> {
        for f in &faces {
            for &i in &f.v {
                if i as usize >= vertices.len() {
                    return Err(crate::FormatError(format!(
                        "face refers to vertex {i} of {}",
                        vertices.len()
                    )));
                }
            }
        }
        Ok(Model {
            vertices,
            faces,
            texture,
        })
    }

    /// The layout used inside .RSC files and by .3DF files: the counts, an
    /// object (bone) count, the texture size in bytes, then the lists.
    pub fn read_with_objects(r: &mut Reader) -> Result<Model> {
        let vc = r.count("vertex", MAX_VERTICES)?;
        let fc = r.count("face", MAX_FACES)?;
        let oc = r.count("object", 4096)?;
        let ts = r.count("texture byte", 1 << 20)?;
        let faces = Self::read_faces(r, fc)?;
        let vertices = Self::read_vertices(r, vc)?;
        r.skip(oc * 48)?;
        let texture = r.u16_vec(ts / 2)?;
        Self::finish(vertices, faces, texture)
    }

    /// The layout inside .CAR files: no object list.
    pub fn read_car(r: &mut Reader) -> Result<(Model, usize)> {
        let vc = r.count("vertex", MAX_VERTICES)?;
        let fc = r.count("face", MAX_FACES)?;
        let ts = r.count("texture byte", 1 << 20)?;
        let faces = Self::read_faces(r, fc)?;
        let vertices = Self::read_vertices(r, vc)?;
        let texture = r.u16_vec(ts / 2)?;
        Ok((Self::finish(vertices, faces, texture)?, vc))
    }

    /// A loose .3DF file. Carnivores 2 and Ice Age brighten its texture as
    /// they do every other: pass (brightness, day_night).
    pub fn load_3df(path: &std::path::Path, bright: Option<(i32, i32)>) -> Result<Model> {
        let data = std::fs::read(path)
            .map_err(|e| crate::FormatError(format!("{}: {e}", path.display())))?;
        let mut m = Self::read_with_objects(&mut Reader::new(&data))?;
        if let Some((b, dn)) = bright {
            crate::color::brighten(&mut m.texture, b, dn);
        }
        Ok(m)
    }
}

/// A vertex animation: every frame is a full set of vertex positions, in
/// eighths of a unit with z flipped like the model's.
#[derive(Clone, Debug)]
pub struct Animation {
    pub name: String,
    /// Frames per second.
    pub kps: i32,
    pub frames: usize,
    /// Length in milliseconds.
    pub ani_time: i32,
    /// frames * vertex_count * 3
    pub data: Vec<i16>,
}

impl Animation {
    /// Vertex positions at `time` milliseconds into the animation, blended
    /// between the two nearest frames.
    pub fn sample(&self, vcount: usize, time: i32, scale: f32, out: &mut Vec<[f32; 3]>) {
        out.clear();
        if self.frames == 0 || vcount == 0 || self.ani_time <= 0 {
            return;
        }
        let t = time.clamp(0, self.ani_time) as i64;
        let cur = ((self.frames as i64 - 1) * t * 256) / self.ani_time as i64;
        let spline = (cur & 0xFF) as f32;
        let mut frame = (cur >> 8) as usize;
        if frame >= self.frames {
            frame = self.frames - 1;
        }
        let next = (frame + 1).min(self.frames - 1);
        let k2 = spline / 256.0 * scale / 8.0;
        let k1 = (1.0 - spline / 256.0) * scale / 8.0;
        let a = &self.data[frame * vcount * 3..];
        let b = &self.data[next * vcount * 3..];
        for v in 0..vcount {
            let x = a[v * 3] as f32 * k1 + b[v * 3] as f32 * k2;
            let y = a[v * 3 + 1] as f32 * k1 + b[v * 3 + 1] as f32 * k2;
            let z = -(a[v * 3 + 2] as f32 * k1) - b[v * 3 + 2] as f32 * k2;
            out.push([x, y, z]);
        }
    }
}
