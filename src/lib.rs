#[macro_use]
extern crate glium;
extern crate wvr_data;

use std::borrow::Cow;
use std::collections::hash_map::HashMap;
use std::convert::TryFrom;
use std::path::PathBuf;
use std::time::Instant;
use std::vec::Vec;

use anyhow::{Context, Result};

use glium::framebuffer::SimpleFrameBuffer;
use glium::glutin;
use glium::glutin::event_loop::EventLoop;
use glium::texture::MipmapsOption;
use glium::texture::SrgbTexture2d;
use glium::texture::Texture2d;
use glium::texture::Texture2dDataSink;
use glium::uniforms::MagnifySamplerFilter;
use glium::BlitTarget;
use glium::Display;
use glium::Rect;
use glium::Surface;
use glium::{backend::Facade, uniforms::MinifySamplerFilter};

use glutin::dpi::PhysicalSize;
use glutin::window::WindowBuilder;
use glutin::ContextBuilder;

use wvr_data::config::project_config::{FilterConfig, RenderStageConfig, SampledInput, ViewConfig};
use wvr_data::InputProvider;

pub mod filter;
pub mod stage;
pub mod uniform;

use filter::Filter;
use stage::Stage;
use uniform::UniformHolder;

pub struct RGBAImageData {
    pub data: Vec<(u8, u8, u8, u8)>,
    pub width: u32,
    pub height: u32,
}

impl Texture2dDataSink<(u8, u8, u8, u8)> for RGBAImageData {
    fn from_raw(data: Cow<[(u8, u8, u8, u8)]>, width: u32, height: u32) -> Self {
        RGBAImageData {
            data: data.into_owned(),
            width,
            height,
        }
    }
}

pub struct ShaderView {
    display: Display,

    uniform_holder: HashMap<String, UniformHolder>,

    begin_time: Instant,
    last_update_time: Instant,

    resolution: (usize, usize),
    frame_count: usize,
    beat: f64,
    bpm: f64,
    mouse_position: (f64, f64),

    target_fps: f64,
    locked_speed: bool,
    dynamic: bool,

    filter_list: HashMap<String, Filter>,
    render_buffer_list: HashMap<String, (Vec<Texture2d>, (u32, u32))>,
    view_chain: Vec<Stage>,
    final_stage: Stage,
}

impl ShaderView {
    pub fn new(
        bpm: f64,
        view_config: &ViewConfig,
        render_chain: &[RenderStageConfig],
        final_stage_config: &RenderStageConfig,
        filters: &HashMap<String, (PathBuf, FilterConfig)>,
        events_loop: &EventLoop<()>,
    ) -> Result<Self> {
        let fullscreen = if view_config.fullscreen {
            let monitor = events_loop.primary_monitor();
            if let Some(monitor) = monitor {
                Some(glium::glutin::window::Fullscreen::Exclusive(
                    monitor.video_modes().next().unwrap(),
                ))
            } else {
                None
            }
        } else {
            None
        };

        let window = WindowBuilder::new()
            .with_inner_size(PhysicalSize::new(
                view_config.width as u32,
                view_config.height as u32,
            ))
            .with_resizable(view_config.dynamic)
            .with_fullscreen(fullscreen)
            .with_title("wvr");
        let window = if view_config.dynamic {
            window
        } else {
            window
                .with_min_inner_size(PhysicalSize::new(
                    view_config.width as u32,
                    view_config.height as u32,
                ))
                .with_max_inner_size(PhysicalSize::new(
                    view_config.width as u32,
                    view_config.height as u32,
                ))
        };
        let context = ContextBuilder::new()
            .with_vsync(view_config.vsync)
            .with_srgb(true);

        let display = Display::new(window, context, &events_loop)
            .context("Failed to create the rendering window")?;

        let resolution = (view_config.width as usize, view_config.height as usize);

        let mut view_chain = Vec::new();
        let mut filter_list = HashMap::new();
        let mut render_buffer_list = HashMap::new();

        for (filter_name, (filter_path, filter_config)) in filters {
            let filter = Filter::from_config(
                &[&filter_path.join("src"), &wvr_data::get_libs_path()],
                filter_config,
                &display,
                resolution,
            )?;
            filter_list.insert(filter_name.clone(), filter);
        }

        for render_stage_config in render_chain {
            let stage =
                Stage::from_config(&render_stage_config.name, &display, render_stage_config)
                    .context("Failed to build render stage")?;

            render_buffer_list.insert(
                render_stage_config.name.clone(),
                (
                    vec![
                        Texture2d::empty_with_format(
                            &display,
                            stage.get_buffer_format(),
                            MipmapsOption::EmptyMipmaps,
                            resolution.0 as u32,
                            resolution.1 as u32,
                        )
                        .context("Failed to create a rendering buffer")?,
                        Texture2d::empty_with_format(
                            &display,
                            stage.get_buffer_format(),
                            MipmapsOption::EmptyMipmaps,
                            resolution.0 as u32,
                            resolution.1 as u32,
                        )
                        .context("Failed to create a rendering buffer")?,
                    ],
                    (resolution.0 as u32, resolution.1 as u32),
                ),
            );

            view_chain.push(stage);
        }

        let final_stage =
            Stage::from_config(&final_stage_config.name, &display, final_stage_config)
                .context("Failed to build final render stage")?;

        Ok(Self {
            display,

            uniform_holder: HashMap::new(),

            begin_time: Instant::now(),
            last_update_time: Instant::now(),

            resolution,
            bpm,
            frame_count: 0,
            beat: 0.0,
            mouse_position: (0.0, 0.0),

            target_fps: view_config.target_fps as f64,
            locked_speed: view_config.locked_speed,

            dynamic: view_config.dynamic,

            filter_list,
            render_buffer_list,
            view_chain,
            final_stage,
        })
    }
    pub fn get_display(&self) -> &Display {
        &self.display
    }

