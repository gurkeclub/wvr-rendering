use std::convert::TryFrom;
use std::path::Path;
use std::{collections::HashMap, path::MAIN_SEPARATOR};

use anyhow::{Context, Result};

use glium::framebuffer::SimpleFrameBuffer;
use glium::index::PrimitiveType;
use glium::program::ProgramChooserCreationError;
use glium::program::ProgramCreationError;
use glium::program::ShaderType;
use glium::texture::texture2d::Texture2d;
use glium::texture::DepthTexture2d;
use glium::texture::SrgbTexture2d;
use glium::uniforms::{AsUniformValue, UniformValue, Uniforms};
use glium::uniforms::{MagnifySamplerFilter, MinifySamplerFilter};
use glium::uniforms::{Sampler, SamplerWrapFunction};
use glium::Display;
use glium::IndexBuffer;
use glium::Program;
use glium::Surface;
use glium::VertexBuffer;

use wvr_data::config::project_config::FilterConfig;
use wvr_data::shader::Shader;
use wvr_data::shader::{FileShader, ShaderComposer};

use crate::uniform::UniformHolder;

#[derive(Copy, Clone)]
pub struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}

implement_vertex!(Vertex, position, tex_coords);

struct CustomUniforms<'hihi> {
    pub primitive_list: Vec<(&'hihi String, &'hihi dyn AsUniformValue)>,
    pub render_targets_list: Vec<(&'hihi String, Sampler<'hihi, Texture2d>)>,
    pub texture_list: Vec<(&'hihi String, Sampler<'hihi, SrgbTexture2d>)>,
    pub buffer_list: Vec<(&'hihi String, Sampler<'hihi, DepthTexture2d>)>,
}

impl<'hihi> Uniforms for CustomUniforms<'hihi> {
    fn visit_values<'a, F: FnMut(&str, UniformValue<'a>)>(&'a self, mut output: F) {
        for (uniform_name, uniform_value) in self.primitive_list.iter() {
            output(uniform_name, uniform_value.as_uniform_value());
        }

        for (uniform_name, texture_sampler) in self.render_targets_list.iter() {
            output(uniform_name, texture_sampler.as_uniform_value());
        }

        for (uniform_name, texture_sampler) in self.texture_list.iter() {
            output(uniform_name, texture_sampler.as_uniform_value());
        }

        for (uniform_name, buffer_sampler) in self.buffer_list.iter() {
            output(uniform_name, buffer_sampler.as_uniform_value());
        }
    }
}

fn parse_error_message(
    error: &ProgramChooserCreationError,
    vertex_text: &str,
    fragment_text: &str,
) -> Result<String> {
    let mut result_message = String::new();
    match error {
        ProgramChooserCreationError::ProgramCreationError(e) => {
            match e {
                ProgramCreationError::CompilationError(message, shader_type) => {
                    let mut message_parts = message.split(':');
                    if let Some(_) = message_parts.next() {
                        if let Some(position_info) = message_parts.next() {
                            let mut position_info_parts = position_info.split('(');
                            if let Some(error_line) = position_info_parts.next() {
                                let error_line: usize = error_line
                                    .parse()
                                    .context("Failed to parse error line for shader error.")?;
                                if let Some(error_char) = position_info_parts.next() {
                                    let error_char: usize =
                                        error_char[..error_char.len() - 1].parse().context(
                                            "Failed to parse error position for shader error",
                                        )?;
                                    let error_message = message_parts
                                        .collect::<String>()
                                        .lines()
                                        .next()
                                        .unwrap_or("")
                                        .to_owned();

                                    let code_line = match shader_type {
                                    ShaderType::Vertex => vertex_text.lines().nth(error_line - 1).context("Failed to find faulty error in vertex shader file")?,
                                    ShaderType::Fragment => fragment_text.lines().nth(error_line - 1).context("Failed to find faulty error in fragment shader file")?,
                                    _ => unreachable!(),
                                };

                                    result_message.push_str(&code_line.to_string());
                                    result_message.push('\n');

                                    result_message.push_str(
                                        &(0..error_char).map(|_| " ").collect::<String>(),
                                    );
                                    result_message.push('^');
                                    result_message.push('\n');

                                    result_message.push_str(&error_message);
                                    result_message.push('\n');
                                }
                            }
                        }
                    }
                }
                e => result_message.push_str(&e.to_string()),
            }
        }
        e => result_message.push_str(&e.to_string()),
    }

    Ok(result_message)
}

