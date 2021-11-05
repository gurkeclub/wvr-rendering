use std::collections::HashMap;
use std::convert::TryFrom;

use anyhow::Result;

use glium::backend::Facade;
use glium::texture::UncompressedFloatFormat;

use wvr_data::config::filter::FilterMode;
use wvr_data::config::rendering::RenderStageConfig;
use wvr_data::types::{Automation, BufferPrecision, DataHolder, InputSampler};

use crate::UniformHolder;

pub struct Stage {
    name: String,
    filter: String,
    filter_mode_params: FilterMode,
    pub input_map: HashMap<String, InputSampler>,
    pub variable_list: HashMap<String, (DataHolder, Automation, Option<(String, DataHolder)>)>,
    pub uniform_list: HashMap<String, UniformHolder>,
    pub buffer_format: UncompressedFloatFormat,

    pub recreate_buffers: bool,
}

impl Stage {
    pub fn from_config(
        name: &str,
        display: &dyn Facade,
        config: &RenderStageConfig,
    ) -> Result<Self> {
        let mut uniform_list = HashMap::new();
        for (key, (variable_value, _, _)) in config.variables.iter() {
            uniform_list.insert(
                key.clone(),
                UniformHolder::try_from((display, variable_value, false))?,
            );
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
            config.filter_mode_params,
            config.inputs.clone(),
            config.variables.clone(),
            uniform_list,
        ))
    }

    pub fn new(
        name: &str,
        buffer_format: UncompressedFloatFormat,
        filter: &str,
        filter_mode_params: FilterMode,
        input_map: HashMap<String, InputSampler>,
        variable_list: HashMap<String, (DataHolder, Automation, Option<(String, DataHolder)>)>,
        uniform_list: HashMap<String, UniformHolder>,
    ) -> Self {
        Self {
            name: name.to_string(),
            filter: filter.to_string(),
            filter_mode_params,
            input_map,
            variable_list,
            uniform_list,
            buffer_format,
            recreate_buffers: true,
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

    pub fn get_input_map(&self) -> &HashMap<String, InputSampler> {
        &self.input_map
    }

    pub fn get_uniform_list(&self) -> &HashMap<String, UniformHolder> {
        &self.uniform_list
    }

    pub fn get_buffer_format(&self) -> UncompressedFloatFormat {
        self.buffer_format
    }

    pub fn set_precision(&mut self, precision: &BufferPrecision) {
        let new_buffer_format = match precision {
            BufferPrecision::U8 => UncompressedFloatFormat::U8U8U8U8,
            BufferPrecision::F16 => UncompressedFloatFormat::F16F16F16F16,
            BufferPrecision::F32 => UncompressedFloatFormat::F32F32F32F32,
        };
        if new_buffer_format != self.buffer_format {
            self.buffer_format = new_buffer_format;

            self.recreate_buffers = true;
        }
    }

    pub fn set_variable_value(
        &mut self,
        display: &dyn Facade,
        variable_name: &str,
        variable_value: &DataHolder,
    ) -> Result<()> {
        if let Some((old_variable_value, _, _)) = self.variable_list.get_mut(variable_name) {
            *old_variable_value = variable_value.clone()
        } else {
            self.variable_list.insert(
                variable_name.to_string(),
                (variable_value.clone(), Automation::None, None),
            );
        }

        self.uniform_list.insert(
            variable_name.to_string(),
            UniformHolder::try_from((display, variable_value, false))?,
        );

        Ok(())
    }

    pub fn set_variable_offset(
        &mut self,
        variable_name: &str,
        offset: &Option<(String, DataHolder)>,
    ) -> Result<()> {
        if let Some((_, _, old_offset)) = self.variable_list.get_mut(variable_name) {
            *old_offset = offset.clone()
        }

        Ok(())
    }

    pub fn set_variable_automation(
        &mut self,
        variable_name: &str,
        variable_automation: &Automation,
    ) -> Result<()> {
        if let Some((_, old_variable_automation, _)) = self.variable_list.get_mut(variable_name) {
            *old_variable_automation = *variable_automation;
        }

        Ok(())
    }

    pub fn update(
        &mut self,
        display: &dyn Facade,
        env_variable_list: &HashMap<String, DataHolder>,
        beat: f64,
    ) -> Result<()> {
        for (variable_name, (variable_value, automation, offset)) in &self.variable_list {
            let mut variable_changed = false;

            let mut variable_value = variable_value.clone();

            if let Some((offset_reference_name, offset_weight)) = offset {
                if let Some(reference_value) = env_variable_list.get(offset_reference_name) {
                    variable_value = &variable_value + &(offset_weight * reference_value);
                    variable_changed = true;
                }
            }

            if let Some(new_variable_value) = automation.apply(&variable_value, beat) {
                variable_value = new_variable_value;
                variable_changed = true;
            }
            if variable_changed {
                let new_uniform_value = UniformHolder::try_from((display, &variable_value, false))?;

                if let Some(old_automation_value) = self.uniform_list.get_mut(variable_name) {
                    *old_automation_value = new_uniform_value;
                } else {
                    self.uniform_list
                        .insert(variable_name.to_string(), new_uniform_value);
                }
            }
        }

        Ok(())
    }

    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }
    pub fn set_input(&mut self, input_name: &str, input: &InputSampler) {
        self.input_map.insert(input_name.to_string(), input.clone());
    }

    pub fn set_filter(&mut self, filter_name: &str) {
        self.filter = filter_name.to_string();
    }

    pub fn set_filter_mode_params(&mut self, filter_mode_params: &FilterMode) {
        self.filter_mode_params = *filter_mode_params;
    }
}
