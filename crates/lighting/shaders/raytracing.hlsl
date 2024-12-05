#include "scene.hlsl"
#include "light.hlsl"
#include "brdf.hlsl"

// must match MAX_MESH_DATA_COUNT in raytracing.rs
#define MAX_MESH_DATA_COUNT 2

struct MeshData {
    uint index_buffer_id;
    uint position_buffer_id;
    uint normal_buffer_id;
    uint1 reserved;
};

cbuffer ResourceHandles : register(b0) {
    MeshData mesh_data_list[MAX_MESH_DATA_COUNT];

    SpotLight light;

    uint camera_id;
    uint output_id;
    uint raytracing_scene_id;
    uint transform_buffer_id;

    uint material_buffer_id;
    uint3 pad;
};

RayDesc generate_primary_ray(Camera camera, uint2 id) {
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

struct HitPoint {
    float3 position;
    float3 normal;

    Material material;
};

bool trace_ray(RaytracingAccelerationStructure raytracing_scene, RayDesc ray, out HitPoint hitpoint) {
    RayQuery<RAY_FLAG_CULL_NON_OPAQUE> query;
    query.TraceRayInline(raytracing_scene, RAY_FLAG_NONE, 0xFF, ray);

    // perform intersection tests
    query.Proceed();

    if (query.CommittedStatus() == COMMITTED_NOTHING) {
        hitpoint = (HitPoint)0.0;
        return false;
    }

    uint geometry_index = query.CommittedGeometryIndex();
    MeshData mesh_data = mesh_data_list[geometry_index];

    uint primitive_index = query.CommittedPrimitiveIndex();

    StructuredBuffer<uint> index_buffer = ResourceDescriptorHeap[mesh_data.index_buffer_id];
    uint index_buf_offset = 3 * primitive_index;
    uint3 indices = uint3(index_buffer[index_buf_offset], index_buffer[index_buf_offset+1], index_buffer[index_buf_offset+2]);
    
    hitpoint.position = ray.Origin + query.CommittedRayT() * ray.Direction;

    float2 barycentrics = query.CommittedTriangleBarycentrics();
    float3 weights = float3(1 - barycentrics.x - barycentrics.y, barycentrics.x, barycentrics.y);

    StructuredBuffer<float3> normal_buffer = ResourceDescriptorHeap[mesh_data.normal_buffer_id];
    float3 vertex_normals[3] = {
        normal_buffer[indices.x],
        normal_buffer[indices.y],
        normal_buffer[indices.z],
    };
    float3 local_normal = normalize(vertex_normals[0] * weights.x + vertex_normals[1] * weights.y + vertex_normals[2] * weights.z);

    StructuredBuffer<float4x3> transform_buffer = ResourceDescriptorHeap[transform_buffer_id];
    float3x4 world_to_local = transpose(transform_buffer[2 * geometry_index + 1]);
    hitpoint.normal = mul(world_to_local, float4(local_normal, 0));

    StructuredBuffer<Material> material_buffer = ResourceDescriptorHeap[material_buffer_id];
    hitpoint.material = material_buffer[geometry_index];

    return true;
}

bool tracec_shadow_ray(RaytracingAccelerationStructure raytracing_scene, RayDesc ray) {
    RayQuery<RAY_FLAG_CULL_NON_OPAQUE> query;
    query.TraceRayInline(raytracing_scene, RAY_FLAG_ACCEPT_FIRST_HIT_AND_END_SEARCH, 0xFF, ray);

    // perform intersection tests
    query.Proceed();

    return query.CommittedStatus() != COMMITTED_NOTHING;
}

[numthreads(8, 8, 1)]
void main(uint3 dispatch_thread_id  : SV_DispatchThreadID) {
    ConstantBuffer<Camera> camera = ResourceDescriptorHeap[camera_id];
    
    if (any(dispatch_thread_id.xy >= camera.viewport_size)) {
        return;
    }
    
    RayDesc ray = generate_primary_ray(camera, dispatch_thread_id.xy);
    
    RaytracingAccelerationStructure raytracing_scene = ResourceDescriptorHeap[raytracing_scene_id];
    HitPoint hitpoint = (HitPoint)0;
    
    bool intersected = trace_ray(raytracing_scene, ray, hitpoint);

    float3 contribution = 0;
    if (intersected) {
        float3 light_dir = light.position - hitpoint.position;
        float distance_to_light = length(light_dir);
        light_dir /= distance_to_light;

        float3 camera_dir = normalize(camera.position - hitpoint.position);

        float3 incoming_radiance = eval_spot_light(light, hitpoint.position);
        float3 brdf = eval_brdf(light_dir, camera_dir, hitpoint.normal, hitpoint.material);
        contribution += incoming_radiance * brdf * dot(hitpoint.normal, light_dir);

        if (any(contribution > 0.0)) {
            RayDesc shadow_ray;
            shadow_ray.Origin = hitpoint.position;
            shadow_ray.Direction = light_dir;
            shadow_ray.TMin = 1e-3;
            shadow_ray.TMax = distance_to_light;

            bool occluded = tracec_shadow_ray(raytracing_scene, shadow_ray);
            if (occluded) {
                contribution = 0;
            }
        }
        
    } else {
        contribution = float3(0.4, 0.6, 0.9);;
    }

    RWTexture2D<float4> output = ResourceDescriptorHeap[output_id];
    output[dispatch_thread_id.xy] = float4(contribution, 1.0);
}