#ifndef LIGHT_HLSL
#define LIGHT_HLSL

struct SpotLight {
    float3 position;
    float intensity;
    float3 direction;
    float cos_half_angle;
};

float3 eval_spot_light(SpotLight spot_light, float3 shaded_point) {
    float3 to_light = spot_light.position - shaded_point;
    float distance2 = dot(to_light, to_light);
    float distance_to_light = sqrt(distance2);
    float3 light_dir = to_light / distance_to_light;

    return (dot(-light_dir, spot_light.direction) > spot_light.cos_half_angle) ? (spot_light.intensity / distance2) : 0.0;
}

#endif // LIGHT_HLSL
