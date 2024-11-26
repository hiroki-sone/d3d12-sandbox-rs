cbuffer Transform : register(b0) {
    float4x4 model_view_projection;
};

struct Vertex {
    float3 position: POSITION;
    float3 color: COLOR;
};

struct VertexShaderOutput {
    float4 color: COLOR;
    float4 sv_position: SV_Position;
};

VertexShaderOutput vs_main(Vertex v) {
    VertexShaderOutput output;
    output.sv_position = mul(model_view_projection, float4(v.position, 1));
    output.color = float4(v.color, 1);
    
    return output;
}

float4 ps_main(VertexShaderOutput input) : SV_Target {
    return input.color;
}
