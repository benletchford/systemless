// Each guest index is widened to u32 in GPU storage. No normalized blending.
@group(0) @binding(0) var<storage, read_write> pixels: array<u32>;
@group(0) @binding(1) var<storage, read> source: array<u32>;
@group(0) @binding(2) var<storage, read> palette: array<u32>;
@group(0) @binding(3) var<storage, read> p: array<i32>;
@group(0) @binding(4) var output: texture_storage_2d<rgba8unorm, write>;
// Match indexed source RGB against the current white/black destination policy.
fn blit_bit(index:u32)->u32 {
    if p[21]==0 {return index;}
    let at=25+4*max(p[24],0)+3*i32(index);
    let sum=p[at]+p[at+1]+p[at+2];
    return select(0u,1u,sum<98303);
}
// Color configuration is an optional fixed-size tail. Draw-kind flags leave
// all eight legacy pattern rows and existing variable command payloads intact.
fn draw_kind()->i32{return p[2]&255;}
fn color_target()->i32{if (p[2]&256)==0{return 0;}return i32(arrayLength(&p))-1026;}
// Indexed destination colors keep their original sixteen-bit components.
fn destination_rgb(index:u32)->vec3<u32>{let q=color_target()+2+4*i32(index);return vec3<u32>(u32(p[q]),u32(p[q+1]),u32(p[q+2]));}
fn closest(rgb:vec3<u32>)->u32 {
    let q=color_target();var best=0u;var best_hi=0xffffffffu;var best_lo=0xffffffffu;
    for(var i=0u;i<(1u<<u32(p[q]));i=i+1u){
        if p[q+2+4*i32(i)+3]==0{continue;}
        let c=destination_rgb(i);let d=max(rgb,c)-min(rgb,c);let square=d*d;
        let a=square.x+square.y;var hi=select(0u,1u,a<square.x);let lo=a+square.z;hi=hi+select(0u,1u,lo<a);
        if hi<best_hi || (hi==best_hi && lo<best_lo){best=i;best_hi=hi;best_lo=lo;}
    }return best;
}
fn weighted(source:vec3<u32>,foreground:vec3<u32>,background:vec3<u32>)->vec3<u32>{return (source*background+(vec3<u32>(65535u)-source)*foreground+vec3<u32>(32767u))/vec3<u32>(65535u);}
fn color_result(src:u32,dst:u32,pattern_on:bool)->u32 {
    let mode=p[13];
    if draw_kind()!=4 && draw_kind()!=2 {
        switch mode {
            case 0:{return src;}
            case 1:{return select(dst,u32(p[11]),pattern_on);}
            case 2:{if pattern_on{return closest(vec3<u32>(65535u)-destination_rgb(dst));}return dst;}
            case 3:{return select(dst,u32(p[12]),pattern_on);}
            default:{return dst;}
        }
    }
    if draw_kind()==2 && mode==0{return src;}
    var rgb=select(vec3<u32>(65535u),vec3<u32>(0u),src!=0u);
    if draw_kind()==2{rgb=destination_rgb(src);}
    else if p[21]!=0{let at=25+4*max(p[24],0)+3*i32(src);rgb=vec3<u32>(u32(p[at]),u32(p[at+1]),u32(p[at+2]));}
    if draw_kind()==4 && p[20]!=0{rgb=vec3<u32>(65535u)-rgb;}
    var fg=u32(p[11]);var bg=u32(p[12]);
    if draw_kind()==2{fg=closest(vec3<u32>(0u));bg=closest(vec3<u32>(65535u));}
    if mode==0 {
        if draw_kind()==4 && p[color_target()+1]!=0 && p[20]==0 && all(destination_rgb(fg)==vec3<u32>(0u)) && all(destination_rgb(bg)==vec3<u32>(65535u)){return src;}
        if all(rgb==vec3<u32>(0u)){return fg;}if all(rgb==vec3<u32>(65535u)){return bg;}
        return closest(weighted(rgb,destination_rgb(fg),destination_rgb(bg)));
    }
    if mode==2{if all(rgb==vec3<u32>(0u)){return closest(vec3<u32>(65535u)-destination_rgb(dst));}return dst;}
    if all(rgb==vec3<u32>(65535u)){return dst;}
    let ink=select(fg,bg,mode==3);
    if all(rgb==vec3<u32>(0u)){return ink;}
    return closest(weighted(rgb,destination_rgb(ink),destination_rgb(dst)));
}
// Unsigned products fit in u32 for signed-word endpoints, including extremes.
fn line_minor(origin:i32,delta:i32,steps:i32,t:i32)->i32 {
    if steps==0 {return origin;}
    let distance=i32((u32(abs(delta))*u32(t)+u32(steps/2))/u32(steps));
    return origin+select(-distance,distance,delta>=0);
}
// Shared line-brush membership, used once per pixel for polygon edge unions.
fn line_contains(x:i32,y:i32,ox:i32,oy:i32,delta:i32,steps:i32,vertical:bool,direction:i32,pw:i32,ph:i32)->bool {
    var lo=0;var hi=0;
    if vertical {lo=max(0,y-ph+1-oy);hi=min(steps,y-oy);}
    else if direction>0 {lo=max(0,x-pw+1-ox);hi=min(steps,x-ox);}
    else {lo=max(0,ox-x);hi=min(steps,ox-x+pw-1);}
    if pw<=0 || ph<=0 || lo>hi {return false;}
    let origin=select(oy,ox,vertical);let a=line_minor(origin,delta,steps,lo);let b=line_minor(origin,delta,steps,hi);
    let coordinate=select(y,x,vertical);let size=select(ph,pw,vertical);
    return coordinate>=min(a,b) && coordinate<max(a,b)+size;
}
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
    if in_region && draw_kind()!=0 && x>=p[3] && y>=p[4] && x<p[5] && y<p[6] &&
       x>=p[7] && y>=p[8] && x<p[9] && y<p[10] {
        var src=u32(p[11]);
        let row=u32(p[16+(y & 7)]);
        let pattern_on=(row & (128u >> u32(x & 7)))!=0u;
        if !pattern_on { src=u32(p[12]); }
        var valid=true;
        if draw_kind()==2 {
            let sx=x-p[3]+p[14]; let sy=y-p[4]+p[15];
            if sx>=0 && sy>=0 && sx<w && sy<h { src=source[u32(sy*w+sx)]; }
            else { valid=false; }
        }
        if draw_kind()==4 {
            let sx=p[14]+((x-p[3])*(p[18]-p[14]))/(p[5]-p[3]);
            let sy=p[15]+((y-p[4])*(p[19]-p[15]))/(p[6]-p[4]);
            var bit=0u;
            if color_target()==0 && p[22]==0 && ((p[18]-p[14])>(p[5]-p[3]) || (p[19]-p[15])>(p[6]-p[4])) {
                // Basic QuickDraw reductions OR the source footprint. Enlargement
                // still samples a single source pixel along the enlarged axis.
                let ex=max(sx+1,p[14]+((x-p[3]+1)*(p[18]-p[14]))/(p[5]-p[3]));
                let ey=max(sy+1,p[15]+((y-p[4]+1)*(p[19]-p[15]))/(p[6]-p[4]));
                let lx=max(0,sx); let ly=max(0,sy);
                let hx=min(p[16],ex); let hy=min(p[17],ey);
                valid=lx<hx && ly<hy;
                for (var yy=ly; yy<hy && bit==0u; yy=yy+1) {
                    for (var xx=lx; xx<hx && bit==0u; xx=xx+1) {
                        bit=bit | blit_bit(source[u32(yy*p[16]+xx)]);
                    }
                }
            } else if sx>=0 && sy>=0 && sx<p[16] && sy<p[17] {
                let sample=source[u32(sy*p[16]+sx)];bit=select(blit_bit(sample),sample,color_target()!=0);
            } else { valid=false; }
            if p[22]!=0 {
                // Masked copies use the legacy reference's nearest-neighbor
                // source/mask pair; ordinary Basic CopyBits keeps OR reduction.
                let q=p[22];let mx=p[q+2]+sx-p[14];let my=p[q+3]+sy-p[15];
                if mx<0 || my<0 || mx>=p[q] || my>=p[q+1] {valid=false;}
                else {valid=valid && p[q+4+my*p[q]+mx]!=0;}
            }
            if color_target()==0{bit=bit ^ u32(p[20]);src=select(u32(p[12]),u32(p[11]),bit!=0u);}else{src=bit;}
        }
        if draw_kind()==5 {
            let q=25+4*max(p[24],0);
            valid=line_contains(x,y,p[14],p[15],p[q],p[q+1],p[q+2]!=0,p[q+3],p[q+4],p[q+5]);
        }
        if draw_kind()==6 {
            let q=25+4*max(p[24],0);let count=p[q];let pw=p[q+1];let ph=p[q+2];let lx=x-p[14];let ly=y-p[15];
            valid=false;
            if pw<0 && count>=3 {
                // Even-odd fill at integer guest coordinates, matching the legacy
                // reference sampling rule. Products stay signed-32-bit
                // because host validation limits both geometric extents to 32767.
                for (var i=0;i<count;i=i+1) {
                    let a=q+3+2*i;let b=q+3+2*((i+1)%count);
                    let x0=p[a];let y0=p[a+1];let x1=p[b];let y1=p[b+1];
                    let yy=ly;
                    if (y0>yy)!=(y1>yy) {
                        let left=(lx-x0)*(y1-y0);let right=(yy-y0)*(x1-x0);
                        if select((left > right),(left < right),(y1 > y0)) {valid=!valid;}
                    }
                }
            } else if pw>=0 && count>=2 {
                for (var i=0;i<count && !valid;i=i+1) {
                    let a=q+3+2*i;let b=q+3+2*((i+1)%count);
                    var sx=p[a];var sy=p[a+1];var ex=p[b];var ey=p[b+1];
                    if sx==ex && sy==ey {continue;}
                    if sy>ey {let tx=sx;let ty=sy;sx=ex;sy=ey;ex=tx;ey=ty;}
                    let dh=abs(ex-sx);let dv=ey-sy;let vertical=dv>=dh;
                    valid=line_contains(lx,ly,sx,sy,select(dv,ex-sx,vertical),max(dh,dv),vertical,select(-1,1,ex>=sx),pw,ph);
                }
            }
        }
        if valid && color_target()!=0 {value=color_result(src,value,pattern_on);}
        if valid && color_target()==0 { switch p[13] {
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
    if draw_kind()==3 {
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
