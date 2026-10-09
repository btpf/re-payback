"""Render Payback Icons/* to PNG. Format: headerless 8bpp chunky, rows of stride
= filesize/h (width rounded up to 16), visible width w (dims from FUN_010138dc).
Writes out/img/icons/*.png and contact sheets. usage: python tools/re/render_icons.py"""
import os,glob,numpy as np
from PIL import Image
from common import GAME, OUT as OUTROOT, game_palette
G=str(GAME)+'/'
OUT=str(OUTROOT/'img'/'icons')+'/'
os.makedirs(OUT,exist_ok=True)
pal=np.frombuffer(game_palette(),np.uint8).reshape(256,3)
# (glob, w, h, filesize)
DIMS=[('Cursor',6,10,160),('Heart.*',9,10,160),('Checkpoint.*',54,54,3456),
 ('Big.*',23,22,704),('You.*',6,10,160),('Them.*',6,10,160),
 ('LeftArrow',13,10,160),('RightArrow',13,10,160),
 ('LoadingPiece*',6,16,256),('PaybackLogo',166,80,14080),('Blob.*',7,7,112),
 ('Lights.*',37,20,960),('Phone',25,38,1216),('Lips',25,38,1216),('Mobile',25,38,1216),
 ('Pager',25,38,1216),('Info',25,38,1216),('BigPager.?',7,7,112),('BigPager.image',38,38,1824),
 ('Small.*',9,13,208),('Siren*',9,13,208),('*.Normal',9,20,320),('*.Bright',9,20,320),
 ('Cup.*',15,20,320),('Loading',94,35,3360),('TitleScreen',320,200,64000),
 ('TitleScreen.medium',320,256,81920),('TitleScreen.big',640,480,307200)]
def load(path,w,h,size):
    b=open(path,'rb').read(); b=b+bytes(max(0,size-len(b)))
    stride=size//h
    a=np.frombuffer(b[:size],np.uint8).reshape(h,stride)[:,:w]
    return a
def rgba(a):
    im=np.zeros(a.shape+(4,),np.uint8); im[...,:3]=pal[a]; im[...,3]=np.where(a==0,0,255); return im
sheets={}
for pat,w,h,size in DIMS:
    for p in sorted(glob.glob(G+'Icons/'+pat)):
        n=os.path.basename(p)
        a=load(p,w,h,size)
        Image.fromarray(rgba(a)).save(OUT+n+'.png')
        sheets.setdefault(pat,[]).append((n,a))
# contact sheet per group, scaled x4 on dark grey
for pat,items in sheets.items():
    if len(items)<2: continue
    h=max(a.shape[0] for _,a in items); w=max(a.shape[1] for _,a in items)
    cols=min(len(items),16); rows=(len(items)+cols-1)//cols
    S=np.full((rows*(h+2),cols*(w+2),3),40,np.uint8)
    for i,(n,a) in enumerate(items):
        r,c=divmod(i,cols); rg=rgba(a); m=rg[...,3]>0
        blk=S[r*(h+2):r*(h+2)+a.shape[0],c*(w+2):c*(w+2)+a.shape[1]]; blk[m]=rg[...,:3][m]
    Image.fromarray(S).resize((S.shape[1]*4,S.shape[0]*4),Image.NEAREST).save(OUT+'sheet_'+pat.replace('*','X').replace('?','Q')+'.png')
print('ok')
