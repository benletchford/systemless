struct Geometry {
    area:vec4<i32>, rect:vec4<i32>, border:vec4<i32>,
    transfer:vec4<u32>, pattern0:vec4<u32>, pattern1:vec4<u32>,
}
@group(0) @binding(0) var<storage,read> background:array<u32>;
@group(0) @binding(1) var<storage,read_write> result:array<vec2<u32>>;
@group(0) @binding(2) var<uniform> geometry:Geometry;
@compute @workgroup_size(8,8) fn raster(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=vec2<u32>(geometry.area.zw)) {return;}
    let index=id.y*u32(geometry.area.z)+id.x;
    let point=geometry.area.xy+vec2<i32>(id.xy);
    var covered=all(point>=geometry.rect.xy)&&all(point<geometry.rect.zw);
    if geometry.border.z!=0 {
        let inner=all(point>=geometry.rect.xy+geometry.border.xy)&&all(point<geometry.rect.zw-geometry.border.xy);
        covered=covered&&!inner;
    }
    var value=background[index];
    if covered {
        let row=u32(point.y&7);
        var pattern=geometry.pattern0[row&3u];
        if row>=4u {pattern=geometry.pattern1[row&3u];}
        let black=((pattern>>u32(7-(point.x&7)))&1u)!=0u;
        let masks=select(geometry.transfer.xy,geometry.transfer.zw,black);
        value=(value&masks.x)^masks.y;
    }
    result[index]=vec2(value,u32(covered));
}
