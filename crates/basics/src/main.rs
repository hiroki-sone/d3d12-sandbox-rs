use basics::*;

fn main() -> windows::core::Result<()> {
    // change cwd so opening HLSL files will not fail
    let dir = std::env::current_exe()
        .inspect_err(|e| println!("Failed to get the path of this program: {e}"))
        .ok()
        .map(|path| path.parent().unwrap().to_path_buf());
    if let Some(dir) = dir {
        if let Err(e) = std::env::set_current_dir(dir) {
            println!("Failed to change the current working directory: {e}");
        }
    }

    let config = crate::parse_args(std::env::args());
    framework::run(&config)?;
    d3d12::device::report_live_objects()?;
    Ok(())
}
