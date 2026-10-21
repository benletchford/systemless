struct Geometry {
    area:vec4<i32>, rect:vec4<i32>, border:vec4<i32>,
    transfer:vec4<u32>, pattern0:vec4<u32>, pattern1:vec4<u32>, polygon:vec4<u32>, mask:vec4<i32>,
}
@group(0) @binding(0) var<storage,read> background:array<u32>;
@group(0) @binding(1) var<storage,read_write> result:array<vec2<u32>>;
@group(0) @binding(2) var<uniform> geometry:Geometry;
@group(0) @binding(3) var<storage,read> clips:array<i32>;
@group(0) @binding(4) var<storage,read> edges:array<vec4<f32>>;
@group(0) @binding(6) var<storage,read> coverage:array<u32>;
@group(0) @binding(5) var<storage,read_write> products:array<f32>;
// A storage boundary rounds the product before the add, matching the guest's
// software f32 evaluation rather than Metal's fused multiply-add.
@compute @workgroup_size(64) fn polygon_products(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=geometry.polygon.y || id.y>=u32(geometry.area.w) {return;}
    let edge=edges[id.x];
    products[id.y*geometry.polygon.y+id.x]=(f32(geometry.area.y+i32(id.y))-edge.x)*edge.w;
}
@compute @workgroup_size(8,8) fn raster(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=vec2<u32>(geometry.area.zw)) {return;}
    let index=id.y*u32(geometry.area.z)+id.x;
    let point=geometry.area.xy+vec2<i32>(id.xy);
    var covered=all(point>=geometry.rect.xy)&&all(point<geometry.rect.zw);
    if geometry.border.z!=0 {
        let inner=all(point>=geometry.rect.xy+geometry.border.xy)&&all(point<geometry.rect.zw-geometry.border.xy);
        covered=covered&&!inner;
    }
    if covered && geometry.border.w!=0 {
        let offset=u32(clips[id.y*2u]);let count=u32(clips[id.y*2u+1u]);
        var in_clip=false;
        for(var i=0u;i<count;i+=2u) {if point.x>=clips[offset+i] && point.x<clips[offset+i+1u] {in_clip=true;break;}}
        covered=in_clip;
    }
    if covered && geometry.polygon.x!=0u {
        var crossings=0u;
        for(var i=0u;i<geometry.polygon.y;i+=1u) {
            let edge=edges[i];
            if f32(point.y)>=edge.x && f32(point.y)<edge.y {
                let intersection=edge.z+products[id.y*geometry.polygon.y+i];
                if intersection<=f32(point.x) {crossings+=1u;}
            }
        }
        covered=(crossings&1u)!=0u;
    }
    var repetitions=1u;
    if covered && geometry.polygon.z!=0u {
        let texel=point-geometry.mask.xy;
        if any(texel<vec2(0)) || any(texel>=geometry.mask.zw) {covered=false;}
        else {
            repetitions=coverage[u32(texel.y)*u32(geometry.mask.z)+u32(texel.x)];
            covered=repetitions>=geometry.polygon.z;
        }
    }
    var value=background[index];
    if covered {
        let row=u32(point.y&7);
        var pattern=geometry.pattern0[row&3u];
        if row>=4u {pattern=geometry.pattern1[row&3u];}
        let black=((pattern>>u32(7-(point.x&7)))&1u)!=0u;
        let masks=select(geometry.transfer.xy,geometry.transfer.zw,black);
        var xor=masks.y;
        if geometry.polygon.w!=0u && (repetitions&1u)==0u {xor^=xor&masks.x;}
        value=(value&masks.x)^xor;
    }
    result[index]=vec2(value,u32(covered));
}
