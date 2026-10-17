struct Vertex { position: vec4<f32>, uv: vec4<f32>, color: vec4<f32>, lighting: vec4<f32>, normal:vec4<f32>, view:vec4<f32>, specular:vec4<f32>, world:vec4<f32> }
struct Triangle { vertices: array<Vertex,3>, info: vec4<u32>, fog_color: vec4<f32>, fog_parameters: vec4<f32>, ambient: vec4<f32>, light_info:vec4<u32> }
struct Geometry { size: vec4<u32>, viewport: vec4<u32>, clear: vec4<u32> }
@group(0) @binding(0) var<storage, read> triangles: array<Triangle>;
@group(0) @binding(1) var<storage, read> textures: array<u32>;
@group(0) @binding(2) var<storage, read_write> pixels: array<u32>;
@group(0) @binding(3) var<uniform> geometry: Geometry;
struct Directional { direction:vec4<f32>, color:vec4<f32> }
@group(0) @binding(4) var<storage,read> lights:array<Directional>;
fn unit(v:vec3<f32>) -> vec4<f32> {
    let length_squared=dot(v,v);
    if length_squared > 0. && length_squared <= 3.402823e38 { return vec4(v/sqrt(length_squared),1.); }
    return vec4(0.);
}
fn shade(v:Vertex, diffuse:vec3<f32>, t:Triangle) -> vec3<f32> {
    var factor=vec3(0.); var specular=vec3(0.);
    if t.ambient.w != 0. { factor=t.ambient.rgb*v.lighting.x; }
    for(var i=0u;i<t.light_info.y;i++) {
        let light=lights[t.light_info.x+i];
        var intensity=1.;
        if v.normal.w != 0. && light.direction.w != 0. {
            intensity=max(-dot(v.normal.xyz,light.direction.xyz),0.);
            if t.light_info.w != 0u && v.lighting.y != 0. && intensity > 0. && v.specular.w >= 0. && v.view.w != 0. {
                let view=unit(v.view.xyz);
                let reflection=unit(fma(vec3(2.*intensity),v.normal.xyz,light.direction.xyz));
                if view.w != 0. && reflection.w != 0. {
                    let cosine=max(dot(reflection.xyz,view.xyz),0.);
                    var power=1.;
                    if v.specular.w != 0. { power=pow(cosine,v.specular.w); }
                    if power > 0. && power <= 3.402823e38 { specular+=light.color.rgb*light.color.w*v.specular.rgb*power; }
                }
            }
        }
        factor+=light.color.rgb*light.color.w*intensity;
    }
    return fma(diffuse,factor,specular);
}
fn interpolate_shading(a:Vertex,b:Vertex,c:Vertex,w:vec3<f32>) -> Vertex {
    var v=a;
    v.lighting=a.lighting+(b.lighting-a.lighting)*w.y+(c.lighting-a.lighting)*w.z;
    v.lighting.y=a.lighting.y*b.lighting.y*c.lighting.y;
    v.specular=a.specular+(b.specular-a.specular)*w.y+(c.specular-a.specular)*w.z;
    v.world=a.world+(b.world-a.world)*w.y+(c.world-a.world)*w.z;
    v.normal=vec4(0.); v.view=vec4(0.);
    if a.normal.w*b.normal.w*c.normal.w != 0. { v.normal=unit(a.normal.xyz+(b.normal.xyz-a.normal.xyz)*w.y+(c.normal.xyz-a.normal.xyz)*w.z); }
    if a.view.w*b.view.w*c.view.w != 0. { v.view=unit(a.view.xyz+(b.view.xyz-a.view.xyz)*w.y+(c.view.xyz-a.view.xyz)*w.z); }
    return v;
}
fn edge(a: vec2<f32>, b: vec2<f32>, p: vec2<f32>) -> f32 {
    let d = p-a; let e = b-a;
    return d.x*e.y-d.y*e.x;
}
fn rgb555(value: vec3<f32>) -> u32 {
    let c = vec3<u32>(floor(clamp(value,vec3(0.),vec3(1.))*31.+vec3(0.5)));
    return (c.r<<10u)|(c.g<<5u)|c.b;
}
fn unpack555(value: u32) -> vec3<f32> {
    return vec3<f32>((vec3(value)>>vec3(10u,5u,0u))&vec3(31u))/31.;
}
@compute @workgroup_size(8,8) fn raster(@builtin(global_invocation_id) id: vec3<u32>) {
    let xy = id.xy;
    if any(xy >= geometry.size.xy) { return; }
    let index = xy.y*geometry.size.x+xy.x;
    var pixel = pixels[index];
    if any(xy < geometry.viewport.xy) || any(xy > geometry.viewport.zw) { return; }
    if geometry.clear.x != 0u { pixel = geometry.clear.y; }
    var depth = 3.402823e38;
    let point = vec2<f32>(xy);
    for(var i=0u; i<geometry.size.z; i++) {
        let triangle = triangles[i];
        let a = triangle.vertices[0]; let b = triangle.vertices[1]; let c = triangle.vertices[2];
        let area = edge(a.position.xy,b.position.xy,c.position.xy);
        let weights = vec3(edge(b.position.xy,c.position.xy,point),edge(c.position.xy,a.position.xy,point),edge(a.position.xy,b.position.xy,point));
        if (area > 0. && any(weights < vec3(0.))) || (area < 0. && any(weights > vec3(0.))) { continue; }
        let affine = weights/area;
        let z = a.position.z+(b.position.z-a.position.z)*affine.y+(c.position.z-a.position.z)*affine.z;
        if z > depth { continue; }
        if (triangle.info.w & 1u) != 0u { depth = z; }
        var perspective = affine*vec3(a.position.w,b.position.w,c.position.w);
        perspective /= dot(perspective,vec3(1.));
        var ca=a.color; var cb=b.color; var cc=c.color;
        let directional=triangle.light_info.y != 0u;
        let vertex_lighting=directional && triangle.light_info.z != 2u;
        if vertex_lighting {
            ca=vec4(shade(a,ca.rgb,triangle),ca.a);
            cb=vec4(shade(b,cb.rgb,triangle),cb.a);
            cc=vec4(shade(c,cc.rgb,triangle),cc.a);
        } else if !directional && triangle.ambient.w == 1. {
            ca=vec4(ca.rgb*triangle.ambient.rgb*a.lighting.x,ca.a);
            cb=vec4(cb.rgb*triangle.ambient.rgb*b.lighting.x,cb.a);
            cc=vec4(cc.rgb*triangle.ambient.rgb*c.lighting.x,cc.a);
        }
        var color = ca+(cb-ca)*perspective.y+(cc-ca)*perspective.z;
        if triangle.info.y != 0u {
            var uv = a.uv.xy+(b.uv.xy-a.uv.xy)*perspective.y+(c.uv.xy-a.uv.xy)*perspective.z;
            uv = select(clamp(uv,vec2(0.),vec2(1.)), uv-floor(uv), vec2((triangle.info.w&2u)!=0u,(triangle.info.w&4u)!=0u));
            let size = triangle.info.yz;
            let texel = min(vec2<u32>(vec2(uv.x,1.-uv.y)*vec2<f32>(size)),size-vec2(1u));
            let rgba = textures[triangle.info.x+texel.y*size.x+texel.x];
            color *= vec4<f32>((vec4(rgba)>>vec4(0u,8u,16u,24u))&vec4(255u))/255.;
        }
        if directional && !vertex_lighting {
            color=vec4(shade(interpolate_shading(a,b,c,perspective),color.rgb,triangle),color.a);
        } else if !directional && triangle.ambient.w == 2. {
            let coefficient=a.lighting.x+(b.lighting.x-a.lighting.x)*perspective.y+(c.lighting.x-a.lighting.x)*perspective.z;
            color=vec4(color.rgb*triangle.ambient.rgb*coefficient,color.a);
        }
        let fog_mode = u32(triangle.fog_color.w);
        if fog_mode != 0u {
            let fog_depth = a.uv.z+(b.uv.z-a.uv.z)*perspective.y+(c.uv.z-a.uv.z)*perspective.z;
            let parameters = triangle.fog_parameters;
            var factor = 1.;
            if fog_mode == 1u {
                let range = parameters.y-parameters.x;
                if range == 0. { factor = select(0.,1.,fog_depth < parameters.y); }
                else { factor = (parameters.y-fog_depth)/range; }
            } else if fog_mode == 2u { factor = exp(-parameters.z*fog_depth); }
            else if fog_mode == 3u { let value=parameters.z*fog_depth; factor=exp(-value*value); }
            else if fog_mode == 4u && all(vec3(a.uv.w,b.uv.w,c.uv.w)>=vec3(0.)) {
                factor = a.uv.w+(b.uv.w-a.uv.w)*perspective.y+(c.uv.w-a.uv.w)*perspective.z;
            }
            // Overflowing exponential factors mean no fog, matching the CPU.
            if abs(factor) <= 3.402823e38 {
                factor = clamp(factor,0.,1.);
                color = vec4(fma(color.rgb,vec3(factor),triangle.fog_color.rgb*(1.-factor)),color.a);
            }
        }
        let alpha = clamp(color.a,0.,1.);
        pixel = rgb555(fma(color.rgb,vec3(alpha),unpack555(pixel)*(1.-alpha)));
    }
    pixels[index] = pixel;
}
