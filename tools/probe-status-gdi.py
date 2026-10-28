"""Bounded, windowless status-BMP color conversion experiment (modern Windows).

Reference path: 0x423430 uses 16 bpp for the opening video, then calls
0x457880(640,480,8) for the game. 0x45cf40 creates an offscreen surface with
flags 7 (no explicit pixel format), caps 0x40 for the HP bitmaps, and no
individual palette. 0x45d150 uses LoadImageA(0x2010), a compatible source DC
and StretchBlt(SRCCOPY) into the DirectDraw surface DC.

This probe does NOT change display mode or create a visible window. Its DIB
and explicitly 8-bit DirectDraw surfaces are comparisons, not a reproduction
of a Windows 9x full-screen primary surface. Microsoft documents shared device
colors for a palette-less DC when both display and surface are 8-bit:
https://learn.microsoft.com/en-us/windows/win32/api/ddrawgdi/nf-ddrawgdi-ddgetdc
That Windows 2000+ contract does not establish the exact historical driver
mapping. Outputs remain diagnostic files under output/, not fidelity fixtures.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib, json, struct, platform
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
user=c.WinDLL('user32',use_last_error=True); gdi=c.WinDLL('gdi32',use_last_error=True)
user.LoadImageA.argtypes=[w.HINSTANCE,c.c_char_p,w.UINT,c.c_int,c.c_int,w.UINT];user.LoadImageA.restype=w.HANDLE
gdi.CreateCompatibleDC.argtypes=[w.HDC];gdi.CreateCompatibleDC.restype=w.HDC
gdi.SelectObject.argtypes=[w.HDC,w.HGDIOBJ];gdi.SelectObject.restype=w.HGDIOBJ
gdi.CreateDIBSection.argtypes=[w.HDC,c.c_void_p,w.UINT,c.POINTER(c.c_void_p),w.HANDLE,w.DWORD];gdi.CreateDIBSection.restype=w.HBITMAP
gdi.StretchBlt.argtypes=[w.HDC,c.c_int,c.c_int,c.c_int,c.c_int,w.HDC,c.c_int,c.c_int,c.c_int,c.c_int,w.DWORD];gdi.StretchBlt.restype=w.BOOL
gdi.GetDIBColorTable.argtypes=[w.HDC,w.UINT,w.UINT,c.c_void_p];gdi.GetDIBColorTable.restype=w.UINT
gdi.DeleteObject.argtypes=[w.HGDIOBJ];gdi.DeleteDC.argtypes=[w.HDC]
manifest=json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'));objects={o['sha256']:o for o in manifest['objects']}
def resource(name):return ROOT/objects[manifest['resources']['install/bmp/'+name+'.bmp']['sha256']]['path']
background=resource('pm06g01').read_bytes()[54:1078]
def dib(width,height,palette):
    bits=c.c_void_p();header=c.create_string_buffer(struct.pack('<IiiHHIIiiII',40,width,-height,1,8,0,0,0,0,256,0)+palette)
    bmp=gdi.CreateDIBSection(None,header,0,c.byref(bits),None,0)
    if not bmp:raise c.WinError(c.get_last_error())
    dc=gdi.CreateCompatibleDC(None);old=gdi.SelectObject(dc,bmp)
    return bmp,dc,old,bits
def indices(pointer,width,height,pitch):
    return b''.join(c.string_at(pointer+y*pitch,width)for y in range(height))
def stats(data,original,palette):
    colors=bytes(channel for index in data for channel in palette[4*index:4*index+3])
    return {'indicesSha256':hashlib.sha256(data).hexdigest(),'unchangedIndices':sum(a==b for a,b in zip(data,original)),
        'pixels':len(data),'nonblackPixels':sum(any(palette[4*i:4*i+3])for i in data),
        'distinctIndices':sorted(set(data)),'bgrSha256':hashlib.sha256(colors).hexdigest()}
result={'platform':platform.platform(),'warning':'Modern Windows windowless GDI experiment; no display-mode change and no historical DD claim','cases':[]}
for name in ['pm06a03','pm06a04','pm06a05']:
    path=resource(name);raw=path.read_bytes();width,height=struct.unpack_from('<ii',raw,18);height=abs(height);pitch=(width+3)&~3
    offset=struct.unpack_from('<I',raw,10)[0];original=b''.join(raw[offset+y*pitch:offset+y*pitch+width]for y in reversed(range(height)))
    source=user.LoadImageA(None,str(path).encode('mbcs'),0,0,0,0x2010);srcdc=gdi.CreateCompatibleDC(None);old=gdi.SelectObject(srcdc,source)
    row={'resource':name,'width':width,'height':height,'source':stats(original,original,raw[54:1078]),'variants':{}}
    for label,palette in [('samePalette',raw[54:1078]),('statusBackgroundPalette',background)]:
        bmp,dc,previous,bits=dib(width,height,palette)
        if not gdi.StretchBlt(dc,0,0,width,height,srcdc,0,0,width,height,0xcc0020):raise c.WinError(c.get_last_error())
        gdi.GdiFlush()
        row['variants'][label]=stats(indices(bits.value,width,height,pitch),original,palette)
        gdi.SelectObject(dc,previous);gdi.DeleteObject(bmp);gdi.DeleteDC(dc)
    gdi.SelectObject(srcdc,old);gdi.DeleteObject(source);gdi.DeleteDC(srcdc);result['cases'].append(row)
print(json.dumps(result,indent=2))
(ROOT/'output/status-gdi-probe.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')

# Do not change display mode. An explicit 8-bit system-memory surface lets us
# check whether a windowless modern DirectDraw DC can expose the required path.
class Desc(c.Structure):
    _fields_=[('size',w.DWORD),('flags',w.DWORD),('height',w.DWORD),('width',w.DWORD),('pitch',w.LONG),
        ('backbuffers',w.DWORD),('refresh',w.DWORD),('alpha',w.DWORD),('reserved',w.DWORD),('surface',c.c_void_p),
        ('keys',w.DWORD*8),('pixel_format',w.DWORD*8),('caps',w.DWORD)]
def method(obj,index,*args):
    table=c.cast(obj,c.POINTER(c.POINTER(c.c_void_p))).contents
    return c.WINFUNCTYPE(c.c_int32,c.c_void_p,*args)(table[index])
ddraw=c.WinDLL('ddraw',use_last_error=True)
ddraw.DirectDrawCreate.argtypes=[c.c_void_p,c.POINTER(c.c_void_p),c.c_void_p];ddraw.DirectDrawCreate.restype=c.c_int32
dd=c.c_void_p();dd_result={'warning':'Explicit 8-bit offscreen surfaces under current desktop mode, not original fullscreen/video-memory environment; pixel colors below are evaluated with the unmodified status-background palette','cases':[]}
hr=ddraw.DirectDrawCreate(None,c.byref(dd),None);dd_result['create']=hex(hr&0xffffffff)
if hr==0:
    try:
        hr=method(dd,20,w.HWND,w.DWORD)(dd,None,8);dd_result['normalCooperativeLevel']=hex(hr&0xffffffff)
        for caps,attached in [(caps,attached)for caps in [0x40,0x840]for attached in [False,True]]:
            surface=c.c_void_p();palette=c.c_void_p();desc=Desc();desc.size=c.sizeof(Desc);desc.flags=0x1007;desc.width=102;desc.height=9;desc.caps=caps
            desc.pixel_format[0]=32;desc.pixel_format[1]=0x60;desc.pixel_format[3]=8
            row={'surfaceCaps':hex(caps),'attachedStatusPalette':attached};dd_result['cases'].append(row)
            hr=method(dd,6,c.POINTER(Desc),c.POINTER(c.c_void_p),c.c_void_p)(dd,c.byref(desc),c.byref(surface),None);row['createSurface']=hex(hr&0xffffffff)
            if hr!=0:continue
            try:
                if attached:
                    entries=c.create_string_buffer(bytes(v for i in range(256)for v in [background[4*i+2],background[4*i+1],background[4*i],0]))
                    hr=method(dd,5,w.DWORD,c.c_void_p,c.POINTER(c.c_void_p),c.c_void_p)(dd,0x44,entries,c.byref(palette),None);row['createPalette']=hex(hr&0xffffffff)
                    if hr!=0:continue
                    row['setPalette']=hex(method(surface,31,c.c_void_p)(surface,palette)&0xffffffff)
                dc=w.HDC();hr=method(surface,17,c.POINTER(w.HDC))(surface,c.byref(dc));row['getDC']=hex(hr&0xffffffff)
                if hr!=0:continue
                path=resource('pm06a05');source=user.LoadImageA(None,str(path).encode('mbcs'),0,0,0,0x2010);srcdc=gdi.CreateCompatibleDC(None);old=gdi.SelectObject(srcdc,source)
                try:
                    palette_data=(c.c_ubyte*1024)();row['dcPaletteCount']=gdi.GetDIBColorTable(dc,0,256,palette_data)
                    row['dcPaletteSha256']=hashlib.sha256(bytes(palette_data)).hexdigest()
                    row['stretchBlt']=bool(gdi.StretchBlt(dc,0,0,102,9,srcdc,0,0,102,9,0xcc0020));gdi.GdiFlush()
                finally:
                    gdi.SelectObject(srcdc,old);gdi.DeleteObject(source);gdi.DeleteDC(srcdc);method(surface,26,w.HDC)(surface,dc)
                locked=Desc();locked.size=c.sizeof(Desc)
                hr=method(surface,25,c.c_void_p,c.POINTER(Desc),w.DWORD,w.HANDLE)(surface,None,c.byref(locked),0x11,None);row['lock']=hex(hr&0xffffffff)
                if hr==0:
                    raw=path.read_bytes();original=b''.join(raw[1078+y*104:1078+y*104+102]for y in reversed(range(9)))
                    row['pixels']=stats(indices(locked.surface,102,9,locked.pitch),original,background)
                    method(surface,32,c.c_void_p)(surface,None)
            finally:
                method(surface,2)(surface)
                if palette:method(palette,2)(palette)
    finally:method(dd,2)(dd)
print(json.dumps(dd_result,indent=2))
(ROOT/'output/status-ddraw-probe.json').write_text(json.dumps(dd_result,indent=2)+'\n',encoding='utf8')
