use std::path;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Graphics::Direct3D::Dxc::*;

pub struct ShaderConfig {
    pub path: path::PathBuf,
    pub entry_point: String,
    pub target: String,
}

pub struct ShaderCompiler {
    compiler: IDxcCompiler3,
    utils: IDxcUtils,
    include_handler: IDxcIncludeHandler,
    debug: bool,
}

impl ShaderCompiler {
    pub fn build(debug: bool) -> windows::core::Result<Self> {
        let utils: IDxcUtils = unsafe { DxcCreateInstance(&CLSID_DxcUtils) }?;
        let compiler = unsafe { DxcCreateInstance(&CLSID_DxcCompiler) }?;

        let include_handler = unsafe { utils.CreateDefaultIncludeHandler() }?;

        Ok(Self {
            compiler,
            utils,
            include_handler,
            debug,
        })
    }

    pub fn compile_file(&self, config: &ShaderConfig) -> windows::core::Result<IDxcBlob> {
        println!(
            "Compiling {} {}",
            config.path.to_str().unwrap(),
            config.entry_point
        );

        let filename: HSTRING = config.path.as_os_str().into();
        let file = unsafe { self.utils.LoadFile(PCWSTR(filename.as_ptr()), None) }?;

        let source = DxcBuffer {
            Ptr: unsafe { file.GetBufferPointer() },
            Size: unsafe { file.GetBufferSize() },
            Encoding: DXC_CP_ACP.0,
        };

        let entry: HSTRING = config.entry_point.as_str().into();
        let target: HSTRING = config.target.as_str().into();

        let mut args = vec![
            PCWSTR(filename.as_ptr()),
            w!("-E"),
            PCWSTR(entry.as_ptr()),
            w!("-T"),
            PCWSTR(target.as_ptr()),
            w!("-HV"),
            w!("2021"),
        ];
        if self.debug {
            args.append(&mut vec![w!("-Zi"), w!("-Qembed_debug"), w!("-Od")]);
        }

        let result: IDxcResult = unsafe {
            self.compiler
                .Compile(&source, Some(&args), &self.include_handler)
        }?;

        let mut errors: Option<IDxcBlobUtf8> = None;
        unsafe { result.GetOutput(DXC_OUT_ERRORS, &mut None, &mut errors) }?;
        if let Some(error) = errors {
            if unsafe { error.GetStringLength() } != 0 {
                let message = unsafe { error.GetStringPointer().to_string() }.unwrap();
                eprintln!("Warnings and Errors: {message}");
            }
        }

        let status = unsafe { result.GetStatus() }?;
        if status.is_err() {
            let message = format!(
                "Failed to compile {}: {status}",
                config.path.to_str().unwrap()
            );
            return Err(windows::core::Error::new(status, message));
        }

        let mut output = None;
        let mut shader_name = None;
        unsafe { result.GetOutput(DXC_OUT_OBJECT, &mut shader_name, &mut output) }?;
        let Some(output) = output else {
            let message = format!(
                "Failed to get compiled shader of {}",
                config.path.to_str().unwrap()
            );
            return Err(windows::core::Error::new(E_FAIL, message));
        };

        Ok(output)
    }
}
