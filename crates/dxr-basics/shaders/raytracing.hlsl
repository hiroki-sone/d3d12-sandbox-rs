#include "scene.hlsl"

// must match MAX_MESH_DATA_COUNT in raytracing.rs
#define MAX_MESH_DATA_COUNT 1

struct MeshData {
    uint index_buffer_id;
    uint color_buffer_id;
    uint2 reserved;
};

cbuffer ResourceHandles : register(b0) {
    MeshData mesh_data_list[MAX_MESH_DATA_COUNT];

    uint camera_id;
    uint output_id;
    uint raytracing_scene_id;
};

RayDesc generate_ray(Camera camera, uint2 id) {
    float2 uv = (id + 0.5) / float2(camera.viewport_size);
    float2 dst = uv * float2(2, -2) + float2(-1, 1);
    float4 ray_d = mul(camera.inv_view_projection, float4(dst, 1, 1));
    float4 ray_o = mul(camera.inv_view_projection, float4(0, 0, 0, 1));
    RayDesc ray;
    ray.Origin = ray_o.xyz / ray_o.w;
    ray.Direction = normalize(ray_d.xyz / ray_d.w - ray.Origin);
    ray.TMin = 1e-4;
    ray.TMax = 100;
    return ray;
}

float3 trace_ray(RaytracingAccelerationStructure raytracing_scene, RayDesc ray) {
    RayQuery<RAY_FLAG_CULL_NON_OPAQUE> query;
    query.TraceRayInline(raytracing_scene, RAY_FLAG_NONE, 0xFF, ray);

    // perform intersection tests
    query.Proceed();

    if (query.CommittedStatus() != COMMITTED_TRIANGLE_HIT) {
        return float3(0.4, 0.6, 0.9);
    }

    uint geometry_index = query.CommittedGeometryIndex();
    MeshData mesh_data = mesh_data_list[geometry_index];

    uint primitive_index = query.CommittedPrimitiveIndex();

    StructuredBuffer<uint> index_buffer = ResourceDescriptorHeap[mesh_data.index_buffer_id];
    uint index_buf_offset = 3 * primitive_index;
    uint3 indices = uint3(index_buffer[index_buf_offset], index_buffer[index_buf_offset+1], index_buffer[index_buf_offset+2]);

    StructuredBuffer<float3> color_buffer = ResourceDescriptorHeap[mesh_data.color_buffer_id];
    float3 vertex_colors[3] = {
        color_buffer[indices.x],
        color_buffer[indices.y],
        color_buffer[indices.z],
    };
    
    float2 barycentrics = query.CommittedTriangleBarycentrics();
    float3 weights = float3(1 - barycentrics.x - barycentrics.y, barycentrics.x, barycentrics.y);
    float3 color = vertex_colors[0] * weights.x + vertex_colors[1] * weights.y + vertex_colors[2] * weights.z;
    return color;
}

[numthreads(8, 8, 1)]
void main(uint3 dispatch_thread_id  : SV_DispatchThreadID) {
    ConstantBuffer<Camera> camera = ResourceDescriptorHeap[camera_id];
    
    if (any(dispatch_thread_id.xy >= camera.viewport_size)) {
        return;
    }
    
    RayDesc ray = generate_ray(camera, dispatch_thread_id.xy);
    
    RaytracingAccelerationStructure raytracing_scene = ResourceDescriptorHeap[raytracing_scene_id];
    float3 color = trace_ray(raytracing_scene, ray);

    RWTexture2D<float4> output = ResourceDescriptorHeap[output_id];
    output[dispatch_thread_id.xy] = float4(color, 1.0);
}