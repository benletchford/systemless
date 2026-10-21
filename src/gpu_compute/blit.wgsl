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

@group(0) @binding(7) var<storage,read> arithmetic:array<u32>;
fn arithmetic_rgb(pixel:u32,clut:u32) -> vec3<u32> {
    let depth=arithmetic[0];
    if depth==16u {
        return (vec3<u32>((pixel>>10u)&31u,(pixel>>5u)&31u,pixel&31u)*65535u+vec3<u32>(15u))/31u;
    }
    if depth==32u {return vec3<u32>((pixel>>16u)&255u,(pixel>>8u)&255u,pixel&255u)*257u;}
    let offset=clut+pixel*3u;
    return vec3<u32>(arithmetic[offset],arithmetic[offset+1u],arithmetic[offset+2u]);
}
fn arithmetic_channel(s:u32,d:u32,op:u32) -> u32 {
    switch arithmetic[1] {
        case 32u: {return (s*op+d*(65536u-op))>>16u;}
        case 33u: {return min(min(s+d,65535u),op);}
        case 34u: {return (s+d)&65535u;}
        case 35u: {return max(s-min(s,d),op);}
        case 37u: {return max(s,d);}
        case 38u: {return (s+65536u-d)&65535u;}
        default: {return min(s,d);}
    }
}
fn arithmetic_pixel(s:u32,d:u32) -> u32 {
    let depth=arithmetic[0];let mode=arithmetic[1];
    if depth==1u {
        if mode==32u {return s;}
        if mode==33u || mode==37u {return d&~s;}
        if mode==34u || mode==38u {return d^s;}
        return d|s;
    }
    let clut=6u+arithmetic[2]*3u;
    let source_rgb=arithmetic_rgb(s,6u);let destination_rgb=arithmetic_rgb(d,clut);
    let rgb=vec3<u32>(arithmetic_channel(source_rgb.x,destination_rgb.x,arithmetic[3]),
        arithmetic_channel(source_rgb.y,destination_rgb.y,arithmetic[4]),
        arithmetic_channel(source_rgb.z,destination_rgb.z,arithmetic[5]));
    if depth==16u {return ((rgb.x>>11u)<<10u)|((rgb.y>>11u)<<5u)|(rgb.z>>11u);}
    if depth==32u {return ((rgb.x>>8u)<<16u)|((rgb.y>>8u)<<8u)|(rgb.z>>8u);}
    let requested_color=vec3<i32>(rgb>>vec3<u32>(11u));
    var best=0u;var best_distance=0xffffffffu;
    for(var i=0u;i<arithmetic[2];i++) {
        let base=clut+i*3u;
        let color=vec3<i32>(i32(arithmetic[base]>>11u),i32(arithmetic[base+1u]>>11u),i32(arithmetic[base+2u]>>11u));
        let delta=requested_color-color;
        let distance=u32(delta.x*delta.x+delta.y*delta.y+delta.z*delta.z);
        if distance<best_distance {best=i;best_distance=distance;}
    }
    return best;
}
@compute @workgroup_size(8,8) fn blit_arithmetic(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=geometry.size.y || id.y>=geometry.size.z {return;}
    let xs=columns[id.x];let ys=rows[id.y];
    let raw=source[ys.x*geometry.size.x+xs.x];
    let index=id.y*geometry.size.y+id.x;
    var covered=true;
    if (coverage[0]&1u)!=0u {covered=coverage[index+3u]!=0u;}
    if (coverage[0]&2u)!=0u && (raw&coverage[2])==coverage[1] {covered=false;}
    let background=arithmetic[6u+arithmetic[2]*6u+index];
    output[index*2u]=arithmetic_pixel(raw,background);
    output[index*2u+1u]=select(0u,1u,covered);
}