pub struct Filter {
    name: String,
    resolution: (usize, usize),
    time: f64,
    beat: f64,
    bpm: f64,
    frame_count: usize,
    mouse_position: (f64, f64, f64, f64),

    vertex_shader: Box<dyn Shader>,
    fragment_shader: Box<dyn Shader>,

    uniform_holder: HashMap<
        String,
        (
            UniformHolder,
            Option<(MinifySamplerFilter, MagnifySamplerFilter)>,
        ),
    >,
    inputs: Vec<String>,

    vertex_buffer: VertexBuffer<Vertex>,
    index_buffer: IndexBuffer<u16>,

    vertex_text: String,
    fragment_text: String,
    program: Program,
}

impl Filter {
    pub fn from_config(
        project_path: &Path,
        name: &str,
        config: &FilterConfig,
        display: &Display,
        resolution: (usize, usize),
    ) -> Result<Self> {
        let mut vertex_shader = Box::new(ShaderComposer::default());

        for shader_file in config.vertex_shader.iter() {
            let shader_file = shader_file.replace('/', MAIN_SEPARATOR.to_string().as_str());
            let shader_file_path = match shader_file
                .chars()
                .next()
                .context("Empty shader file path")?
            {
                '#' => {
                    let mut shader_file = shader_file.chars();
                    shader_file.next();
                    wvr_data::get_libs_path().join(&shader_file.as_str())
                }
                _ => project_path.join(shader_file),
            };

            vertex_shader.push(Box::new(FileShader::new(shader_file_path, true)?));
        }

        let mut fragment_shader = Box::new(ShaderComposer::default());

        for shader_file in config.fragment_shader.iter() {
            let mut live_reload = true;

            let shader_file = shader_file.replace('/', MAIN_SEPARATOR.to_string().as_str());

            let shader_file_path = match shader_file
                .chars()
                .next()
                .context("Empty shader file path")?
            {
                '#' => {
                    live_reload = false;

                    let mut shader_file = shader_file.chars();
                    shader_file.next();
                    wvr_data::get_libs_path().join(shader_file.as_str())
                }
                _ => project_path.join(shader_file),
            };

            fragment_shader.push(Box::new(FileShader::new(shader_file_path, live_reload)?));
        }

        let mut uniform_holder = HashMap::new();

        for (variable_name, variable_value) in &config.variables {
            if let Ok(variable_value) = UniformHolder::try_from((display, variable_value)) {
                uniform_holder.insert(variable_name.clone(), (variable_value, None));
            }
        }

        Self::new(
            display,
            name,
            resolution,
            vertex_shader,
            fragment_shader,
            config.inputs.clone(),
            uniform_holder,
        )
    }

    pub fn new(
        display: &Display,
        name: &str,
        resolution: (usize, usize),
        vertex_shader: Box<dyn Shader>,
        fragment_shader: Box<dyn Shader>,
        inputs: Vec<String>,
        uniform_holder: HashMap<
            String,
            (
                UniformHolder,
                Option<(MinifySamplerFilter, MagnifySamplerFilter)>,
            ),
        >,
    ) -> Result<Self> {
        let vertex_buffer = {
            VertexBuffer::new(
                display,
                &[
                    Vertex {
                        position: [-1.0, -1.0],
                        tex_coords: [0.0, 0.0],
                    },
                    Vertex {
                        position: [-1.0, 1.0],
                        tex_coords: [0.0, 1.0],
                    },
                    Vertex {
                        position: [1.0, 1.0],
                        tex_coords: [1.0, 1.0],
                    },
                    Vertex {
                        position: [1.0, -1.0],
                        tex_coords: [1.0, 0.0],
                    },
                ],
            )
            .context("Failed to create vertex buffer")?
        };

        // building the index buffer
        let index_buffer =
            IndexBuffer::new(display, PrimitiveType::TriangleStrip, &[1 as u16, 2, 0, 3])
                .context("Failed to create index buffer")?;

        let vertex_text = vertex_shader.get_text().to_owned();
        let fragment_text = fragment_shader.get_text().to_owned();

        // compiling shaders and linking them together

        let program = match program!(display, 140 => { vertex: &vertex_text, fragment: &fragment_text })
        {
            Ok(program) => program,
            Err(e) => panic!(
                "{:}",
                parse_error_message(&e, &vertex_text, &fragment_text)
                    .unwrap_or(format!("Unexpected shader error: {:?}", e))
            ),
        };
        Ok(Self {
            name: name.to_owned(),
            resolution,
            time: 0.0,
            beat: 0.0,
            bpm: 110.0,
            mouse_position: (0.0, 0.0, 0.0, 0.0),
            frame_count: 0,

            vertex_shader,
            fragment_shader,

            uniform_holder,
            inputs,

            vertex_buffer,
            index_buffer,

            vertex_text,
            fragment_text,
            program,
        })
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm;
    }

