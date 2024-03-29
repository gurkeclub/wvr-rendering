use std::convert::TryFrom;

use anyhow::{Context, Error, Result};

use glium::backend::Facade;
use glium::texture::RawImage2d;
use glium::texture::SrgbTexture2d;
use glium::texture::Texture2d;
use glium::texture::{DepthTexture2d, MipmapsOption};

use wvr_data::types::DataHolder;

pub enum UniformHolder {
    Buffer((DepthTexture2d, usize)),
    Texture((Texture2d, (u32, u32))),
    SrgbTexture((SrgbTexture2d, (u32, u32))),

    Float(f32),
    Float2((f32, f32)),
    Float3((f32, f32, f32)),
    Float4((f32, f32, f32, f32)),

    Integer(i32),
    Bool(bool),

    Mat2([[f32; 2]; 2]),
    Mat3([[f32; 3]; 3]),
    Mat4([[f32; 4]; 4]),
}

impl TryFrom<(&dyn Facade, &DataHolder, bool)> for UniformHolder {
    type Error = Error;

    fn try_from(uniform: (&dyn Facade, &DataHolder, bool)) -> Result<UniformHolder> {
        let (display, uniform, generate_mipmaps) = uniform;
        match uniform {
            DataHolder::Float(value) => Ok(UniformHolder::Float(*value as f32)),
            DataHolder::Float2(value) => Ok(UniformHolder::Float2((value[0], value[1]))),
            DataHolder::Float3(value) => Ok(UniformHolder::Float3((value[0], value[1], value[2]))),
            DataHolder::Float4(value) => Ok(UniformHolder::Float4((
                value[0], value[1], value[2], value[3],
            ))),
            DataHolder::Int(value) => Ok(UniformHolder::Integer(*value as i32)),
            DataHolder::Bool(value) => Ok(UniformHolder::Bool(*value)),
            DataHolder::Texture((resolution, texture_data)) => {
                let image = RawImage2d::from_raw_rgb(texture_data.clone(), *resolution);
                let texture = Texture2d::with_mipmaps(display, image, MipmapsOption::EmptyMipmaps)
                    .context("Failed to build texture from texture data")?;

                if generate_mipmaps {
                    unsafe {
                        texture.generate_mipmaps();
                    }
                }

                Ok(UniformHolder::Texture((texture, *resolution)))
            }
            DataHolder::SrgbTexture((resolution, texture_data)) => {
                let image = RawImage2d::from_raw_rgb(texture_data.clone(), *resolution);
                let texture =
                    SrgbTexture2d::with_mipmaps(display, image, MipmapsOption::EmptyMipmaps)
                        .context("Failed to build texture from texture data")?;

                if generate_mipmaps {
                    unsafe {
                        texture.generate_mipmaps();
                    }
                }

                Ok(UniformHolder::SrgbTexture((texture, *resolution)))
            }
            DataHolder::FloatArray(array) => Ok(UniformHolder::Buffer((
                DepthTexture2d::new(display, vec![array.clone()])
                    .context("Failed to build buffer from float array")?,
                array.len(),
            ))),
            DataHolder::BoolArray(array) => Ok(UniformHolder::Buffer((
                DepthTexture2d::new(
                    display,
                    vec![array.iter().map(|&x| if x { 1.0 } else { 0.0 }).collect()],
                )
                .context("Failed to build buffer from boolean array")?,
                array.len(),
            ))),

            DataHolder::IntArray(array) => Ok(UniformHolder::Buffer((
                DepthTexture2d::new(
                    display,
                    vec![array.iter().map(|&x| x as f32 / 2f32.powf(32.0)).collect()],
                )
                .context("Failed to build buffer from integer array")?,
                array.len(),
            ))),

            DataHolder::ByteArray(array) => Ok(UniformHolder::Buffer((
                DepthTexture2d::new(
                    display,
                    vec![array.iter().map(|&x| x as f32 / 255.0).collect()],
                )
                .context("Failed to build buffer from byte array")?,
                array.len(),
            ))),
            _ => unimplemented!(),
        }
    }
}