    pub fn get_frame_count(&self) -> usize {
        self.frame_count
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm;
    }

    pub fn set_mouse_position(&mut self, position: (f64, f64)) {
        self.mouse_position = position;
    }

    pub fn update(
        &mut self,
        uniform_sources: &mut HashMap<String, Box<dyn InputProvider>>,
    ) -> Result<()> {
        let new_update_time = Instant::now();

        self.beat += if self.locked_speed {
            self.bpm / (60.0 * self.target_fps)
        } else {
            let time_diff = new_update_time - self.last_update_time;
            time_diff.as_secs_f64() * self.bpm / 60.0
        };

        let current_time = if self.locked_speed {
            self.frame_count as f64 / self.target_fps
        } else {
            self.begin_time.elapsed().as_secs_f64()
        };

        let new_resolution = self.display.get_framebuffer_dimensions();
        let new_resolution = (new_resolution.0 as usize, new_resolution.1 as usize);

        if new_resolution != self.resolution && self.dynamic {
            self.set_resolution(new_resolution)?;
        }

        for (_input_name, source) in uniform_sources.iter_mut() {
            source.set_beat(self.beat, self.locked_speed);
            source.set_time(current_time, self.locked_speed);

            for ref source_id in source.provides() {
                if let Some(ref value) = source.get(&source_id, true) {
                    if let Ok(value) = UniformHolder::try_from((&self.display, value)) {
                        self.uniform_holder.insert(source_id.to_owned(), value);
                    }
                }
            }
        }

        for filter in self.filter_list.values_mut() {
            filter.set_time(current_time);
            filter.set_beat(self.beat);
            filter.set_frame_count(self.frame_count);
            filter.set_mouse_position(self.mouse_position);
            filter.set_resolution(self.resolution);

            filter.update(&self.display);
        }

        self.last_update_time = new_update_time;

        Ok(())
    }

    pub fn render(&mut self) -> Result<()> {
        for stage in self.view_chain.iter() {
            if let Some((render_target_pack, _)) = self.render_buffer_list.get(stage.get_name()) {
                let render_target = &render_target_pack[1];

                self.render_stage(stage, Some(render_target))?;
            }

            if let Some((ref mut render_target_pack, _)) =
                self.render_buffer_list.get_mut(stage.get_name())
            {
                let tmp_buffer = render_target_pack.remove(0);
                render_target_pack.push(tmp_buffer);

                unsafe {
                    render_target_pack[0].generate_mipmaps(); //finish().context("Failed to finalize framebuffer rendering")?;
                }
            }
        }

        self.render_stage(&self.final_stage, None)?;

        self.frame_count += 1;

        Ok(())
    }

