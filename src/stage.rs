use std::collections::HashMap;
use std::convert::TryFrom;

use anyhow::Result;

use glium::backend::Facade;
use glium::texture::UncompressedFloatFormat;

use wvr_data::config::project_config::{
    BufferPrecision, FilterMode, RenderStageConfig, SampledInput,
};
use wvr_data::DataHolder;

use crate::UniformHolder;

pub struct Stage {
    name: String,
    filter: String,
    filter_mode_params: FilterMode,
    pub input_map: HashMap<String, SampledInput>,
    pub variable_list: HashMap<String, UniformHolder>,
    pub buffer_format: UncompressedFloatFormat,
}

impl Stage {
    pub fn from_config(
        name: &str,
        display: &dyn Facade,
        config: &RenderStageConfig,
    ) -> Result<Self> {
        let mut variable_list = HashMap::new();
        for (key, value) in config.variables.iter() {
            variable_list.insert(key.clone(), UniformHolder::try_from((display, value))?);
        }

        let buffer_format = match &config.precision {
            BufferPrecision::U8 => UncompressedFloatFormat::U8U8U8U8,
            BufferPrecision::F16 => UncompressedFloatFormat::F16F16F16F16,
            BufferPrecision::F32 => UncompressedFloatFormat::F32F32F32F32,
        };

        Ok(Self::new(
            name,
            buffer_format,
            &config.filter,
            config.filter_mode_params.clone(),
            config.inputs.clone(),
            variable_list,
        ))
    }

    pub fn new(
        name: &str,
        buffer_format: UncompressedFloatFormat,
        filter: &str,
        filter_mode_params: FilterMode,
        input_map: HashMap<String, SampledInput>,
        variable_list: HashMap<String, UniformHolder>,
    ) -> Self {
        Self {
            name: name.to_string(),
            filter: filter.to_string(),
            filter_mode_params,
            input_map,
            variable_list,
            buffer_format,
        }
    }

    pub fn get_name(&self) -> &String {
        &self.name
    }

    pub fn get_filter(&self) -> &String {
        &self.filter
    }

    pub fn get_filter_mode_params(&self) -> &FilterMode {
        &self.filter_mode_params
    }

    pub fn get_input_map(&self) -> &HashMap<String, SampledInput> {
        &self.input_map
    }

    pub fn get_variable_list(&self) -> &HashMap<String, UniformHolder> {
        &self.variable_list
    }

    pub fn get_buffer_format(&self) -> UncompressedFloatFormat {
        self.buffer_format
    }

    pub fn set_variable(
        &mut self,
        display: &dyn Facade,
        variable_name: &str,
        variable_value: &DataHolder,
    ) -> Result<()> {
        self.variable_list.insert(
            variable_name.to_string(),
            UniformHolder::try_from((display, variable_value))?,
        );

        Ok(())
    }
    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }
    pub fn set_input(&mut self, input_name: &str, input: &SampledInput) {
        self.input_map.insert(input_name.to_string(), input.clone());
    }

    pub fn set_filter(&mut self, filter_name: &str) {
        self.filter = filter_name.to_string();
    }

    pub fn set_filter_mode_params(&mut self, filter_mode_params: &FilterMode) {
        self.filter_mode_params = filter_mode_params.clone();
    }
}
