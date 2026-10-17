struct Geometry { size:vec4<u32> }
@group(0) @binding(0) var<storage,read> source:array<u32>;
@group(0) @binding(1) var<storage,read> palette:array<u32>;
@group(0) @binding(2) var<storage,read> columns:array<vec2<u32>>;
@group(0) @binding(3) var<storage,read> rows:array<vec2<u32>>;
@group(0) @binding(4) var<storage,read_write> output:array<u32>;
@group(0) @binding(5) var<uniform> geometry:Geometry;
@compute @workgroup_size(8,8) fn blit(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=geometry.size.y || id.y>=geometry.size.z {return;}
    let xs=columns[id.x]; let ys=rows[id.y];
    var value=0u;
    for(var y=ys.x;y<ys.y;y++) {
        for(var x=xs.x;x<xs.y;x++) {value=max(value,source[y*geometry.size.x+x]);}
    }
    if geometry.size.w!=0u {value=palette[value];}
    output[id.y*geometry.size.y+id.x]=value;
}
