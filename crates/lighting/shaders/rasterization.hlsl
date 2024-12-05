#include "scene.hlsl"
#include "light.hlsl"
#include "brdf.hlsl"

cbuffer ResourceHandles : register(b0) {
    uint camera_id;
    uint transform_buffer_id;
    uint mesh_id;
    uint shadow_map_id;

    SpotLight light;
    float4x4 light_transform;
    
    float shadow_offset;
    float shadow_bias;
    uint2 pad;

    Material material;
};

SamplerComparisonState shadow_map_sampler : register(s0);

struct Vertex {
    float3 position: POSITION;
    float3 normal: NORMAL;
};

struct VertexShaderOutput {
    float3 position: POSITION;
    float4 sv_position: SV_Position;
    float3 normal: NORMAL;
};

VertexShaderOutput vs_main(Vertex v) {
    // the model transform matrix (for DXR) is row-major 3x4 matrix, but the memory layout is column-based
    // so it needs to be transposed
    //TODO: make it consistent
    StructuredBuffer<float4x3> transform_buffer = ResourceDescriptorHeap[transform_buffer_id];
    float3x4 local_to_world = transpose(transform_buffer[2 * mesh_id]);
    float3 world_position = mul(local_to_world, float4(v.position, 1));

    ConstantBuffer<Camera> camera = ResourceDescriptorHeap[camera_id];
    
    VertexShaderOutput output;
    output.position = world_position;
    output.sv_position = mul(camera.view_projection, float4(world_position, 1));
    
    float3x4 world_to_local = transpose(transform_buffer[2 * mesh_id + 1]);
    output.normal = mul(world_to_local, float4(v.normal, 0)).xyz;
    
    return output;
}

float eval_shadow(float3 position, Texture2D<float> shadow_map) {
    float4 p = mul(light_transform, float4(position, 1));
    float3 shadow_map_coords = p.xyz / p.w;
    if (any(shadow_map_coords.xy < 0) || any(shadow_map_coords.xy >= 1)) {
        return 0;
    }

    float shadow = shadow_map.SampleCmp(shadow_map_sampler, shadow_map_coords.xy, shadow_map_coords.z - shadow_bias) > 0 ? 1 : 0;

    shadow_map_coords.xy -= shadow_offset;
    shadow += shadow_map.SampleCmp(shadow_map_sampler, shadow_map_coords.xy, shadow_map_coords.z - shadow_bias) > 0 ? 1 : 0;

    shadow_map_coords.x += 2.0 * shadow_offset;
    shadow += shadow_map.SampleCmp(shadow_map_sampler, shadow_map_coords.xy, shadow_map_coords.z - shadow_bias) > 0 ? 1 : 0;

    shadow_map_coords.y += 2.0 * shadow_offset;
    shadow += shadow_map.SampleCmp(shadow_map_sampler, shadow_map_coords.xy, shadow_map_coords.z - shadow_bias) > 0 ? 1 : 0;

    shadow_map_coords.x -= 2.0 * shadow_offset;
    shadow += shadow_map.SampleCmp(shadow_map_sampler, shadow_map_coords.xy, shadow_map_coords.z - shadow_bias) > 0 ? 1 : 0;

    return shadow * 0.2;
}

float4 ps_main(VertexShaderOutput input) : SV_Target {
    Texture2D<float> shadow_map = ResourceDescriptorHeap[shadow_map_id];

    float3 normal = normalize(input.normal);

    float3 albedo = 0.5;
    float3 light_dir = normalize(light.position - input.position);
    float3 incoming_radiance = eval_spot_light(light, input.position) * eval_shadow(input.position, shadow_map);

    ConstantBuffer<Camera> camera = ResourceDescriptorHeap[camera_id];
    float3 camera_dir = normalize(camera.position - input.position);

    float3 brdf = eval_brdf(light_dir, camera_dir, normal, material);
    float3 contribution = incoming_radiance * brdf * dot(normal, light_dir);

    return float4(contribution, 1);
}
