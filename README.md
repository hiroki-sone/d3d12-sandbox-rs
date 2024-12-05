# Direct3D 12 Sandbox

Small Direct3D 12 programs written in Rust

## [`basics` crate](./crates/basics/)

Renders a rotating cube.  
The implementation is based on [articles of 3D Game Engine Programming](https://www.3dgep.com/learning-directx-12-2/)


## [`dxr-basics` crate](./crates/dxr-basics/)

Renders a rotating cube, which is same as `basics` crate.  
There are two rendering modes; Rastarization and Raytracing.  
The mode can be toggled with Space key, but the visual results should be the same.

This crate uses DirectX Raytracing feature as well as bindless resources.


## [`lighting` crate](./crates/dxr-basics/)

Implements simple ligting including
* Shadows, both raytracing and shadow mapping
* A Physically-based light (i.e., emitted light is attenuated with the inverse-square law)
* Specular reflection using microfacet model
  * [Trowbridge-Reitz microfacet BRDF (a.k.a. GGX)](https://www.cs.cornell.edu/~srm/publications/EGSR07-btdf.html)
  * [Fresnel reflectance using Lazanyi-Schlick approximation with Naty Hoffman's reparametrization](https://renderwonk.com/publications/mam2019/)
* Lambertian diffuse reflection
