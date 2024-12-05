#ifndef BRDF_HLSL
#define BRDF_HLSL

// Trowbridge-Reitz microfacet BRDF (a.k.a. GGX)
// https://www.cs.cornell.edu/~srm/publications/EGSR07-btdf.html
// https://jcgt.org/published/0003/02/03/

static const float PI = 3.1415926535897932384626433832795;

struct Material {
    float3 base_color;
    float metallic;
    float3 specular_reflectance;
    float roughness;
    float3 specular_tint;
    uint pad;
};

float sqr(float x) {
    return x * x;
}

float microfacet_distribution(float cos_theta, float alpha) {
    float alpha2 = sqr(alpha);
    float cos2_theta = sqr(cos_theta);
    float cos4_theta = sqr(cos2_theta);
    float tan2_theta = (1 / cos2_theta) - 1;
    return alpha2 / (PI * cos4_theta * sqr(alpha2 + tan2_theta));
}

float pow5(float x) {
    float x2 = x * x;
    return x2 * x2 * x;
}

float pow6(float x) {
    float x2 = x * x;
    return x2 * x2 * x;
}

float3 fresnel(float cos_theta, float3 r, float3 h) {
    //  Lazanyi-Schlick approximation with Naty Hoffman's reparametrization
    // https://doi.org/10.2312/mam.20191305
    // https://renderwonk.com/publications/mam2019/
    float3 a = (823543.0/46656.0) * (r - h) + (49.0/6.0) * (1 - r);
    float3 f = r + (1 - r) * pow5(1 - cos_theta)
        - a * cos_theta * pow6(1 - cos_theta);
    return saturate(f);
}

float lambda(float cos_theta, float alpha) {
    // Understanding the Masking-Shadowing Function in Microfacet-Based BRDFs [Heitz et al. 2014]
    // https://jcgt.org/published/0003/02/03/
    // Equation (72)
    float tan2_theta = (1 / sqr(cos_theta)) - 1;
    return (sqrt(1 + sqr(alpha) * tan2_theta) - 1) / 2;
}

float shadowing_factor(float cos_theta_i, float cos_theta_o, float alpha) {
    // Understanding the Masking-Shadowing Function in Microfacet-Based BRDFs [Heitz et al. 2014]
    // https://jcgt.org/published/0003/02/03/
    // Equation (99)
    return 1 / (lambda(cos_theta_o, alpha) + lambda(cos_theta_i, alpha) + 1);
}

float3 eval_specular(float3 incoming, float3 outgoing, float3 normal, Material material) {
    float cos_theta_i = saturate(dot(incoming, normal));
    if (cos_theta_i == 0) return 0;

    float cos_theta_o = saturate(dot(outgoing, normal));
    if (cos_theta_o == 0) return 0;

    float3 half_vector = normalize(incoming + outgoing);

    float cos_theta_m = saturate(dot(half_vector, normal));
    if (cos_theta_m == 0) return 0;

    float alpha = sqr(material.roughness);

    float3 f = fresnel(cos_theta_i, material.specular_reflectance, material.specular_tint);
    float g = shadowing_factor(cos_theta_i, cos_theta_o, alpha);
    float d = microfacet_distribution(cos_theta_m, alpha);

    return (f * g * d) / (4 * cos_theta_i * cos_theta_o);
}

float3 eval_diffuse(Material material) {
    return material.base_color / PI;
}

float3 eval_brdf(float3 incoming, float3 outgoing, float3 normal, Material material) {
    return material.metallic * eval_specular(incoming, outgoing, normal, material)
        + (1 - material.metallic) * eval_diffuse(material);
}

#endif // BRDF_HLSL
