#ifndef SCENE_HLSL
#define SCENE_HLSL

struct Camera {
    float4x4 view_projection;
    float4x4 inv_view_projection;

    float3 position;
    uint pad;
    
    uint2 viewport_size;
};

#endif // SCENE_HLSL
