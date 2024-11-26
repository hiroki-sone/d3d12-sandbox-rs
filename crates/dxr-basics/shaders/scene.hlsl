#ifndef SCENE_HLSL
#define SCENE_HLSL

struct Camera {
    float4x4 view_projection;
    float4x4 inv_view_projection;
    
    uint2 viewport_size;
};

#endif // SCENE_HLSL
