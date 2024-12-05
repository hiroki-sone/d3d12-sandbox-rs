# Direct3D 12 Sandbox

Small Direct3D 12 programs written in Rust

## [`basics` crate](./crates/basics/)

Renders a rotating cube.  
The implementation is based on [the article of 3D Game Engine Programming](https://www.3dgep.com/learning-directx-12-2/)


## [`dxr-basics` crate](./crates/dxr-basics/)

Renders a rotating cube, which is same as `basics` crate.  
There are two rendering modes; Rastarization and Raytracing.  
The mode can be toggled with Space key, but the visual results should be the same.

This crate uses DirectX Raytracing feature as well as bindless resources.