@group(0) @binding(8) var<storage,read> deep_mask:array<u32>;
fn deep_rgb(depth:u32,pixel:u32,clut:u32) -> vec3<u32> {
    if depth==1u {return select(vec3<u32>(0u),vec3<u32>(65535u),pixel==0u);}
    if depth==16u {
        let c=vec3<u32>((pixel>>10u)&31u,(pixel>>5u)&31u,pixel&31u);
        if (deep_mask[3]&256u)!=0u {return (c<<vec3<u32>(11u))|(c<<vec3<u32>(6u))|(c<<vec3<u32>(1u))|(c>>vec3<u32>(4u));}
        return (c*65535u+vec3<u32>(15u))/31u;
    }
    if depth==32u {return vec3<u32>((pixel>>16u)&255u,(pixel>>8u)&255u,pixel&255u)*257u;}
    let base=clut+pixel*3u;return vec3<u32>(deep_mask[base],deep_mask[base+1u],deep_mask[base+2u]);
}
fn deep_encode(rgb:vec3<u32>,clut:u32) -> u32 {
    let depth=deep_mask[1];
    if depth==1u {return select(0u,1u,rgb.x+rgb.y+rgb.z<=98302u);}
    if depth==16u {return ((rgb.x>>11u)<<10u)|((rgb.y>>11u)<<5u)|(rgb.z>>11u);}
    if depth==32u {return ((rgb.x>>8u)<<16u)|((rgb.y>>8u)<<8u)|(rgb.z>>8u);}
    if (deep_mask[3]&256u)!=0u {
        // Classic inverse tables pin exact white to the first endpoint even
        // when the terminal entry duplicates white; exact black prefers last.
        if all(rgb==vec3<u32>(65535u)) && all(vec3<u32>(deep_mask[clut],deep_mask[clut+1u],deep_mask[clut+2u])==rgb) {return 0u;}
        let last=deep_mask[5]-1u;
        let endpoint=clut+last*3u;
        if all(rgb==vec3<u32>(deep_mask[endpoint],deep_mask[endpoint+1u],deep_mask[endpoint+2u])) {return last;}
        var best=0u;var best_low=0xffffffffu;var best_high=0xffffffffu;
        for(var i=0u;i<deep_mask[5];i++) {
            let base=clut+i*3u;
            let color=vec3<u32>(deep_mask[base],deep_mask[base+1u],deep_mask[base+2u]);
            if all(rgb==color) {return i;}
            let delta=max(rgb,color)-min(rgb,color);
            let square=delta*delta;
            let rg=square.x+square.y;
            let low=rg+square.z;
            let high=select(0u,1u,rg<square.x)+select(0u,1u,low<rg);
            if high<best_high || (high==best_high && low<best_low) {best=i;best_low=low;best_high=high;}
        }
        return best;
    }
    let requested_color=vec3<i32>(rgb>>vec3<u32>(11u));
    var best=0u;var best_distance=0xffffffffu;
    for(var i=0u;i<deep_mask[5];i++) {
        let base=clut+i*3u;
        let color=vec3<i32>(i32(deep_mask[base]>>11u),i32(deep_mask[base+1u]>>11u),i32(deep_mask[base+2u]>>11u));
        let delta=requested_color-color;let distance=u32(delta.x*delta.x+delta.y*delta.y+delta.z*delta.z);
        if distance<best_distance {best=i;best_distance=distance;}
    }
    return best;
}
fn deep_boolean(s:u32,d:u32) -> u32 {
    var bits=0xffffffffu;if deep_mask[1]<32u {bits=(1u<<deep_mask[1])-1u;}
    switch (deep_mask[3]&63u) {
        case 1u: {return s|d;}
        case 2u: {return s^d;}
        case 3u: {return (~s)&d&bits;}
        case 4u: {return (~s)&bits;}
        case 5u: {return ((~s)&bits)|d;}
        case 6u: {return ((~s)&bits)^d;}
        case 7u: {return s&d;}
        default: {return s;}
    }
}
@compute @workgroup_size(8,8) fn blit_deep_mask(@builtin(global_invocation_id) id:vec3<u32>) {
    if id.x>=geometry.size.y || id.y>=geometry.size.z {return;}
    let raw=source[rows[id.y].x*geometry.size.x+columns[id.x].x];
    let index=id.y*geometry.size.y+id.x;
    let destination_clut=11u+deep_mask[4]*3u;
    let mask_clut=destination_clut+deep_mask[5]*3u;
    let mask_pixels=mask_clut+deep_mask[6]*3u;
    let mask_columns=mask_pixels+deep_mask[7]*deep_mask[8];
    let mask_rows=mask_columns+deep_mask[9];
    let mask_pixel=deep_mask[mask_pixels+deep_mask[mask_rows+id.y]*deep_mask[7]+deep_mask[mask_columns+id.x]];
    let destination=deep_mask[mask_rows+deep_mask[10]+index];
    var covered=true;
    if (coverage[0]&1u)!=0u {covered=coverage[index+3u]!=0u;}
    if (coverage[0]&2u)!=0u && (raw&coverage[2])==coverage[1] {covered=false;}
    var source_rgb=deep_rgb(deep_mask[0],raw,11u);
    var value=0u;
    if deep_mask[2]==1u {
        covered=covered && mask_pixel!=0u;
        value=deep_boolean(deep_encode(source_rgb,destination_clut),destination);
    } else {
        if (deep_mask[3]&63u)>=1u && (deep_mask[3]&63u)<=7u {
            source_rgb=deep_rgb(deep_mask[1],deep_boolean(deep_encode(source_rgb,destination_clut),destination),destination_clut);
        }
        let d=deep_rgb(deep_mask[1],destination,destination_clut);
        let m=deep_rgb(deep_mask[2],mask_pixel,mask_clut);
        // Complementary weights keep the rounded numerator below 2^32.
        let rgb=(source_rgb*(vec3<u32>(65535u)-m)+d*m+vec3<u32>(32767u))/65535u;
        value=deep_encode(rgb,destination_clut);
    }
    output[index*2u]=value;output[index*2u+1u]=select(0u,1u,covered);
}
