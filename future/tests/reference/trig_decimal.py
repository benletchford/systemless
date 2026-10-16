# Independent reference generation; Python standard library only.
from decimal import Decimal as D, localcontext, ROUND_FLOOR, ROUND_HALF_EVEN

def calculate(extra):
 out=[]
 for exponent,mantissa in [(0x3fff,1<<63),(0xc000,1<<63),(0x403f,1<<63),(0x43e7,1<<63),(0x3fff,(1<<63)+1)]:
  power=(exponent&0x7fff)-16383
  with localcontext() as c:
   c.prec=max(120,int(abs(power)*.302)+extra)
   def atan_inverse(n):
    x=D(1)/n; total=term=x; k=1
    while True:
     term=-term/(n*n); new=total+term/(2*k+1)
     if new==total:return total
     total=new;k+=1
   pi=16*atan_inverse(5)-4*atan_inverse(239)
   x=D(mantissa)*D(2)**(power-63)
   if exponent&0x8000:x=-x
   x=x%(2*pi)
   if x>pi:x-=2*pi
   if x< -pi:x+=2*pi
   sin=term=x;k=1
   while True:
    term=-term*x*x/((2*k)*(2*k+1));new=sin+term
    if new==sin:break
    sin=new;k+=1
   cos=term=D(1);k=1
   while True:
    term=-term*x*x/((2*k-1)*(2*k));new=cos+term
    if new==cos:break
    cos=new;k+=1
   for name,y in [('Sin',sin),('Cos',cos),('Tan',sin/cos)]:
    e=int((abs(y).ln()/D(2).ln()).to_integral_value(rounding=ROUND_FLOOR))
    m=int((abs(y)/D(2)**e*D(2)**63).to_integral_value(rounding=ROUND_HALF_EVEN))
    if m==1<<64:m>>=1;e+=1
    out.append((name,exponent,mantissa,(0x8000 if y<0 else 0)|(e+16383),m))
 return out
first=calculate(110);second=calculate(180);assert first==second
for name,e,m,oe,om in first:print(f'(Unary::{name}, 0x{e:04x}, 0x{m:016x}, 0x{oe:04x}, 0x{om:016x}),')
