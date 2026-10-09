# Software-renderer mock of the single-player in-game HUD at 320x256 (lo-res, hires flag 0),
# following FUN_01019bd8 & friends (PaybackOS4). Palette-indexed framebuffer + General/Alpha.256.
from PIL import Image
from common import GAME, OUT, game_palette
G=str(GAME)+'/'
pal=list(game_palette())
A=open(G+'General/Alpha.256','rb').read()
W,H=320,256; XMAX,YMAX=W-1,H-1; CX=(XMAX>>1)+1
th=open(G+'Maps/0.thumb','rb').read()
fb=bytearray(th[y*256+((x+40)%256)] for y in range(H) for x in range(W))
def icon(n,w,h,size):
    d=open(G+'Icons/'+n,'rb').read(); d+=bytes(size-len(d)); s=size//h
    return (w,h,bytes(d[y*s+x] for y in range(h) for x in range(w)))
def blit(ic,x,y,table=None):     # table None: FUN_0100a53c (0 transparent); else FUN_01111280 (table[dst*256+src])
    w,h,p=ic
    for j in range(h):
        for i in range(w):
            X,Y=x+i,y+j
            if not(0<=X<W and 0<=Y<H): continue
            s=p[j*w+i]
            if table is None:
                if s: fb[Y*W+X]=s
            else: fb[Y*W+X]=A[table+fb[Y*W+X]*256+s]
def fx_quad(tex,x0,y0,x1,y1,table=0x10000):   # FUN_01033d50 software path, flipU=1 flipV=0
    t=open(G+'EffectTextures/'+tex,'rb').read()
    du=0x3fffff//((x1-x0)+1); dv=0xfc0//(y1-y0) if y1!=y0 else 0
    v=0
    for Y in range(y0,min(y1,YMAX)+1):
        u=0
        for X in range(x0,min(x1,XMAX)+1):
            s=t[(u>>16)+(v&0xffc0)]; fb[Y*W+X]=A[table+fb[Y*W+X]*256+s]; u+=du
        v+=dv
small=[icon('Small.%d'%i,9,13,208) for i in range(10)]
def text(s,x,y,font,table=0x30000,adv=7):
    for ch in s:
        if ch!=' ':
            n={'.':'FullStop',':':'Colon','!':'ExclamationMark','-':'Hyphen',',':'Comma',"'":'Apostrophe','?':'QuestionMark'}.get(ch)
            if n is None: n=ch if not ch.islower() else 'little_'+ch
            blit(icon(font+'.'+n,6,10,160),x,y,table)
        x+=adv
# --- vehicle-name banner FUN_01016224, t in [20,130): FadeDarken2 (cx-80,0)-(cx+80,16), text y=2 font1
name='Sports car'; fx_quad('FadeDarken2',CX-80,0,CX+80,16); text(name,CX-(7*len(name)>>1),2,'You')
# --- weapons FUN_01019490: fist(5,5); slots 15,25,..,75 at y=5; selected blinks normal/bright
wl=[('Fist',5),('Pistol',15),('Shotgun',25),('Flamethrower',35),('Grenade',45),('Minigun',55),('Bazooka',65),('Bomb',75)]
for n,x in wl:
    if n in('Flamethrower','Bomb'): continue          # no ammo -> slot left empty
    blit(icon(n+('.Bright' if n=='Pistol' else '.Normal'),9,20,320),x,5)
ammo=42
for k,div in enumerate([10000,1000,100,10,1]): blit(small[ammo//div%10],5+10*k,0x1a,0x30000)
# --- score FUN_010180b0(score, xmax-75, 5): 7 digits, 10px pitch, lighten table
score=1234567
for k in range(7): blit(small[score//10**(6-k)%10],XMAX-75+10*k,5,0x30000)
# --- multiplier (green-add table 0x70000) and lives
blit(icon('Small.x',9,13,208),XMAX-0x4b,0x13,0x70000); blit(small[2],XMAX-0x41,0x13,0x70000)
blit(icon('Small.Person',9,13,208),XMAX-0x23,0x13); blit(icon('Small.x',9,13,208),XMAX-0x19,0x13,0x30000); blit(small[3],XMAX-0xf,0x13,0x30000)
# --- hearts FUN_01018518(health,cx,31)
health=55
for i in range(5): blit(icon('Heart.Small' if health//20<i else 'Heart.Big',9,10,160),CX-25+10*i,0x1f,0x30000)
# --- wanted stars: n=3, spacing = siren.w+2 = 11, y=16, animation counter c (per frame)
n=3; sp=11; x0=CX-((n*sp)>>1); p=4
for i in range(n):
    blit(icon('Siren.%d'%(p+1 if p<=9 else 1),9,13,208),x0+i*sp,0x10); p+=2
    if p>29: p=0
# --- countdown timer (BigPager) FUN_010f699c()<1000: image (5,45), digits (13|21|29, 60)
t=123; blit(icon('BigPager.image',38,38,1824),5,0x2d)
for k,div in enumerate([100,10,1]): blit(icon('BigPager.%d'%(t//div%10),7,7,112),13+8*k,0x3c)
# --- start lights at (cx-19, H/4)
blit(icon('Lights.2',37,20,960),CX-0x13,H//4)
# --- minimap FUN_01018954(xmax-71, 33, px,py, tx,ty)
mx,my=XMAX-0x47,0x21
d=open(G+'Maps/0.mini','rb').read(); m=bytearray(d[y*65+x] for y in range(64) for x in range(64))
m[0x1f]=m[0x60]=m[0xa1]=m[0xe1]=0xb0; m[0x9d:0xa1]=b'\xb0'*4; m[0xdd:0xe1]=b'\xb0'*4; m[0x11d:0x121]=b'\x08'*4; m[0x5e:0x60]=b'\xb0\xb0'; m[0x121:0x123]=b'\x08\x08'
blit((64,64,bytes(m)),mx+1,my+1)
px,py=30<<12,40<<12; tx,ty=50<<12,20<<12     # world coords (square = 0x800 units)
blit(icon('Blob.0',7,7,112),(px>>12)+mx-3,(py>>12)+my-3); blit(icon('Blob.2',7,7,112),(tx>>12)+mx-3,(ty>>12)+my-3)
# --- phone/pager message box FUN_01016424 state 2: FadeDarken (0,ymax-41)-(xmax,ymax), icon (2,ymax-39), lines x=29
fx_quad('FadeDarken',0,YMAX-0x29,XMAX,YMAX); blit(icon('Pager',25,38,1216),2,YMAX-0x27)
text('Get to the phone box on',0x1d,YMAX-0x22,'Them',None); text('Main Street. Hurry!',0x1d,YMAX-0x22+11,'Them',None)
img=Image.new('P',(W,H)); img.putpalette(pal); img.putdata(bytes(fb))
(OUT/'img').mkdir(parents=True,exist_ok=True)
img.convert('RGB').resize((W*3,H*3),Image.NEAREST).save(OUT/'img'/'hud_mock_320x256.png')
print('ok')
