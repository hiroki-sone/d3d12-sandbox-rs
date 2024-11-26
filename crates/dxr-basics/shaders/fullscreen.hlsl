#include "scene.hlsl"

cbuffer ResourceHandles : register(b0) {
    uint camera_id;
    uint src_texture_id;
};

struct ScreenParams {
    float4 sv_position: SV_POSITION;
    float2 uv: TEXCOORD;
};

ScreenParams vs_main(uint id : SV_VertexID) {
    // https://wallisc.github.io/rendering/2021/04/18/Fullscreen-Pass.html
    ScreenParams output;
    output.uv = float2((id << 1) & 2, id & 2);
    output.sv_position = float4(output.uv * float2(2, -2) + float2(-1, 1), 0, 1);
	return output;
}

float4 copy_ps(ScreenParams input) : SV_Target {
    ConstantBuffer<Camera> camera = ResourceDescriptorHeap[camera_id];
    Texture2D<float4> src_texture = ResourceDescriptorHeap[src_texture_id];

    uint2 index = uint2(float2(camera.viewport_size) * input.uv);
    return src_texture[index];
}
