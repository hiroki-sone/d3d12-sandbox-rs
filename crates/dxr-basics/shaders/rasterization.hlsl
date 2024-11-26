#include "scene.hlsl"

cbuffer ResourceHandles : register(b0) {
    uint camera_id;
    uint transform_buffer_id;
};

struct Vertex {
    float3 position: POSITION;
    float3 color: COLOR;
};

struct VertexShaderOutput {
    float4 sv_position: SV_Position;
    float4 color: COLOR;
};

VertexShaderOutput vs_main(Vertex v) {
    // the model transform matrix (for DXR) is row-major 3x4 matrix, but the memory layout is column-based
    // so it needs to be transposed
    //TODO: make it consistent
    StructuredBuffer<float4x3> transform_buffer = ResourceDescriptorHeap[transform_buffer_id];
    float3x4 model_transform = transpose(transform_buffer[0]);
    float3 world_position = mul(model_transform, float4(v.position, 1));

    ConstantBuffer<Camera> camera = ResourceDescriptorHeap[camera_id];
    VertexShaderOutput output;
    output.sv_position = mul(camera.view_projection, float4(world_position, 1));
    
    output.color = float4(v.color, 1);
    
    return output;
}

float4 ps_main(VertexShaderOutput input) : SV_Target {
    return input.color;
}
