"""Read-only skill and career-history screens (0041cd20 / 0041d8a0)."""
import json, struct, sys
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
pe=pefile.PE(str(ROOT/'research/original/N2Game.exe'));base=pe.OPTIONAL_HEADER.ImageBase
manifest=json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'))
objects={row['sha256']:row for row in manifest['objects']}
def number(address,fmt='<i'):return struct.unpack(fmt,pe.get_data(address-base,struct.calcsize(fmt)))[0]
def text(address):return pe.get_string_at_rva(address-base).decode('gbk')
def pointer(address):return text(number(address,'<I'))
def media(address,x=0,y=0,transparent=1):
    name=text(address).replace('\\','/').lower()
    resource=next(p for p in [name,'install/'+name,'cd/data/'+name] if p in manifest['resources'])
    raw=objects[manifest['resources'][resource]['sha256']]['media']
    result={'resource':resource,'x':x,'y':y,'transparent':transparent,**raw}
    if raw['kind'] in ('flc','fli'):result.update(frameCount=raw['frames'],interval=raw['speedRaw'])
    return result
data={
    'skills':{
        'evidence':'0x0041cd20','background':media(0x479dd8,transparent=None),
        'scrollThumb':media(0x479dc8,508,132,None),
        'scrollButtons':[media(0x479db0,508,113),media(0x479da0,508,358)],
        'tabs':[media(0x479d90,45,77),media(0x479d80,130,77)],
        'back':[media(0x47293c,554,383),media(0x47294c,554,383)],
        'scrollRect':[508,132,523,353],'scrollStep':9,'visibleRows':4,'maxScroll':22,
        'rowY':199,'rowHeight':48,'nameX':57,'nameWidth':11,
        'columns':[158,270,382],'columnWidth':13,'firstLineY':190,'secondLineY':208,
        'learnedColor':1,'unlearnedColor':255,
        'names':[text(0x479d64)]+[pointer(0x476f30+i*4) for i in range(1,26)],
        'triggerPrefix':text(0x479dc0),'attributePrefix':text(0x479d50),'percentSuffix':text(0x479d4c),
        'triggerNames':[pointer(0x479c58+i*4) for i in range(5)],
        'attributeNames':[pointer(0x479c6c+i*4) for i in range(7)],
        'rows':[None]+[{'trigger':number(0x479aa8+i*16),'attribute':number(0x479aac+i*16),
                 'values':[number(0x479ab0+i*16+j*2,'<h') for j in range(3)]} for i in range(1,26)],
        'student':{'trigger':number(0x479c48),'attribute':number(0x479c4c),
                   'values':[number(0x479c50+i*2,'<h') for i in range(3)]},
    },
    'careerHistory':{
        'evidence':'0x0041d8a0','background':media(0x479e18,transparent=None),
        'scrollThumb':media(0x479e08,454,128,None),
        # 0041d8a0 uses the opaque 459e80 constructor here; Skills uses 459eb0(...,1).
        'scrollButtons':[media(0x479db0,454,109,None),media(0x479da0,454,354,None)],
        'back':[media(0x479de8,553,383),media(0x479df8,553,383)],
        'scrollRect':[454,128,468,350],'scrollTravel':171,'visibleRows':10,
        'columns':[71,129,272],'widths':[6,16,21],'rowY':136,'rowHeight':24,
    },
}
path=ROOT/'assets/catalog/rules.json';rules=json.loads(path.read_text(encoding='utf8'))
rules['statusRendering'].update(data)
path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'skills':data['skills']['background']['resource'],'history':data['careerHistory']['background']['resource'],'texts':{k:data['skills'][k] for k in ['names','triggerPrefix','attributePrefix','triggerNames','attributeNames','student','rows']}},ensure_ascii=True))
