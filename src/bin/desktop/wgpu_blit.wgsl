struct Geometry { size:vec4<u32>, transfer:vec4<u32> }
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
    if geometry.transfer.x!=0u {value=direct_color(value);}
    output[id.y*geometry.size.y+id.x]=value;
}

@compute @workgroup_size(8,8) fn packed_copy(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=geometry.size.y || id.y>=geometry.size.z {return;}
    let index=id.y*geometry.size.y+id.x;
    var value=columns[index].x;
    let depth=geometry.size.w;
    let mask=(1u<<depth)-1u;
    for(var lane=0u;lane<8u/depth;lane++) {
        let bit=id.x*8u+lane*depth;
        if bit>=geometry.transfer.y && bit<geometry.transfer.y+geometry.transfer.z*depth {
            let source_bit=geometry.transfer.x+bit-geometry.transfer.y;
            var pixel=(source[id.y*geometry.size.x+source_bit/8u]>>(8u-depth-source_bit%8u))&mask;
            if geometry.transfer.w!=0u {pixel=palette[pixel];}
            let shift=8u-depth-bit%8u;
            value=(value&~(mask<<shift))|(pixel<<shift);
        }
    }
    output[index]=value;
}

fn direct_color(pixel:u32) -> u32 {
    var rgb=vec3<u32>((pixel>>16u)&255u,(pixel>>8u)&255u,pixel&255u);
    if geometry.transfer.x==16u {
        let channels=vec3<u32>((pixel>>10u)&31u,(pixel>>5u)&31u,pixel&31u);
        rgb=((channels*65535u+vec3<u32>(15u))/31u)>>vec3<u32>(8u);
    }
    let depth=geometry.transfer.y;
    if depth==32u {return (rgb.x<<16u)|(rgb.y<<8u)|rgb.z;}
    if depth==16u {return ((rgb.x>>3u)<<10u)|((rgb.y>>3u)<<5u)|(rgb.z>>3u);}
    // Exact squared-distance comparison against black/white after 8-to-16-bit expansion.
    if depth==1u && geometry.transfer.x==32u {return select(0u,1u,rgb.x+rgb.y+rgb.z<=382u);}
    let requested_color=vec3<i32>(rgb>>vec3<u32>(3u));
    var best=0u; var best_distance=0xffffffffu;
    for(var i=0u;i<geometry.transfer.z;i++) {
        let color=palette[i];
        let delta=requested_color-vec3<i32>(i32((color>>10u)&31u),i32((color>>5u)&31u),i32(color&31u));
        let distance=u32(delta.x*delta.x+delta.y*delta.y+delta.z*delta.z);
        if distance<best_distance {best=i;best_distance=distance;}
    }
    return best;
}

@group(0) @binding(6) var<storage,read> coverage:array<u32>;
@compute @workgroup_size(8,8) fn blit_cells(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=geometry.size.y || id.y>=geometry.size.z {return;}
    let xs=columns[id.x]; let ys=rows[id.y];
    var value=0u;
    for(var y=ys.x;y<ys.y;y++) {
        for(var x=xs.x;x<xs.y;x++) {value=max(value,source[y*geometry.size.x+x]);}
    }
    let index=id.y*geometry.size.y+id.x;
    var covered=true;
    if (coverage[0]&1u)!=0u {covered=coverage[index+3u]!=0u;}
    if (coverage[0]&2u)!=0u && (value&coverage[2])==coverage[1] {covered=false;}
    if geometry.size.w!=0u {value=palette[value];}
    if geometry.transfer.x!=0u {value=direct_color(value);}
    output[index*2u]=value;
    output[index*2u+1u]=select(0u,1u,covered);
}
