// Each guest index is widened to u32 in GPU storage. No normalized blending.
@group(0) @binding(0) var<storage, read_write> pixels: array<u32>;
@group(0) @binding(1) var<storage, read> source: array<u32>;
@group(0) @binding(2) var<storage, read> palette: array<u32>;
@group(0) @binding(3) var<storage, read> p: array<i32>;
@group(0) @binding(4) var output: texture_storage_2d<rgba8unorm, write>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let x=i32(id.x); let y=i32(id.y); let w=p[0]; let h=p[1];
    if x>=w || y>=h { return; }
    let at=u32(y*w+x);
    var value=pixels[at];
    var in_region=p[24]<0;
    for (var n=0; n<p[24] && !in_region; n=n+1) {
        let at=25+4*n;
        in_region=x>=p[at] && y>=p[at+1] && x<p[at+2] && y<p[at+3];
    }
    // rect and clip use half-open guest coordinates.
    if in_region && p[2]!=0 && x>=p[3] && y>=p[4] && x<p[5] && y<p[6] &&
       x>=p[7] && y>=p[8] && x<p[9] && y<p[10] {
        var src=u32(p[11]);
        let row=u32(p[16+(y & 7)]);
        if (row & (128u >> u32(x & 7)))==0u { src=u32(p[12]); }
        var valid=true;
        if p[2]==2 {
            let sx=x-p[3]+p[14]; let sy=y-p[4]+p[15];
            if sx>=0 && sy>=0 && sx<w && sy<h { src=source[u32(sy*w+sx)]; }
            else { valid=false; }
        }
        if valid { switch p[13] {
            case 0: { value=src; }
            case 1: { value=value | src; }
            case 2: { value=value ^ src; }
            case 3: { value=value & (~src); }
            default: {}
        } }
        value=value & 255u;
        pixels[at]=value;
    }
    let rgba=palette[value];
    textureStore(output,vec2<i32>(x,y),vec4<f32>(f32(rgba&255u),f32((rgba>>8u)&255u),f32((rgba>>16u)&255u),f32(rgba>>24u))/255.0);
}
