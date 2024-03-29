#[macro_use]
extern crate glium;
extern crate wvr_data;

use std::borrow::Cow;
use std::collections::hash_map::HashMap;
use std::convert::TryFrom;
use std::path::PathBuf;
use std::vec::Vec;

use anyhow::{Context, Result};

use glium::texture::MipmapsOption;
use glium::texture::Texture2d;
use glium::texture::Texture2dDataSink;
use glium::uniforms::MagnifySamplerFilter;
use glium::Frame;
use glium::{backend::Facade, uniforms::MinifySamplerFilter};

use wvr_data::config::filter::FilterConfig;
use wvr_data::config::project::ViewConfig;
use wvr_data::config::rendering::RenderStageConfig;
use wvr_data::types::DataHolder;
use wvr_data::types::{InputProvider, InputSampler};

pub mod filter;
pub mod stage;
pub mod uniform;

use filter::{Filter, RenderTarget};
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
    uniform_holder: HashMap<String, UniformHolder>,

    resolution: (usize, usize),
    mouse_position: (f64, f64),

    dynamic: bool,

    filter_list: HashMap<String, Filter>,
    render_buffer_list: Vec<(Vec<Texture2d>, (u32, u32))>,
    render_chain: Vec<Stage>,
    final_stage: Stage,
}

impl ShaderView {
    pub fn new(
        view_config: &ViewConfig,
        render_chain: &[RenderStageConfig],
        final_stage_config: &RenderStageConfig,
        filters: &HashMap<String, (PathBuf, FilterConfig, bool)>,
        display: &dyn Facade,
    ) -> Result<Self> {
        let resolution = (view_config.width as usize, view_config.height as usize);

        let mut view_chain = Vec::new();
        let mut filter_list = HashMap::new();
        let mut render_buffer_list = Vec::new();

        for (filter_name, (filter_path, filter_config, system_filter)) in filters {
            let filter = Filter::from_config(
                &[&filter_path.join("src"), &wvr_data::get_libs_path()],
                filter_config,
                display,
                resolution,
                *system_filter,
            )?;
            filter_list.insert(filter_name.clone(), filter);
        }

        for render_stage_config in render_chain {
            let mut stage =
                Stage::from_config(&render_stage_config.name, display, render_stage_config)
                    .context("Failed to build render stage")?;

            render_buffer_list.push((
                vec![
                    Texture2d::empty_with_format(
                        display,
                        stage.get_buffer_format(),
                        MipmapsOption::EmptyMipmaps,
                        resolution.0 as u32,
                        resolution.1 as u32,
                    )
                    .context("Failed to create a rendering buffer")?,
                    Texture2d::empty_with_format(
                        display,
                        stage.get_buffer_format(),
                        MipmapsOption::EmptyMipmaps,
                        resolution.0 as u32,
                        resolution.1 as u32,
                    )
                    .context("Failed to create a rendering buffer")?,
                ],
                (resolution.0 as u32, resolution.1 as u32),
            ));

            stage.recreate_buffers = false;

            view_chain.push(stage);
        }

        let final_stage = Stage::from_config(&final_stage_config.name, display, final_stage_config)
            .context("Failed to build final render stage")?;

        Ok(Self {
            uniform_holder: HashMap::new(),

            resolution,
            mouse_position: (0.0, 0.0),

            dynamic: view_config.dynamic,

            filter_list,
            render_buffer_list,
            render_chain: view_chain,
            final_stage,
        })
    }

    pub fn set_mouse_position(&mut self, position: (f64, f64)) {
        self.mouse_position = position;
    }

    pub fn remove_render_stage(&mut self, stage_index: usize) {
        self.render_buffer_list.remove(stage_index);
        self.render_chain.remove(stage_index);
    }

    pub fn move_render_stage(&mut self, original_index: usize, target_index: usize) {
        let render_buffer = self.render_buffer_list.remove(original_index);
        self.render_buffer_list.insert(target_index, render_buffer);

        let render_stage = self.render_chain.remove(original_index);
        self.render_chain.insert(target_index, render_stage);
    }

    pub fn add_render_stage(&mut self, display: &dyn Facade, stage: Stage) -> Result<()> {
        self.render_buffer_list.push((
            vec![
                Texture2d::empty_with_format(
                    display,
                    stage.get_buffer_format(),
                    MipmapsOption::EmptyMipmaps,
                    self.resolution.0 as u32,
                    self.resolution.1 as u32,
                )
                .context("Failed to create a rendering buffer")?,
                Texture2d::empty_with_format(
                    display,
                    stage.get_buffer_format(),
                    MipmapsOption::EmptyMipmaps,
                    self.resolution.0 as u32,
                    self.resolution.1 as u32,
                )
                .context("Failed to create a rendering buffer")?,
            ],
            (self.resolution.0 as u32, self.resolution.1 as u32),
        ));
        self.render_chain.push(stage);

        Ok(())
    }

