struct Vertex { position: vec4<f32>, uv: vec4<f32>, color: vec4<f32>, lighting: vec4<f32>, normal:vec4<f32>, view:vec4<f32>, specular:vec4<f32>, world:vec4<f32> }
struct Triangle { vertices: array<Vertex,3>, info: vec4<u32>, fog_color: vec4<f32>, fog_parameters: vec4<f32>, ambient: vec4<f32>, light_info:vec4<u32> }
struct Geometry { size: vec4<u32>, viewport: vec4<u32>, clear: vec4<u32> }
@group(0) @binding(0) var<storage, read> triangles: array<Triangle>;
@group(0) @binding(1) var<storage, read> textures: array<u32>;
@group(0) @binding(2) var<storage, read_write> pixels: array<u32>;
@group(0) @binding(3) var<uniform> geometry: Geometry;
struct Light { direction:vec4<f32>, color:vec4<f32>, location:vec4<f32>, parameters:vec4<f32> }
@group(0) @binding(4) var<storage,read> lights:array<Light>;
@group(0) @binding(5) var<storage,read> palette:array<u32>;
@group(0) @binding(6) var<storage,read> tiles:array<u32>;
@group(0) @binding(7) var<storage,read_write> shaded_triangles:array<Triangle>;
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
        var direction=light.direction;
        var scale=1.;
        if light.location.w != 0. {
            let offset=v.world.xyz-light.location.xyz;
            let squared=dot(offset,offset);
            if !(squared > 0. && squared <= 3.402823e38) { continue; }
            let distance=sqrt(squared);
            direction=vec4(offset/distance,1.);
            if light.parameters.x == 1. { scale=1./distance; }
            else if light.parameters.x == 2. { scale=1./(distance*distance); }
            if !(scale >= 0. && scale <= 3.402823e38) { continue; }
            if light.location.w == 2. {
                if light.direction.w == 0. { continue; }
                let angle=acos(clamp(dot(direction.xyz,light.direction.xyz),-1.,1.));
                if angle > light.parameters.z { continue; }
                if angle > light.parameters.y {
                    let span=light.parameters.z-light.parameters.y;
                    if span <= 0. { continue; }
                    let falloff=clamp((light.parameters.z-angle)/span,0.,1.);
                    if light.parameters.w == 1. { scale*=falloff; }
                    else if light.parameters.w == 2. { scale*=falloff*falloff; }
                    else if light.parameters.w == 3. { scale*=0.5-0.5*cos(3.141592653589793*falloff); }
                }
                if scale <= 0. { continue; }
            }
        }
        var intensity=1.;
        if v.normal.w != 0. && direction.w != 0. {
            intensity=max(-dot(v.normal.xyz,direction.xyz),0.);
            if t.light_info.w != 0u && v.lighting.y != 0. && intensity > 0. && v.specular.w >= 0. && v.view.w != 0. {
                let view=unit(v.view.xyz);
                let reflection=unit(fma(vec3(2.*intensity),v.normal.xyz,direction.xyz));
                if view.w != 0. && reflection.w != 0. {
                    let cosine=max(dot(reflection.xyz,view.xyz),0.);
                    var power=1.;
                    if v.specular.w != 0. { power=pow(cosine,v.specular.w); }
                    if power > 0. && power <= 3.402823e38 { specular+=light.color.rgb*light.color.w*v.specular.rgb*power*scale; }
                }
            }
        }
        factor+=light.color.rgb*light.color.w*(intensity*scale);
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
@compute @workgroup_size(64) fn shade_vertices(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if index >= geometry.size.z { return; }
    let triangle = shaded_triangles[index];
    let kind = (triangle.info.w >> 4u) & 3u;
    if kind != 0u { return; }
    let a = triangle.vertices[0]; let b = triangle.vertices[1]; let c = triangle.vertices[2];
    let spatial_lighting = triangle.light_info.y != 0u;
    if spatial_lighting && triangle.light_info.z != 2u {
        shaded_triangles[index].vertices[0].color = vec4(shade(a, a.color.rgb, triangle), a.color.a);
        shaded_triangles[index].vertices[1].color = vec4(shade(b, b.color.rgb, triangle), b.color.a);
        shaded_triangles[index].vertices[2].color = vec4(shade(c, c.color.rgb, triangle), c.color.a);
    } else if !spatial_lighting && triangle.ambient.w == 1. {
        shaded_triangles[index].vertices[0].color = vec4(a.color.rgb * triangle.ambient.rgb * a.lighting.x, a.color.a);
        shaded_triangles[index].vertices[1].color = vec4(b.color.rgb * triangle.ambient.rgb * b.lighting.x, b.color.a);
        shaded_triangles[index].vertices[2].color = vec4(c.color.rgb * triangle.ambient.rgb * c.lighting.x, c.color.a);
    }
}
@compute @workgroup_size(8,8) fn raster(@builtin(global_invocation_id) id: vec3<u32>) {
    let xy = id.xy;
    if any(xy >= geometry.size.xy) { return; }
    let index = xy.y*geometry.size.x+xy.x;
    let in_viewport=all(xy>=geometry.viewport.xy)&&all(xy<=geometry.viewport.zw);
    var pixel = 0u;
    if in_viewport && geometry.clear.x != 0u {
        pixel = geometry.clear.y;
        if geometry.clear.z != 0u { pixel = palette[pixel & 32767u]; }
    } else {
        pixel = pixels[index];
    }
    var depth = 3.402823e38;
    let point = vec2<f32>(xy);
    let tile=(xy.y/16u)*((geometry.size.x+15u)/16u)+xy.x/16u;
    let offset=tiles[tile*2u];
    let count=tiles[tile*2u+1u];
    for(var entry=0u; entry<count; entry++) {
        let triangle_index = tiles[offset+entry];
        let info = triangles[triangle_index].info;
        let kind=(info.w>>4u)&3u;
        let p0 = triangles[triangle_index].vertices[0].position;
        var perspective=vec3(1.,0.,0.);
        var z=p0.z;
        if kind == 0u {
            if !in_viewport {continue;}
            let p1 = triangles[triangle_index].vertices[1].position;
            let p2 = triangles[triangle_index].vertices[2].position;
            let area = edge(p0.xy,p1.xy,p2.xy);
            let weights = vec3(edge(p1.xy,p2.xy,point),edge(p2.xy,p0.xy,point),edge(p0.xy,p1.xy,point));
            if (area > 0. && any(weights < vec3(0.))) || (area < 0. && any(weights > vec3(0.))) { continue; }
            let affine = weights/area;
            z = p0.z+(p1.z-p0.z)*affine.y+(p2.z-p0.z)*affine.z;
            if (info.w&8u)==0u && z > depth { continue; }
            perspective = affine*vec3(p0.w,p1.w,p2.w);
            perspective /= dot(perspective,vec3(1.));
        } else if kind == 2u {
            if any(vec2<i32>(xy) != vec2<i32>(p0.xy)) { continue; }
            if (info.w&8u)==0u && z > depth { continue; }
        } else {
            // Closed-form Bresenham coverage, including its directional tie rule.
            let p1 = triangles[triangle_index].vertices[1].position;
            let start=vec2<i32>(p0.xy); let end=vec2<i32>(p1.xy);
            let delta=abs(end-start); let sign=select(vec2(-1),vec2(1),start<end);
            let major=max(delta.x,delta.y);
            let horizontal=delta.x>=delta.y;
            let step=select((i32(xy.y)-start.y)*sign.y,(i32(xy.x)-start.x)*sign.x,horizontal);
            if step < 0 || step > major { continue; }
            var expected=start;
            if major != 0 {
                let minor_delta=select(delta.x,delta.y,horizontal);
                let advance=select(i32((u32(step)*u32(minor_delta)+u32(major)/2u)/u32(max(major,1))),minor_delta,step==major);
                // FXC cannot address dynamically selected vector components as
                // l-values. Assemble the whole advance vector instead.
                expected+=select(vec2(advance,step),vec2(step,advance),horizontal)*sign;
            }
            if any(vec2<i32>(xy) != expected) { continue; }
            let t=f32(step)/f32(max(major,1));
            z=fma(p0.z,1.-t,p1.z*t);
            if (info.w&8u)==0u && z > depth { continue; }
            perspective=vec3(0.,0.,1.);
        }
        if (info.w & 1u) != 0u { depth = z; }
        let ca = triangles[triangle_index].vertices[0].color;
        let cb = triangles[triangle_index].vertices[1].color;
        let cc = triangles[triangle_index].vertices[2].color;
        var color = ca+(cb-ca)*perspective.y+(cc-ca)*perspective.z;
        let fog_color = triangles[triangle_index].fog_color;
        let fog_mode = u32(fog_color.w);
        var uv0 = vec4(0.); var uv1 = vec4(0.); var uv2 = vec4(0.);
        if info.y != 0u || fog_mode != 0u {
            uv0 = triangles[triangle_index].vertices[0].uv;
            uv1 = triangles[triangle_index].vertices[1].uv;
            uv2 = triangles[triangle_index].vertices[2].uv;
        }
        if info.y != 0u {
            var uv = uv0.xy+(uv1.xy-uv0.xy)*perspective.y+(uv2.xy-uv0.xy)*perspective.z;
            uv = select(clamp(uv,vec2(0.),vec2(1.)), uv-floor(uv), vec2((info.w&2u)!=0u,(info.w&4u)!=0u));
            let size = info.yz;
            let texel = min(vec2<u32>(vec2(uv.x,1.-uv.y)*vec2<f32>(size)),size-vec2(1u));
            let rgba = textures[info.x+texel.y*size.x+texel.x];
            color *= vec4<f32>((vec4(rgba)>>vec4(0u,8u,16u,24u))&vec4(255u))/255.;
        }
        let light_info = triangles[triangle_index].light_info;
        let spatial_lighting = light_info.y != 0u;
        let vertex_lighting = spatial_lighting && light_info.z != 2u && kind == 0u;
        if spatial_lighting && !vertex_lighting {
            let triangle = triangles[triangle_index];
            color=vec4(shade(interpolate_shading(triangle.vertices[0],triangle.vertices[1],triangle.vertices[2],perspective),color.rgb,triangle),color.a);
        } else if !spatial_lighting {
            let ambient = triangles[triangle_index].ambient;
            if ambient.w == 2. || (kind!=0u && ambient.w!=0.) {
                let la = triangles[triangle_index].vertices[0].lighting.x;
                let lb = triangles[triangle_index].vertices[1].lighting.x;
                let lc = triangles[triangle_index].vertices[2].lighting.x;
                let coefficient = la+(lb-la)*perspective.y+(lc-la)*perspective.z;
                color=vec4(color.rgb*ambient.rgb*coefficient,color.a);
            }
        }
        if fog_mode != 0u {
            let fog_depth = uv0.z+(uv1.z-uv0.z)*perspective.y+(uv2.z-uv0.z)*perspective.z;
            let parameters = triangles[triangle_index].fog_parameters;
            var factor = 1.;
            if fog_mode == 1u {
                let range = parameters.y-parameters.x;
                if range == 0. { factor = select(0.,1.,fog_depth < parameters.y); }
                else { factor = (parameters.y-fog_depth)/range; }
            } else if fog_mode == 2u { factor = exp(-parameters.z*fog_depth); }
            else if fog_mode == 3u { let value=parameters.z*fog_depth; factor=exp(-value*value); }
            else if fog_mode == 4u && all(vec3(uv0.w,uv1.w,uv2.w)>=vec3(0.)) {
                factor = uv0.w+(uv1.w-uv0.w)*perspective.y+(uv2.w-uv0.w)*perspective.z;
            }
            // Overflowing exponential factors mean no fog, matching the CPU.
            if abs(factor) <= 3.402823e38 {
                factor = clamp(factor,0.,1.);
                color = vec4(fma(color.rgb,vec3(factor),fog_color.rgb*(1.-factor)),color.a);
            }
        }
        let alpha = clamp(color.a,0.,1.);
        pixel = rgb555(fma(color.rgb,vec3(alpha),unpack555(pixel)*(1.-alpha)));
        if geometry.clear.z != 0u { pixel = palette[pixel]; }
    }
    if geometry.clear.z != 0u && (pixel & 32768u) == 0u {
        pixel = pixel >> 16u;
    }
    pixels[index] = pixel;
}
