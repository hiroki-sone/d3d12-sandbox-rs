cbuffer ResourceHandles : register(b0) {
    uint transform_buffer_id;
    uint mesh_id;
};

cbuffer LightTransform : register(b1) {
    float4x4 world_to_light;
};

struct Vertex {
    float3 position: POSITION;
};

struct VertexShaderOutput {
    float4 sv_position: SV_Position;
};

VertexShaderOutput vs_main(Vertex v) {
    // the model transform matrix (for DXR) is row-major 3x4 matrix, but the memory layout is column-based
    // so it needs to be transposed
    //TODO: make it consistent
    StructuredBuffer<float4x3> transform_buffer = ResourceDescriptorHeap[transform_buffer_id];
    float3x4 model_transform = transpose(transform_buffer[mesh_id]);
    float3 world_position = mul(model_transform, float4(v.position, 1));

    VertexShaderOutput output;
    output.sv_position = mul(world_to_light, float4(world_position, 1));
    
    return output;
}