    pub fn set_time(&mut self, time: f64) {
        self.time = time;
    }

    pub fn set_beat(&mut self, beat: f64) {
        self.beat = beat;
    }

    pub fn set_frame_count(&mut self, frame_count: usize) {
        self.frame_count = frame_count;
    }

    pub fn set_resolution(&mut self, resolution: (usize, usize)) {
        self.resolution = resolution;
    }

    pub fn set_mouse_position(&mut self, position: (f64, f64)) {
        self.mouse_position = (position.0, position.1, 0.0, 0.0);
    }

    pub fn update(&mut self, display: &Display) {
        self.vertex_shader.update();
        self.fragment_shader.update();

        let vertex_changed = match self.vertex_shader.check_changes() {
            Ok(changed) => changed,
            Err(e) => {
                eprintln!("{:?}", e);
                false
            }
        };

        let fragment_changed = match self.fragment_shader.check_changes() {
            Ok(changed) => changed,
            Err(e) => {
                eprintln!("{:?}", e);
                false
            }
        };

        if vertex_changed || fragment_changed {
            if vertex_changed {
                self.vertex_text.clear();
                self.vertex_text.push_str(self.vertex_shader.get_text());
            }

            if fragment_changed {
                self.fragment_text.clear();
                self.fragment_text.push_str(self.fragment_shader.get_text());
            }

            match program!(display, 140 => { vertex: &self.vertex_text, fragment: &self.fragment_text })
            {
                Ok(new_program) => {
                    self.program = new_program;
                }
                Err(e) => eprintln!(
                    "{:}",
                    parse_error_message(&e, &self.vertex_text, &self.fragment_text)
                        .unwrap_or(format!("Unexpected shader error: {:?}", e))
                ),
            }
        }

        self.uniform_holder.insert(
            "matrix".to_owned(),
            (
                UniformHolder::Mat4([
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0f32],
                ]),
                None,
            ),
        );

