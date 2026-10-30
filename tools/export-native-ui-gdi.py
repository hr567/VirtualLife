"""Observe native BMP-to-8-bit-surface translations at their construction palettes.

FUN_004589c0 -> 0045cf40 LoadImageA(0x2010) -> 0045d150 StretchBlt.
This is the same fixed modern-Windows GDI baseline as export-status-gdi.py;
it does not claim a Win9x display capture. No window or display mode is changed.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib,json,platform,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
if sys.platform!='win32':raise SystemExit('Re-observation requires Windows GDI; normal builds use the saved fixture.')
manifest=json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'))
objects={v['sha256']:v for v in manifest['objects']}
native=json.loads((ROOT/'assets/catalog/native-ui.json').read_text(encoding='utf8'))
school=json.loads((ROOT/'assets/catalog/native-school.json').read_text(encoding='utf8'))
rules=json.loads((ROOT/'assets/catalog/rules.json').read_text(encoding='utf8'))
digest=lambda v:hashlib.sha256(v).hexdigest()
def fnv32(data):
    result=2166136261
    for value in data:result=((result^value)*16777619)&0xffffffff
    return result
def path(resource):return ROOT/objects[manifest['resources'][resource]['sha256']]['path']
def palette(resource):
    raw=path(resource).read_bytes()
    out=bytearray([0,0,0,255]*256)
    if raw[:2]==b'BM':
        start=14+struct.unpack_from('<I',raw,14)[0];count=struct.unpack_from('<I',raw,46)[0]or 256
        for i in range(count):b,g,r,_=raw[start+i*4:start+i*4+4];out[i*4:i*4+4]=bytes([r,g,b,255])
    else:
        start=128
        while struct.unpack_from('<H',raw,start+4)[0]!=0xf1fa:start+=struct.unpack_from('<I',raw,start)[0]
        count=struct.unpack_from('<H',raw,start+6)[0];start+=16
        for _ in range(count):
            size,kind=struct.unpack_from('<IH',raw,start);cursor=start+6
            if kind in (4,11):
                packets=struct.unpack_from('<H',raw,cursor)[0];cursor+=2;index=0
                for _ in range(packets):
                    index+=raw[cursor];colors=raw[cursor+1]or 256;cursor+=2
                    assert index+colors<=256
                    for _ in range(colors):
                        rgb=raw[cursor:cursor+3];cursor+=3
                        out[index*4:index*4+3]=bytes(v<<2 if kind==11 else v&0xfc for v in rgb);index+=1
            start+=size
    return out
user=c.WinDLL('user32',use_last_error=True);gdi=c.WinDLL('gdi32',use_last_error=True)
user.LoadImageA.argtypes=[w.HINSTANCE,c.c_char_p,w.UINT,c.c_int,c.c_int,w.UINT];user.LoadImageA.restype=w.HANDLE
gdi.CreateCompatibleDC.argtypes=[w.HDC];gdi.CreateCompatibleDC.restype=w.HDC
gdi.SelectObject.argtypes=[w.HDC,w.HGDIOBJ];gdi.SelectObject.restype=w.HGDIOBJ
gdi.CreateDIBSection.argtypes=[w.HDC,c.c_void_p,w.UINT,c.POINTER(c.c_void_p),w.HANDLE,w.DWORD];gdi.CreateDIBSection.restype=w.HBITMAP
gdi.StretchBlt.argtypes=[w.HDC,c.c_int,c.c_int,c.c_int,c.c_int,w.HDC,c.c_int,c.c_int,c.c_int,c.c_int,w.DWORD];gdi.StretchBlt.restype=w.BOOL
gdi.DeleteObject.argtypes=[w.HGDIOBJ];gdi.DeleteDC.argtypes=[w.HDC]
def translate(resource,target):
    raw=path(resource).read_bytes();width,height=struct.unpack_from('<ii',raw,18);assert height>0 and struct.unpack_from('<HI',raw,28)==(8,0)
    pitch=(width+3)&~3;start=struct.unpack_from('<I',raw,10)[0]
    original=b''.join(raw[start+y*pitch:start+y*pitch+width]for y in reversed(range(height)))
    source=user.LoadImageA(None,str(path(resource)).encode('mbcs'),0,0,0,0x2010)
    if not source:raise c.WinError(c.get_last_error())
    source_dc=gdi.CreateCompatibleDC(None);old_source=gdi.SelectObject(source_dc,source)
    bgr=bytes(v for i in range(256)for v in [target[i*4+2],target[i*4+1],target[i*4],0])
    info=c.create_string_buffer(struct.pack('<IiiHHIIiiII',40,width,-height,1,8,0,0,0,0,256,0)+bgr)
    bits=c.c_void_p();destination=gdi.CreateDIBSection(None,info,0,c.byref(bits),None,0)
    if not destination:raise c.WinError(c.get_last_error())
    destination_dc=gdi.CreateCompatibleDC(None);old_destination=gdi.SelectObject(destination_dc,destination)
    try:
        if not gdi.StretchBlt(destination_dc,0,0,width,height,source_dc,0,0,width,height,0xcc0020):raise c.WinError(c.get_last_error())
        gdi.GdiFlush();mapped=b''.join(c.string_at(bits.value+y*pitch,width)for y in range(height))
        mapping={}
        for before,after in zip(original,mapped):
            assert before not in mapping or mapping[before]==after,'Translation is not index-deterministic'
            mapping[before]=after
        # FUN_00458b20 sets the numeric destination surface color key after
        # StretchBlt. It is not necessarily the source BMP's index 1.
        rgba=bytes(v for after in mapped for v in [*target[after*4:after*4+3],0 if after==1 else 255])
        black=bytes(v for after in mapped for v in ([0,0,0,255]if after==1 else list(target[after*4:after*4+4])))
        return {'sourceSha256':digest(raw),'indexMap':[[a,b]for a,b in sorted(mapping.items())],
                'width':width,'height':height,'destinationColorKey':1,
                'mappedIndicesSha256':digest(mapped),'rgbaSha256':digest(rgba),'colorKeyedBlackRgbaFnv32':fnv32(black)}
    finally:
        gdi.SelectObject(destination_dc,old_destination);gdi.DeleteObject(destination);gdi.DeleteDC(destination_dc)
        gdi.SelectObject(source_dc,old_source);gdi.DeleteObject(source);gdi.DeleteDC(source_dc)
def resources(value):
    if isinstance(value,dict):
        if 'resource'in value and value['resource'].endswith('.bmp'):yield value['resource']
        for v in value.values():yield from resources(v)
    elif isinstance(value,list):
        for v in value:yield from resources(v)
def resolve(resource):return resource if resource in manifest['resources']else'install/'+resource
contexts={}
for kind in ['statShop','itemShop','equipmentWorkshop','career','school']:
    meta=school if kind=='school'else native[kind]
    background=meta['background']['resource']if kind in ['career','school']else meta['layers'][0]['resource']
    target=palette(background)
    # Original BMPs are constructed before 456dd0 installs FLC palettes. Do
    # not use the final display palette to emulate the earlier StretchBlt.
    sections=[v for v in meta.get('paletteSections',[])if v['resource'].endswith('.bmp')]
    for section in sections:
        colors=palette(section['resource']);a=section['first']*4;b=(section['last']+1)*4;target[a:b]=colors[a:b]
    images=set(resources(meta))
    if kind!='school':images.update(resources(native['common']))
    if kind=='statShop':images.update(resolve(v['resource'])for stage in ['young','adult']for group in rules['statShops'][stage]for v in group)
    contexts[kind]={'targetPaletteSha256':digest(target),'targetPalette':list(target),
                    'constructionSections':sections,'images':{r:translate(r,target)for r in sorted(images)}}
# FUN_0043cf70 creates the research icon buttons after FUN_0043cb10 and
# the initial yes animation palette (43d2c6), before research FLC 2..161.
meta=native['equipmentWorkshop'];target=bytearray(contexts['equipmentWorkshop']['targetPalette'])
yes=next(v for v in meta['paletteSections']if v['resource'].endswith('.flc'))
colors=palette(yes['resource']);target[yes['first']*4:(yes['last']+1)*4]=colors[yes['first']*4:(yes['last']+1)*4]
contexts['equipmentResearch']={'targetPaletteSha256':digest(target),'targetPalette':list(target),
    'constructionSections':[*contexts['equipmentWorkshop']['constructionSections'],yes],
    'images':{r:translate(r,target)for r in sorted(set(resolve(v['icon'])for v in rules['equipment']))}}
exe=(ROOT/'research/original/N2Game.exe').read_bytes();assert digest(exe)==native['sourceSha256']
pe=struct.unpack_from('<I',exe,60)[0];section=pe+24+struct.unpack_from('<H',exe,pe+20)[0]
base=struct.unpack_from('<I',exe,pe+52)[0]
def code(va,size):
    rva=va-base
    for i in range(struct.unpack_from('<H',exe,pe+6)[0]):
        at=section+i*40;start=struct.unpack_from('<I',exe,at+12)[0]
        if start<=rva<start+max(struct.unpack_from('<I',exe,at+8)[0],struct.unpack_from('<I',exe,at+16)[0]):
            offset=struct.unpack_from('<I',exe,at+20)[0]+rva-start
            return {'address':f'0x{va:08x}','bytes':exe[offset:offset+size].hex()}
    raise ValueError('Unmapped native source instruction')
observed={'formatVersion':1,'sourceSha256':native['sourceSha256'],'platform':platform.platform(),
    'method':'Original LoadImageA(0x2010) -> StretchBlt(SRCCOPY) into 8-bit DIB with construction-time primary palette; later FLC palette updates are separate.',
    'evidence':['0x004589c0','0x0045cf40','0x0045d150','0x00456d30','0x00456dd0'],
    'sourceInstructions':[code(va,size)for va,size in [(0x456d30,149),(0x456dd0,90),(0x458b20,45),(0x42c9c0,143),(0x43b640,73),(0x43d260,109),(0x43f189,56)]],
    'contexts':contexts}
destination=ROOT/'tests/fixtures/original-native-ui-gdi.json'
if '--check'in sys.argv:
    if json.loads(destination.read_text(encoding='utf8'))!=observed:raise SystemExit('Native UI GDI observations differ from this Windows baseline')
else:destination.write_text(json.dumps(observed,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'contexts':len(contexts),'images':sum(len(v['images'])for v in contexts.values()),'nonIdentityImages':sum(any(a!=b for a,b in row['indexMap'])for v in contexts.values()for row in v['images'].values())}))
