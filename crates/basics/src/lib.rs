pub mod d3d12;
pub mod framework;
pub mod math;
pub mod renderer;

pub struct Config {
    client_width: u32,
    client_height: u32,

    debug_layer_enabled: bool,
    gpu_validation_enabled: bool,
}

impl Config {
    pub fn client_width(&self) -> u32 {
        self.client_width
    }

    pub fn client_height(&self) -> u32 {
        self.client_height
    }

    pub fn debug_layer_enabled(&self) -> bool {
        self.debug_layer_enabled || self.gpu_validation_enabled
    }

    pub fn gpu_validation_enabled(&self) -> bool {
        self.gpu_validation_enabled
    }
}

pub fn parse_args(_args: impl Iterator<Item = String>) -> Config {
    Config {
        client_width: 1280,
        client_height: 720,
        debug_layer_enabled: true,
        gpu_validation_enabled: true,
    }
}
