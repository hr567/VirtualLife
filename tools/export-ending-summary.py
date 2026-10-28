"""Export original career epilogues, life-history templates, and ending assets."""
import ctypes,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
PE=pefile.PE(str(ROOT/'research/original/N2Game.exe'));BASE=PE.OPTIONAL_HEADER.ImageBase
def u32(a):return struct.unpack('<I',PE.get_data(a-BASE,4))[0]
def i32(a):return struct.unpack('<i',PE.get_data(a-BASE,4))[0]
def raw(a):return PE.get_string_at_rva(a-BASE).decode('gbk')
def text(a):return raw(u32(a))
MANIFEST=json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'));OBJECTS={o['sha256']:o['path']for o in MANIFEST['objects']}
def animation(path):
    data=asset(path);return {'resource':path,'frameCount':struct.unpack_from('<H',data,6)[0],'interval':50,'frame':0,'lastTick':0,'first':True,'x':110,'y':463}
def asset(path):return (ROOT/OBJECTS[MANIFEST['resources'][path]['sha256']]).read_bytes()
def bitmap_palette(path):
    data=asset(path);offset=14+struct.unpack_from('<I',data,14)[0]
    return [[data[offset+i*4+2],data[offset+i*4+1],data[offset+i*4],0]for i in range(256)]
def flic_palette(path):
    data=asset(path);palette=[[0,0,0,0]for _ in range(256)];frame=128
    while frame<len(data):
        size,kind,chunks=struct.unpack_from('<IHH',data,frame);pos=frame+16
        if kind!=0xf1fa:frame+=size;continue
        for _ in range(chunks):
            length,chunk=struct.unpack_from('<IH',data,pos)
            if chunk in [4,11]:
                packets=struct.unpack_from('<H',data,pos+6)[0];p=pos+8;index=0
                for _ in range(packets):
                    skip,count=data[p:p+2];p+=2;index+=skip;count=count or 256
                    for _ in range(count):
                        palette[index]=[(v<<2)if chunk==11 else(v&0xfc)for v in data[p:p+3]]+[0];p+=3;index+=1
                return palette
            pos+=length
        frame+=size
    raise ValueError('No FLC palette: '+path)
def font_height():
    class RECT(ctypes.Structure):_fields_=[('left',ctypes.c_long),('top',ctypes.c_long),('right',ctypes.c_long),('bottom',ctypes.c_long)]
    gdi=ctypes.windll.gdi32;user=ctypes.windll.user32
    gdi.CreateCompatibleDC.argtypes=[ctypes.c_void_p];gdi.CreateCompatibleDC.restype=ctypes.c_void_p
    gdi.CreateFontW.argtypes=[ctypes.c_int]*13+[ctypes.c_wchar_p];gdi.CreateFontW.restype=ctypes.c_void_p
    gdi.SelectObject.argtypes=[ctypes.c_void_p,ctypes.c_void_p];gdi.SelectObject.restype=ctypes.c_void_p
    gdi.DeleteObject.argtypes=[ctypes.c_void_p];gdi.DeleteDC.argtypes=[ctypes.c_void_p]
    user.DrawTextW.argtypes=[ctypes.c_void_p,ctypes.c_wchar_p,ctypes.c_int,ctypes.POINTER(RECT),ctypes.c_uint]
    dc=gdi.CreateCompatibleDC(None);font=gdi.CreateFontW(14,0,0,0,400,0,0,0,0x86,0,0,0,0,raw(0x47c1e4));old=gdi.SelectObject(dc,font)
    rect=RECT(100,19,360,443);height=user.DrawTextW(dc,raw(0x46b288),-1,ctypes.byref(rect),0x600)
    gdi.SelectObject(dc,old);gdi.DeleteObject(font);gdi.DeleteDC(dc)
    assert height>0
    return height
def export():
    paragraphs=[]
    for category in range(16):
        pointer=u32(0x49ba44+category*4);rows=[]
        while u32(pointer)!=0:
            rows.append(text(pointer));pointer+=4
        paragraphs.append(rows)
    value={
        'header':raw(0x46b288),'paragraphs':paragraphs,
        'conclusions':[[text(0x49baec+career*4)for career in range(26)],[text(0x49ba84+career*4)for career in range(26)]],
        'genderReplacements':[[raw(0x4732b0),raw(0x4732b4)],[raw(0x4a2474),raw(0x4a2470)]],
        'textX':i32(0x49b978),'initialY':i32(0x49b97c),'clip':[i32(0x49b980+i*4)for i in range(4)],
        'backgrounds':[[f'cd/data/evaluate/bg/ed{sex}{career+1:02}.bmp'for career in range(26)]for sex in ['m','w']],
        'portraits':[[f'cd/data/evaluate/player/eu{sex}{career+1:02}.bmp'for career in range(26)]for sex in ['m','w']],
        'actors':[animation('cd/data/evaluate/player/'+raw(0x4a2564).lower()),animation('cd/data/evaluate/player/'+raw(0x4a2540).lower())],
        'music':'cd/data/evaluate/end01.wav','fontSize':14,'lineSpacing':14,'byteWrap':32,'characterSpacing':2,
    }
    value['lineHeight']=font_height();value['font']=raw(0x47c1e4)
    value['encodedHeader']=list(value['header'].encode('gbk'))
    value['encodedParagraphs']=[[list(s.encode('gbk'))for s in row]for row in paragraphs]
    value['encodedConclusions']=[[list(s.encode('gbk'))for s in row]for row in value['conclusions']]
    value['encodedGenderReplacements']=[[list(s.encode('gbk'))for s in row]for row in value['genderReplacements']]
    value['palettes']=[]
    for sex in range(2):
        actor=flic_palette(value['actors'][sex]['resource']);row=[]
        for career in range(26):
            palette=bitmap_palette(value['backgrounds'][sex][career]);portrait=bitmap_palette(value['portraits'][sex][career])
            for i in range(83):palette[i][:3]=actor[i][:3]
            for i in range(83,206):palette[i]=portrait[i][:]
            for i in range(246,256):palette[i][:3]=[255*(i-245)//10]*3
            palette[255][:3]=[255]*3
            row.append(palette)
        value['palettes'].append(row)
    for group in ['backgrounds','portraits']:
        for row in value[group]:
            for resource in row:assert resource in MANIFEST['resources'],resource
    path=ROOT/'assets/catalog/rules.json';rules=json.loads(path.read_text(encoding='utf8'));rules['endingSummary']=value
    path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print(json.dumps({key:value[key]for key in ['lineHeight','textX','initialY','clip','actors']},ensure_ascii=True,indent=2))
if __name__=='__main__':export()
