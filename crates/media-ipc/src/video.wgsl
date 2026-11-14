struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
struct Uniforms { opacity: f32, srgb: f32, aspect_scale: vec2<f32> }
@group(0) @binding(0) var frame: texture_2d<f32>;
@group(0) @binding(1) var filtering: sampler;
@group(0) @binding(2) var<uniform> uniforms: Uniforms;
@vertex fn vertex(@builtin(vertex_index) index: u32) -> Vertex {
    let positions=array<vec2<f32>,6>(vec2(-1.,1.),vec2(1.,1.),vec2(-1.,-1.),vec2(1.,1.),vec2(1.,-1.),vec2(-1.,-1.));
    let p=positions[index];
    return Vertex(vec4(p*uniforms.aspect_scale,0.,1.),vec2((p.x+1.)*.5,(1.-p.y)*.5));
}
@fragment fn fragment(input:Vertex) -> @location(0) vec4<f32> {
    var rgb=textureSample(frame,filtering,input.uv).rgb;
    if uniforms.srgb>0.5 {rgb=select(pow((rgb+0.055)/1.055,vec3(2.4)),rgb/12.92,rgb<=vec3(0.04045));}
    return vec4(rgb*uniforms.opacity,uniforms.opacity);
}