    pub fn get_render_chain(&mut self) -> &mut Vec<Stage> {
        &mut self.render_chain
    }
    pub fn get_final_stage(&mut self) -> &mut Stage {
        &mut self.final_stage
    }

    pub fn update(
        &mut self,
        display: &dyn Facade,
        env_variable_list: &HashMap<String, DataHolder>,
        uniform_sources: &mut HashMap<String, Box<dyn InputProvider>>,
        time: f64,
        beat: f64,
        frame_count: usize,
    ) -> Result<()> {
        let mut texture_with_mipmap_list: Vec<String> = Vec::new();
        for render_stage in &self.render_chain {
            for texture_sampling in render_stage.get_input_map().values() {
                if let InputSampler::Mipmaps(texture_name) = texture_sampling {
                    texture_with_mipmap_list.push(texture_name.to_owned());
                }
            }
        }
        for texture_sampling in self.final_stage.get_input_map().values() {
            if let InputSampler::Mipmaps(texture_name) = texture_sampling {
                texture_with_mipmap_list.push(texture_name.to_owned());
            }
        }

        for (input_name, source) in uniform_sources.iter_mut() {
            for source_id in &source.provides() {
                if let Some(ref value) = source.get(source_id, true) {
                    let source_id = if source_id.is_empty() {
                        input_name.clone()
                    } else {
                        source_id.clone()
                    };
                    if let Ok(value) = UniformHolder::try_from((
                        display as &dyn Facade,
                        value,
                        texture_with_mipmap_list.contains(&source_id),
                    )) {
                        self.uniform_holder.insert(source_id, value);
                    }
                }
            }
        }

        for filter in self.filter_list.values_mut() {
            filter.set_time(time);
            filter.set_beat(beat);
            filter.set_frame_count(frame_count);
            filter.set_mouse_position(self.mouse_position);
            filter.set_resolution(self.resolution);

            filter.update(display);
        }

        for (stage_index, ref mut stage) in self.render_chain.iter_mut().enumerate() {
            if stage.recreate_buffers {
                self.render_buffer_list.remove(stage_index);
                self.render_buffer_list.insert(
                    stage_index,
                    (
                        vec![
                            Texture2d::empty_with_format(
                                display,
                                stage.get_buffer_format(),
                                MipmapsOption::EmptyMipmaps,
                                self.resolution.0 as u32,
                                self.resolution.1 as u32,
                            )
                            .context("Failed to create a rendering buffer")?,
                            Texture2d::empty_with_format(
                                display,
                                stage.get_buffer_format(),
                                MipmapsOption::EmptyMipmaps,
                                self.resolution.0 as u32,
                                self.resolution.1 as u32,
                            )
                            .context("Failed to create a rendering buffer")?,
                        ],
                        (self.resolution.0 as u32, self.resolution.1 as u32),
                    ),
                );

                stage.recreate_buffers = false;
            }

            stage.update(display, env_variable_list, beat)?;
        }

        Ok(())
    }

    pub fn render_stages(&mut self, display: &dyn Facade) -> Result<()> {
        let mut texture_with_mipmap_list: Vec<String> = Vec::new();
        for render_stage in &self.render_chain {
            for texture_sampling in render_stage.get_input_map().values() {
                if let InputSampler::Mipmaps(texture_name) = texture_sampling {
                    texture_with_mipmap_list.push(texture_name.to_owned());
                }
            }
        }
        for texture_sampling in self.final_stage.get_input_map().values() {
            if let InputSampler::Mipmaps(texture_name) = texture_sampling {
                texture_with_mipmap_list.push(texture_name.to_owned());
            }
        }

        for (stage_index, stage) in self.render_chain.iter().enumerate() {
            if let Some((render_target_pack, _)) = self.render_buffer_list.get(stage_index) {
                let render_target = &render_target_pack[1];

                self.render_stage(display, stage, RenderTarget::FrameBuffer(render_target))?;
            }

            if let Some((ref mut render_target_pack, _)) =
                self.render_buffer_list.get_mut(stage_index)
            {
                let tmp_buffer = render_target_pack.remove(0);
                render_target_pack.push(tmp_buffer);

                if texture_with_mipmap_list.contains(stage.get_name()) {
                    unsafe {
                        render_target_pack[0].generate_mipmaps();
                    }
                }
            }
        }

        Ok(())
    }