        self.uniform_holder.insert(
            "iResolution".to_owned(),
            (
                UniformHolder::Float3((self.resolution.0 as f32, self.resolution.1 as f32, 0.0)),
                None,
            ),
        );
        self.uniform_holder.insert(
            "iMouse".to_owned(),
            (
                UniformHolder::Float4((
                    self.mouse_position.0 as f32,
                    self.mouse_position.1 as f32,
                    self.mouse_position.2 as f32,
                    self.mouse_position.3 as f32,
                )),
                None,
            ),
        );
        self.uniform_holder.insert(
            "iTime".to_owned(),
            (UniformHolder::Float(self.time as f32), None),
        );
        self.uniform_holder.insert(
            "iBeat".to_owned(),
            (UniformHolder::Float(self.beat as f32), None),
        );
        self.uniform_holder.insert(
            "iFrame".to_owned(),
            (UniformHolder::Integer(self.frame_count as i32), None),
        );
    }

    pub fn render(
        &self,
        display: &Display,
        input_holder: &HashMap<
            &String,
            (
                &UniformHolder,
                Option<(MinifySamplerFilter, MagnifySamplerFilter)>,
            ),
        >,
        render_buffers: &HashMap<
            &String,
            (
                &Texture2d,
                Option<(MinifySamplerFilter, MagnifySamplerFilter)>,
            ),
        >,
        framebuffer_texture: Option<&Texture2d>,
    ) -> Result<()> {
        let mut uniform_vec: Vec<(&String, &dyn AsUniformValue)> = Vec::new();
        let mut uniform_render_targets_vec = Vec::new();
        let mut uniform_textures_vec = Vec::new();
        let mut uniform_buffers_vec = Vec::new();

        for (uniform_name, (value, sampling)) in &self.uniform_holder {
            match value {
                UniformHolder::Buffer((texture, _length)) => {
                    if let Some((down_sampling, up_sampling)) = sampling {
                        let texture = texture
                            .sampled()
                            .wrap_function(SamplerWrapFunction::BorderClamp)
                            .minify_filter(*down_sampling)
                            .magnify_filter(*up_sampling);
                        uniform_buffers_vec.push((uniform_name, texture));
                    }
                }
                UniformHolder::Texture((texture, _resolution)) => {
                    if let Some((down_sampling, up_sampling)) = sampling {
                        let texture = texture
                            .sampled()
                            .wrap_function(SamplerWrapFunction::Repeat)
                            .minify_filter(*down_sampling)
                            .magnify_filter(*up_sampling);
                        uniform_textures_vec.push((uniform_name, texture));
                    }
                }
                UniformHolder::Float(value) => uniform_vec.push((uniform_name, value)),
                UniformHolder::Float2(value) => uniform_vec.push((uniform_name, value)),
                UniformHolder::Float3(value) => uniform_vec.push((uniform_name, value)),
                UniformHolder::Float4(value) => uniform_vec.push((uniform_name, value)),
                UniformHolder::Integer(value) => uniform_vec.push((uniform_name, value)),
                UniformHolder::Bool(value) => uniform_vec.push((uniform_name, value)),

                UniformHolder::Mat2(value) => uniform_vec.push((uniform_name, value)),
                UniformHolder::Mat3(value) => uniform_vec.push((uniform_name, value)),
                UniformHolder::Mat4(value) => uniform_vec.push((uniform_name, value)),
            }
        }

        for uniform_name in &self.inputs {
            if let Some((texture, Some((down_sampling, up_sampling)))) =
                render_buffers.get(uniform_name)
            {
                let texture = texture
                    .sampled()
                    .wrap_function(SamplerWrapFunction::Repeat)
                    .minify_filter(*down_sampling)
                    .magnify_filter(*up_sampling);
                uniform_render_targets_vec.push((uniform_name, texture));
            } else if let Some((value, sampling)) = input_holder.get(uniform_name) {
                match value {
                    UniformHolder::Buffer((texture, _length)) => {
                        if let Some((down_sampling, up_sampling)) = sampling {
                            let texture = texture
                                .sampled()
                                .wrap_function(SamplerWrapFunction::BorderClamp)
                                .minify_filter(*down_sampling)
                                .magnify_filter(*up_sampling);
                            uniform_buffers_vec.push((uniform_name, texture));
                        }
                    }
                    UniformHolder::Texture((texture, _resolution)) => {
                        if let Some((down_sampling, up_sampling)) = sampling {
                            let texture = texture
                                .sampled()
                                .wrap_function(SamplerWrapFunction::Repeat)
                                .minify_filter(*down_sampling)
                                .magnify_filter(*up_sampling);
                            uniform_textures_vec.push((uniform_name, texture));
                        }
                    }
                    UniformHolder::Float(value) => uniform_vec.push((uniform_name, value)),
                    UniformHolder::Float2(value) => uniform_vec.push((uniform_name, value)),
                    UniformHolder::Float3(value) => uniform_vec.push((uniform_name, value)),
                    UniformHolder::Float4(value) => uniform_vec.push((uniform_name, value)),
                    UniformHolder::Integer(value) => uniform_vec.push((uniform_name, value)),
                    UniformHolder::Bool(value) => uniform_vec.push((uniform_name, value)),
                    UniformHolder::Mat2(value) => uniform_vec.push((uniform_name, value)),
                    UniformHolder::Mat3(value) => uniform_vec.push((uniform_name, value)),
                    UniformHolder::Mat4(value) => uniform_vec.push((uniform_name, value)),
                }
            }
        }

        let uniforms_holder = CustomUniforms {
            primitive_list: uniform_vec,
            render_targets_list: uniform_render_targets_vec,
            texture_list: uniform_textures_vec,
            buffer_list: uniform_buffers_vec,
        };

        if let Some(framebuffer_texture) = framebuffer_texture {
            let mut framebuffer = SimpleFrameBuffer::new(display, framebuffer_texture)
                .context("Failed to create target buffer for rendering")?;
            framebuffer.clear_color(1.0, 0.0, 1.0, 0.0);
            framebuffer
                .draw(
                    &self.vertex_buffer,
                    &self.index_buffer,
                    &self.program,
                    &uniforms_holder,
                    &Default::default(),
                )
                .context("Failed to render filter to framebuffer")?;
        } else {
            let mut target = display.draw();
            target.clear_color(0.0, 0.0, 0.0, 0.0);
            target
                .draw(
                    &self.vertex_buffer,
                    &self.index_buffer,
                    &self.program,
                    &uniforms_holder,
                    &Default::default(),
                )
                .context("Failed to render filter to display")?;

            target.finish().context("Failed to finalize rendering")?;
        }

        Ok(())
    }
}
