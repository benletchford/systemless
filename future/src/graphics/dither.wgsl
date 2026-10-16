// Deterministic left-to-right Floyd–Steinberg diffusion, matching the preserved
// GWorld conversion reference. The output remains on-device for the transfer pass.
@group(0) @binding(0) var<storage,read_write> output:array<u32>;
@group(0) @binding(1) var<storage,read> source:array<u32>;
@group(0) @binding(2) var<storage,read_write> errors:array<i32>;
@group(0) @binding(3) var<storage,read> p:array<i32>;
fn closest(rgb:vec3<u32>)->u32 {
    var best=0u;var best_hi=0xffffffffu;var best_lo=0xffffffffu;
    for(var i=0;i<p[8];i=i+1){let at=9+4*i;if p[at+3]==0{continue;}
        let color=vec3<u32>(u32(p[at]),u32(p[at+1]),u32(p[at+2]));
        let d=max(rgb,color)-min(rgb,color);let square=d*d;
        let a=square.x+square.y;var hi=select(0u,1u,a<square.x);let lo=a+square.z;hi=hi+select(0u,1u,lo<a);
        if hi<best_hi || (hi==best_hi && lo<best_lo){best=u32(i);best_hi=hi;best_lo=lo;}
    }return best;
}
@compute @workgroup_size(1)
fn main(){
    let w=p[0];let h=p[1];let stride=(w+2)*3;
    for(var y=0;y<h;y=y+1){
        let current=(y&1)*stride;let next=(1-(y&1))*stride;
        let sy=p[5]+y*(p[7]-p[5])/h;
        for(var x=0;x<w;x=x+1){
            let sx=p[4]+x*(p[6]-p[4])/w;let out_at=(y*w+x)*3;
            if sx<0 || sy<0 || sx>=p[2] || sy>=p[3]{
                output[out_at]=65536u;output[out_at+1]=0u;output[out_at+2]=0u;continue;
            }
            let src=(sy*p[2]+sx)*3;let e=(x+1)*3;
            let adjusted=vec3<u32>(clamp(vec3<i32>(i32(source[src]),i32(source[src+1]),i32(source[src+2]))+
                vec3<i32>(errors[current+e],errors[current+e+1],errors[current+e+2]),vec3<i32>(0),vec3<i32>(65535)));
            let index=closest(adjusted);let palette=9+4*i32(index);
            for(var c=0;c<3;c=c+1){
                let value=u32(p[palette+c]);output[out_at+c]=value;
                let error=i32(adjusted[c])-i32(value);
                errors[current+e+3+c]+=error*7/16;
                errors[next+e-3+c]+=error*3/16;
                errors[next+e+c]+=error*5/16;
                errors[next+e+3+c]+=error/16;
            }
        }
        for(var i=0;i<stride;i=i+1){errors[current+i]=0;}
    }
}
