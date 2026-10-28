"""Export only Status HP bitmap translations observed through Windows GDI.

0x401550 creates these BMP surfaces after loading the background palette,
portrait entries 202..221, and Back animation entries 162..190. The game
requests an 8-bit display in 0x457880 after its 16-bit opening video.
0x45cf40/0x45d150 use LoadImageA(0x2010) and StretchBlt(SRCCOPY) to the
palette-less offscreen surface's DC. In an 8-bit display that DC shares the
device color table (Microsoft DdGetDC contract). We reproduce the color-table
translation with an 8-bit DIB section, without changing display mode or opening
a window. This is a modern Windows GDI baseline, not a Windows 9x capture.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import platform
import struct
import sys
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
if sys.platform!='win32':raise SystemExit('Status HP observation requires Windows GDI; normal builds use the saved catalog.')
manifest=json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'))
objects={row['sha256']:row for row in manifest['objects']}
rules_path=ROOT/'assets/catalog/rules.json'
rules=json.loads(rules_path.read_text(encoding='utf8'));status=rules['statusRendering']
user=c.WinDLL('user32',use_last_error=True);gdi=c.WinDLL('gdi32',use_last_error=True)
user.LoadImageA.argtypes=[w.HINSTANCE,c.c_char_p,w.UINT,c.c_int,c.c_int,w.UINT];user.LoadImageA.restype=w.HANDLE
gdi.CreateCompatibleDC.argtypes=[w.HDC];gdi.CreateCompatibleDC.restype=w.HDC
gdi.SelectObject.argtypes=[w.HDC,w.HGDIOBJ];gdi.SelectObject.restype=w.HGDIOBJ
gdi.CreateDIBSection.argtypes=[w.HDC,c.c_void_p,w.UINT,c.POINTER(c.c_void_p),w.HANDLE,w.DWORD];gdi.CreateDIBSection.restype=w.HBITMAP
gdi.StretchBlt.argtypes=[w.HDC,c.c_int,c.c_int,c.c_int,c.c_int,w.HDC,c.c_int,c.c_int,c.c_int,c.c_int,w.DWORD];gdi.StretchBlt.restype=w.BOOL
gdi.DeleteObject.argtypes=[w.HGDIOBJ];gdi.DeleteDC.argtypes=[w.HDC]


def path(resource):return ROOT/objects[manifest['resources'][resource]['sha256']]['path']
def digest(data):return hashlib.sha256(data).hexdigest()
def fnv32(data):
    result=2166136261
    for value in data:result=((result^value)*16777619)&0xffffffff
    return result


def palette(resource):
    data=path(resource).read_bytes();result=bytearray([0,0,0,255]*256)
    if data[:2]==b'BM':
        offset=14+struct.unpack_from('<I',data,14)[0];count=struct.unpack_from('<I',data,46)[0]or 256
        for i in range(count):
            b,g,r,_=data[offset+i*4:offset+i*4+4];result[i*4:i*4+4]=bytes([r,g,b,255])
    else:
        offset=128
        while struct.unpack_from('<H',data,offset+4)[0]!=0xf1fa:offset+=struct.unpack_from('<I',data,offset)[0]
        chunks=struct.unpack_from('<H',data,offset+6)[0];offset+=16
        for _ in range(chunks):
            size,kind=struct.unpack_from('<IH',data,offset);cursor=offset+6
            if kind in (4,11):
                packets=struct.unpack_from('<H',data,cursor)[0];cursor+=2;index=0
                for _ in range(packets):
                    index+=data[cursor];count=data[cursor+1]or 256;cursor+=2
                    assert index+count<=256
                    for _ in range(count):
                        rgb=data[cursor:cursor+3];cursor+=3
                        result[index*4:index*4+3]=bytes(value<<2 if kind==11 else value&0xfc for value in rgb);index+=1
            offset+=size
    return result


def translate(resource,target):
    source_path=path(resource);raw=source_path.read_bytes();width,height=struct.unpack_from('<ii',raw,18)
    assert height>0 and struct.unpack_from('<HI',raw,28)==(8,0)
    pitch=(width+3)&~3;offset=struct.unpack_from('<I',raw,10)[0]
    original=b''.join(raw[offset+y*pitch:offset+y*pitch+width]for y in reversed(range(height)))
    source=user.LoadImageA(None,str(source_path).encode('mbcs'),0,0,0,0x2010)
    if not source:raise c.WinError(c.get_last_error())
    source_dc=gdi.CreateCompatibleDC(None);source_old=gdi.SelectObject(source_dc,source)
    bgr=bytes(value for i in range(256)for value in [target[i*4+2],target[i*4+1],target[i*4],0])
    info=c.create_string_buffer(struct.pack('<IiiHHIIiiII',40,width,-height,1,8,0,0,0,0,256,0)+bgr)
    bits=c.c_void_p();destination=gdi.CreateDIBSection(None,info,0,c.byref(bits),None,0)
    if not destination:raise c.WinError(c.get_last_error())
    destination_dc=gdi.CreateCompatibleDC(None);destination_old=gdi.SelectObject(destination_dc,destination)
    try:
        if not gdi.StretchBlt(destination_dc,0,0,width,height,source_dc,0,0,width,height,0xcc0020):raise c.WinError(c.get_last_error())
        gdi.GdiFlush();mapped=b''.join(c.string_at(bits.value+y*pitch,width)for y in range(height))
        mapping={}
        for before,after in zip(original,mapped):
            assert before not in mapping or mapping[before]==after,'GDI translation requires per-pixel data'
            mapping[before]=after
        assert mapping.get(1,1)==1,'Native color-key index must remain 1'
        rgba=bytes(value for before,after in zip(original,mapped)for value in [*target[after*4:after*4+3],0 if before==1 else 255])
        return {'resource':resource,'sourceSha256':digest(raw),'width':width,'height':height,
            'indexMap':[[before,after]for before,after in sorted(mapping.items())],
            'mappedIndicesSha256':digest(mapped),'rgbaSha256':digest(rgba),'rgbaFnv32':fnv32(rgba)}
    finally:
        gdi.SelectObject(destination_dc,destination_old);gdi.DeleteObject(destination);gdi.DeleteDC(destination_dc)
        gdi.SelectObject(source_dc,source_old);gdi.DeleteObject(source);gdi.DeleteDC(source_dc)


background=palette(status['background']['resource']);back=palette(status['back'][0]['resource'])
variants=[];variant_ids={};portrait_variants=[]
for portrait in status['portraits']:
    target=background.copy();colors=palette(portrait['resource']);target[202*4:222*4]=colors[202*4:222*4];target[162*4:191*4]=back[162*4:191*4]
    target_hash=digest(target)
    if target_hash not in variant_ids:
        variant_ids[target_hash]=len(variants)
        variants.append({'targetPaletteSha256':target_hash,'images':{key:translate(status[key]['resource'],target)for key in ['hpLabel','hpFrame','hpFill']}})
    portrait_variants.append(variant_ids[target_hash])
observed={
    'method':'LoadImageA(0x2010) -> StretchBlt(SRCCOPY) into 8-bit DIB with the 0x401550 active palette; transparent source index 1',
    'platform':platform.platform(),'sourceSha256':rules['sourceSha256'],
    'contract':'https://learn.microsoft.com/en-us/windows/win32/api/ddrawgdi/nf-ddrawgdi-ddgetdc',
    'paletteEncoding':'RGBA8, alpha 255; background then portrait 202..221 then Back 162..190',
    'portraitVariants':portrait_variants,'variants':variants,
}
if '--check' in sys.argv:
    if status.get('hpGdi')!=observed:raise SystemExit('Saved Status HP GDI observations differ from this Windows GDI run.')
else:
    status['hpGdi']=observed
    rules_path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'portraits':len(portrait_variants),'uniquePalettes':len(variants),'hpBitmaps':len(variants)*3}))
