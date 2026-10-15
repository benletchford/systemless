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
        if p[2]==4 {
            let sx=p[14]+((x-p[3])*(p[18]-p[14]))/(p[5]-p[3]);
            let sy=p[15]+((y-p[4])*(p[19]-p[15]))/(p[6]-p[4]);
            var bit=0u;
            if (p[18]-p[14])>(p[5]-p[3]) || (p[19]-p[15])>(p[6]-p[4]) {
                // Basic QuickDraw reductions OR the source footprint. Enlargement
                // still samples a single source pixel along the enlarged axis.
                let ex=max(sx+1,p[14]+((x-p[3]+1)*(p[18]-p[14]))/(p[5]-p[3]));
                let ey=max(sy+1,p[15]+((y-p[4]+1)*(p[19]-p[15]))/(p[6]-p[4]));
                let lx=max(0,sx); let ly=max(0,sy);
                let hx=min(p[16],ex); let hy=min(p[17],ey);
                valid=lx<hx && ly<hy;
                for (var yy=ly; yy<hy && bit==0u; yy=yy+1) {
                    for (var xx=lx; xx<hx && bit==0u; xx=xx+1) {
                        bit=bit | source[u32(yy*p[16]+xx)];
                    }
                }
            } else if sx>=0 && sy>=0 && sx<p[16] && sy<p[17] {
                bit=source[u32(sy*p[16]+sx)];
            } else { valid=false; }
            bit=bit ^ u32(p[20]);
            src=select(u32(p[12]),u32(p[11]),bit!=0u);
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
    // Cursor data/mask affect presentation only, never canonical pixel storage.
    if p[2]==3 {
        let cx=x-p[25]; let cy=y-p[26];
        if cx>=0 && cy>=0 && cx<16 && cy<16 {
            let bit=15u-u32(cx);
            let data=(u32(p[27+cy])>>bit)&1u;
            let mask=(u32(p[43+cy])>>bit)&1u;
            value=(value & (~mask)) ^ data;
        }
    }
    let rgba=palette[value];
    textureStore(output,vec2<i32>(x,y),vec4<f32>(f32(rgba&255u),f32((rgba>>8u)&255u),f32((rgba>>16u)&255u),f32(rgba>>24u))/255.0);
}