    pub fn render_final_stage(
        &mut self,
        display: &dyn Facade,
        window_frame: &mut Frame,
    ) -> Result<()> {
        self.render_stage(
            display,
            &self.final_stage,
            RenderTarget::Window(window_frame),
        )?;

        Ok(())
    }

    pub fn render_stage(
        &self,
        display: &dyn Facade,
        stage: &Stage,
        target: RenderTarget,
    ) -> Result<()> {
        let mut render_buffer_list = HashMap::new();
        let mut input_holder = HashMap::new();

        for (uniform_name, input_name) in stage.get_input_map() {
            let (input_name, down_sampling, up_sampling) = match input_name {
                InputSampler::Nearest(input_name) => (
                    input_name,
                    MinifySamplerFilter::Nearest,
                    MagnifySamplerFilter::Nearest,
                ),
                InputSampler::Linear(input_name) => (
                    input_name,
                    MinifySamplerFilter::Linear,
                    MagnifySamplerFilter::Linear,
                ),
                InputSampler::Mipmaps(input_name) => (
                    input_name,
                    MinifySamplerFilter::LinearMipmapLinear,
                    MagnifySamplerFilter::Linear,
                ),
            };

            let mut render_buffer_for_input = None;
            for (stage_index, stage) in self.render_chain.iter().enumerate() {
                if stage.get_name() == input_name {
                    render_buffer_for_input = Some(stage_index);
                }
            }

            if let Some(render_buffer_index) = render_buffer_for_input {
                if let Some(render_buffer_pack) = self.render_buffer_list.get(render_buffer_index) {
                    render_buffer_list.insert(
                        uniform_name,
                        (&render_buffer_pack.0[0], Some((down_sampling, up_sampling))),
                    );
                }
            } else if let Some(uniform_value) = self.uniform_holder.get(input_name) {
                input_holder.insert(
                    uniform_name,
                    (uniform_value, Some((down_sampling, up_sampling))),
                );
            }
        }

        for (uniform_name, uniform_value) in stage.get_uniform_list() {
            input_holder.insert(uniform_name, (uniform_value, None));
        }

        let filter_name = stage.get_filter();
        if let Some(filter) = self.filter_list.get(filter_name) {
            filter.render(
                display,
                &input_holder,
                &render_buffer_list,
                target,
                stage.get_filter_mode_params(),
            )?;
        }

        Ok(())
    }

    pub fn stage_index_list(&self) -> HashMap<String, usize> {
        self.render_chain
            .iter()
            .enumerate()
            .map(|(index, stage)| (stage.get_name().clone(), index))
            .collect()
    }

    pub fn get_dynamic_resolution(&self) -> bool {
        self.dynamic
    }
    pub fn set_dynamic_resolution(&mut self, dynamic_resolution: bool) {
        self.dynamic = dynamic_resolution;
    }

    pub fn get_resolution(&self) -> (usize, usize) {
        self.resolution
    }

    pub fn set_resolution(
        &mut self,
        display: &dyn Facade,
        resolution: (usize, usize),
    ) -> Result<()> {
        if resolution == self.resolution || !self.dynamic {
            return Ok(());
        }

        self.resolution = resolution;
        self.render_buffer_list.clear();

        for stage in self.render_chain.iter() {
            let new_render_buffer_pair = (
                vec![
                    Texture2d::empty_with_format(
                        display,
                        stage.get_buffer_format(),
                        MipmapsOption::EmptyMipmaps,
                        self.resolution.0 as u32,
                        self.resolution.1 as u32,
                    )
                    .context("Failed to create a rendering buffer")?,
                    Texture2d::empty_with_format(
                        display,
                        stage.get_buffer_format(),
                        MipmapsOption::EmptyMipmaps,
                        self.resolution.0 as u32,
                        self.resolution.1 as u32,
                    )
                    .context("Failed to create a rendering buffer")?,
                ],
                (self.resolution.0 as u32, self.resolution.1 as u32),
            );

            self.render_buffer_list.push(new_render_buffer_pair);
        }

        Ok(())
    }

    pub fn take_screenshot(&self, stage_name: &str) -> Option<Result<RGBAImageData>> {
        for (render_stage, (texture_list, _)) in
            self.render_chain.iter().zip(&self.render_buffer_list)
        {
            if render_stage.get_name() == stage_name {
                return Some(
                    texture_list[0]
                        .read_to_pixel_buffer()
                        .read_as_texture_2d()
                        .context("Could not read blit texture as a pixel buffer"),
                );
            }
        }
        None
    }
}
