"""Export original narrated epilogue, credits layout, and reincarnation choice."""
import ctypes,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
pe=pefile.PE(str(ROOT/'research/original/N2Game.exe'));base=pe.OPTIONAL_HEADER.ImageBase;memory=pe.get_memory_mapped_image()
def read(address,fmt):return struct.unpack_from(fmt,memory,address-base)[0]
def i32(address):return read(address,'<i')
def u32(address):return read(address,'<I')
def raw(address):return pe.get_string_at_rva(address-base).decode('gbk')
def text(address):return raw(u32(address))
def resource(path):return path.replace('\\','/').lower()
manifest=json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'));objects={row['sha256']:row for row in manifest['objects']}
def bitmap(path):
    data=(ROOT/objects[manifest['resources'][path]['sha256']]['path']).read_bytes()
    return {'resource':path,'width':struct.unpack_from('<i',data,18)[0],'height':abs(struct.unpack_from('<i',data,22)[0])}
def subtitles(address,count):return [{'text':text(address+index*12)if u32(address+index*12)else'','segment':i32(address+index*12+4),'durationMs':u32(address+index*12+8)}for index in range(count)]
def color(value):return '#'+''.join(f'{(value>>shift)&255:02x}'for shift in [0,8,16])
credits_rect=[i32(0x46b998+i*4)for i in range(4)];font=raw(0x47c1e4)
# The game uses DrawTextA(..., DT_CENTER|DT_EXTERNALLEADING|DT_CALCRECT),
# a 20-pixel bold GBK font, and two pixels of extra character spacing.
class RECT(ctypes.Structure):_fields_=[('left',ctypes.c_long),('top',ctypes.c_long),('right',ctypes.c_long),('bottom',ctypes.c_long)]
gdi=ctypes.windll.gdi32;user=ctypes.windll.user32
gdi.CreateCompatibleDC.argtypes=[ctypes.c_void_p];gdi.CreateCompatibleDC.restype=ctypes.c_void_p
gdi.CreateFontW.argtypes=[ctypes.c_int]*13+[ctypes.c_wchar_p];gdi.CreateFontW.restype=ctypes.c_void_p
gdi.SelectObject.argtypes=[ctypes.c_void_p,ctypes.c_void_p];gdi.SelectObject.restype=ctypes.c_void_p
gdi.SetTextCharacterExtra.argtypes=[ctypes.c_void_p,ctypes.c_int]
gdi.DeleteObject.argtypes=[ctypes.c_void_p];gdi.DeleteDC.argtypes=[ctypes.c_void_p]
user.DrawTextW.argtypes=[ctypes.c_void_p,ctypes.c_wchar_p,ctypes.c_int,ctypes.POINTER(RECT),ctypes.c_uint]
dc=gdi.CreateCompatibleDC(None);handle=gdi.CreateFontW(20,0,0,0,700,0,0,0,0x86,0,0,0,0,font)
assert dc and handle,'GDI font creation failed';old=gdi.SelectObject(dc,handle);gdi.SetTextCharacterExtra(dc,2)
paragraphs=[];y=0
for index in range(15):
    value=text(0x4a2b5c+index*4);rect=RECT(*credits_rect);height=user.DrawTextW(dc,value,-1,ctypes.byref(rect),0x601)
    assert height>0;paragraphs.append({'text':value,'y':y,'height':rect.bottom-rect.top});y+=rect.bottom-rect.top+50
gdi.SelectObject(dc,old);gdi.DeleteObject(handle)
subtitle_height=i32(0x4e7098);handle=gdi.CreateFontW(subtitle_height,0,0,0,700,0,0,0,0x86,0,0,0,0,font);gdi.SelectObject(dc,handle)
subtitle_rect=RECT(90,430,550,455);effective_subtitle_height=user.DrawTextW(dc,'中',-1,ctypes.byref(subtitle_rect),0x401)
gdi.SelectObject(dc,old);gdi.DeleteObject(handle);gdi.DeleteDC(dc)
ending={
 'subtitles':[subtitles(0x4a2af0,9),subtitles(0x4a2a48,14)],
 'segments':[{'startMs':read(0x46b968+index*24,'<q')//10000,'endMs':read(0x46b970+index*24,'<q')//10000,'color':color(u32(0x46b978+index*24))}for index in range(2)],
 'font':font,'fontHeightArgument':subtitle_height,'fontSize':effective_subtitle_height,'characterSpacing':2,
 'subtitleRect':[i32(0x46b958+index*4)for index in range(4)],
 'movie':'derived/proepi/aopen02.webm','outro':'derived/proepi/aopen04.webm',
 'narration':['cd/data/proepi/back.wav','cd/data/proepi/pass.wav'],'music':'cd/data/proepi/bgm.wav',
 'video':{'x':0,'y':60,'width':640,'height':360,'sourceHeight':180,'scanlines':True},
 'credits':{'paragraphs':paragraphs,'rect':credits_rect,'font':font,'fontSize':20,'characterSpacing':2,'color':'#e0e0e0',
   'backgrounds':[{'resource':'install/'+resource(text(0x46b9b8+index*28)),'values':[i32(0x46b9bc+index*28+n*4)for n in range(6)]}for index in range(16)],
   'end':bitmap('cd/data/proepi/end.bmp')},
 'choice':{'background':bitmap('install/'+resource(raw(0x479aa4))),'buttons':[dict(bitmap('install/'+resource(raw(address))),x=x,y=274)for address,x in [(0x479a94,253),(0x479a84,349)]],'cursor':bitmap('install/'+resource(raw(0x479a70)))},
}
for key in ['movie','outro']:
    if ending[key]in manifest['resources']:ending[key+'Metadata']=objects[manifest['resources'][ending[key]]['sha256']]['media']
path=ROOT/'assets/catalog/rules.json';rules=json.loads(path.read_text(encoding='utf8'));rules['epilogue']=ending
path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'font':ending['font'],'fontSize':ending['fontSize'],'creditsParagraphHeights':[row['height']for row in paragraphs],'segments':ending['segments'],'movieMetadata':ending.get('movieMetadata'),'outroMetadata':ending.get('outroMetadata')},ensure_ascii=True,indent=2))
