use std::convert::TryFrom;

use anyhow::{Context, Error, Result};

use glium::texture::DepthTexture2d;
use glium::texture::RawImage2d;
use glium::texture::SrgbTexture2d;
use glium::Display;

use wvr_data::DataHolder;

pub enum UniformHolder {
    Buffer((DepthTexture2d, usize)),
    Texture((SrgbTexture2d, (u32, u32))),

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

impl TryFrom<(&Display, &DataHolder)> for UniformHolder {
    type Error = Error;

    fn try_from(uniform: (&Display, &DataHolder)) -> Result<UniformHolder> {
        let (display, uniform) = uniform;
        match uniform {
            DataHolder::Float(value) => Ok(UniformHolder::Float(*value as f32)),
            DataHolder::Int(value) => Ok(UniformHolder::Integer(*value as i32)),
            DataHolder::Bool(value) => Ok(UniformHolder::Bool(*value)),
            DataHolder::Texture((resolution, texture_data)) => {
                let image = RawImage2d::from_raw_rgb(texture_data.clone(), *resolution);
                Ok(UniformHolder::Texture((
                    SrgbTexture2d::new(display, image)
                        .context("Failed to build texture from texture data")?,
                    *resolution,
                )))
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
