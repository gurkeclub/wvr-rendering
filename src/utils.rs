use anyhow::{Context, Result};

use glium::glutin;
use glium::glutin::event_loop::EventLoop;
use glium::Display;

use glutin::dpi::PhysicalSize;
use glutin::window::WindowBuilder;
use glutin::ContextBuilder;

use wvr_data::config::project_config::ViewConfig;

pub fn build_window(view_config: &ViewConfig, events_loop: &EventLoop<()>) -> Result<Display> {
    let context = ContextBuilder::new()
        .with_vsync(view_config.vsync)
        .with_srgb(true);
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

    Display::new(window, context, &events_loop).context("Failed to create the rendering window")
}