    pub fn render_stage(
        &self,
        stage: &Stage,
        framebuffer_texture: Option<&Texture2d>,
    ) -> Result<()> {
        let mut render_buffer_list = HashMap::new();
        let mut input_holder = HashMap::new();

        for (uniform_name, input_name) in stage.get_input_map() {
            let (input_name, down_sampling, up_sampling) = match input_name {
                SampledInput::Nearest(input_name) => (
                    input_name,
                    MinifySamplerFilter::Nearest,
                    MagnifySamplerFilter::Nearest,
                ),
                SampledInput::Linear(input_name) => (
                    input_name,
                    MinifySamplerFilter::Linear,
                    MagnifySamplerFilter::Linear,
                ),
                SampledInput::Mipmaps(input_name) => (
                    input_name,
                    MinifySamplerFilter::LinearMipmapNearest,
                    MagnifySamplerFilter::Linear,
                ),
            };

            if let Some(render_buffer_pack) = self.render_buffer_list.get(input_name) {
                render_buffer_list.insert(
                    uniform_name,
                    (&render_buffer_pack.0[0], Some((down_sampling, up_sampling))),
                );
            } else if let Some(uniform_value) = self.uniform_holder.get(input_name) {
                input_holder.insert(
                    uniform_name,
                    (uniform_value, Some((down_sampling, up_sampling))),
                );
            }
        }

        for (uniform_name, uniform_value) in stage.get_variable_list() {
            input_holder.insert(uniform_name, (uniform_value, None));
        }

        let filter_name = stage.get_filter();
        if let Some(filter) = self.filter_list.get(filter_name) {
            filter.render(
                &self.display,
                &input_holder,
                &render_buffer_list,
                framebuffer_texture,
            )?;
        }

        Ok(())
    }

    pub fn set_resolution(&mut self, resolution: (usize, usize)) -> Result<()> {
        self.resolution = resolution;
        self.render_buffer_list.clear();

        for stage in self.view_chain.iter_mut() {
            let new_render_buffer_pair = (
                vec![
                    Texture2d::empty_with_format(
                        &self.display,
                        stage.get_buffer_format(),
                        MipmapsOption::EmptyMipmaps,
                        self.resolution.0 as u32,
                        self.resolution.1 as u32,
                    )
                    .context("Failed to create a rendering buffer")?,
                    Texture2d::empty_with_format(
                        &self.display,
                        stage.get_buffer_format(),
                        MipmapsOption::EmptyMipmaps,
                        self.resolution.0 as u32,
                        self.resolution.1 as u32,
                    )
                    .context("Failed to create a rendering buffer")?,
                ],
                (self.resolution.0 as u32, self.resolution.1 as u32),
            );
            self.render_buffer_list
                .insert(stage.get_name().to_owned(), new_render_buffer_pair);
        }

        Ok(())
    }

    pub fn request_redraw(&mut self) {
        self.display.gl_window().window().request_redraw();
    }

    pub fn take_screenshot(&self) -> Result<RGBAImageData> {
        // Get information about current framebuffer
        let dimensions = self.display.get_context().get_framebuffer_dimensions();
        let rect = Rect {
            left: 0,
            bottom: 0,
            width: dimensions.0,
            height: dimensions.1,
        };
        let blit_target = BlitTarget {
            left: 0,
            bottom: 0,
            width: dimensions.0 as i32,
            height: dimensions.1 as i32,
        };

        // Create temporary texture and blit the front buffer to it
        let texture = SrgbTexture2d::empty(&self.display, dimensions.0, dimensions.1)
            .context("Could not create empty texture for screenshot")?;
        let framebuffer = SimpleFrameBuffer::new(&self.display, &texture)
            .context("Could not create frame buffer for screenshot bliting")?;
        framebuffer.blit_from_frame(&rect, &blit_target, MagnifySamplerFilter::Nearest);

        // Read the texture into new pixel buffer
        let texture = texture
            .read_to_pixel_buffer()
            .read_as_texture_2d()
            .context("Could not read blit texture as a pixel buffer")?;
        Ok(texture)
    }
}
