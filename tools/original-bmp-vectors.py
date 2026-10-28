"""Record the native LoadImageA(0x2010) BMP palette expansion, without a window."""
import ctypes as c
from ctypes import wintypes as w
import hashlib, json, platform, struct
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
user, gdi = c.WinDLL('user32', use_last_error=True), c.WinDLL('gdi32', use_last_error=True)
user.LoadImageA.argtypes = [w.HINSTANCE, c.c_char_p, w.UINT, c.c_int, c.c_int, w.UINT]
user.LoadImageA.restype = w.HANDLE
gdi.CreateCompatibleDC.argtypes = [w.HDC]; gdi.CreateCompatibleDC.restype = w.HDC
gdi.SelectObject.argtypes = [w.HDC,w.HGDIOBJ]; gdi.SelectObject.restype = w.HGDIOBJ
gdi.GetDIBColorTable.argtypes = [w.HDC,w.UINT,w.UINT,c.c_void_p]; gdi.GetDIBColorTable.restype = w.UINT
gdi.GetDIBits.argtypes = [w.HDC,w.HBITMAP,w.UINT,w.UINT,c.c_void_p,c.c_void_p,w.UINT]; gdi.GetDIBits.restype = c.c_int
gdi.DeleteObject.argtypes = [w.HGDIOBJ]; gdi.DeleteDC.argtypes = [w.HDC]
manifest = json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'))
objects = {o['sha256']:o for o in manifest['objects']}
cases = []
for name in ['install/pm0p/pm0p01.bmp','install/pm0p/pm0p05.bmp']:
    digest = manifest['resources'][name]['sha256']; path = ROOT/objects[digest]['path']; data = path.read_bytes()
    width,height = struct.unpack_from('<ii',data,18); count=abs(width*height)
    bitmap = user.LoadImageA(None,str(path).encode('mbcs'),0,0,0,0x2010)
    if not bitmap: raise c.WinError(c.get_last_error())
    dc = gdi.CreateCompatibleDC(None); previous = gdi.SelectObject(dc,bitmap)
    palette = (c.c_ubyte*1024)(); got = gdi.GetDIBColorTable(dc,0,256,palette)
    gdi.SelectObject(dc,previous)
    header = c.create_string_buffer(struct.pack('<IiiHHIIiiII',40,width,-abs(height),1,32,0,count*4,0,0,0,0))
    pixels = (c.c_ubyte*(count*4))()
    if gdi.GetDIBits(dc,bitmap,0,abs(height),pixels,header,0)!=abs(height): raise c.WinError(c.get_last_error())
    rgba = bytearray()
    for i in range(count): rgba.extend([pixels[4*i+2],pixels[4*i+1],pixels[4*i],255])
    fnv = 2166136261
    for byte in rgba: fnv=((fnv^byte)*16777619)&0xffffffff
    row = {'resource':name,'sha256':digest,'width':width,'height':abs(height),'paletteEntriesReturned':got,
        'palette255':list(palette[1020:1024]),'rgbaSha256':hashlib.sha256(rgba).hexdigest(),'rgbaFnv32':fnv}
    cases.append(row); print(json.dumps(row),flush=True)
    gdi.DeleteObject(bitmap);gdi.DeleteDC(dc)
result = {'method':'LoadImageA flags 0x2010, as called by N2Game.exe 0x45cf40 / 0x45d0c0; GetDIBColorTable and GetDIBits. Modern Windows GDI compatibility sample; not a Win9x capture.',
    'os':platform.platform(),'cases':cases}
(ROOT/'tests/fixtures/original-bmp.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')
