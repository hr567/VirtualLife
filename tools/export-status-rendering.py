"""Parameter overview artwork and text tables, from N2Game.exe FUN_00401550."""
import json, re, struct, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT/'research/tools/python-packages'))
import pefile
pe = pefile.PE(str(ROOT/'research/original/N2Game.exe'))
base = pe.OPTIONAL_HEADER.ImageBase
manifest = json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'))
objects = {row['sha256']: row for row in manifest['objects']}
def number(address,fmt): return struct.unpack(fmt,pe.get_data(address-base,struct.calcsize(fmt)))[0]
def text(address): return pe.get_string_at_rva(address-base).decode('gbk')
def pointer(address): return text(number(address,'<I'))
def resolve(path):
    path = path.replace('\\','/').lower().lstrip('./')
    for candidate in [path,'install/'+path,'cd/data/'+path,'cd/'+path]:
        if candidate in manifest['resources']: return candidate
    raise ValueError('Missing parameter overview resource: '+path)
def media(path,x=0,y=0,transparent=1):
    resource = resolve(path); obj = objects[manifest['resources'][resource]['sha256']]
    result = {'resource':resource,'x':x,'y':y,'transparent':transparent,**obj['media']}
    if obj['media']['kind'] in ['flc','fli']:
        # 0045b4a0 stores the original FLI 1000/7 clock conversion as uint16.
        raw = obj['media']['speedRaw']
        result.update(frameCount=obj['media']['frames'],interval=(raw*1000//7)&0xffff if obj['media']['kind']=='fli' else raw)
    return result

source=(ROOT/'research/decompiled/functions/00401550.c').read_text(encoding='utf8')
descriptions=[text(a) for a in [0x4732a4,0x473298,0x47328c,0x473320,0x473320]]
for match in re.finditer(r'local_([0-9a-f]+) = &DAT_([0-9a-f]{8});',source):
    offset=int(match[1],16)
    if 0x888<=offset<=0x94c: descriptions.append(text(int(match[2],16)))

data={
    'evidence':'0x00401550',
    'background':media(text(0x473360),transparent=None),
    'portraits':[media(pointer(0x477258+i*4),39,38) for i in range(62)],
    'portraitPaletteRange':[202,221],
    'hpLabel':media(text(0x473350),217,291),
    'hpFrame':media(text(0x473340),259,297),
    'hpFill':media(text(0x473330),264,300),
    'back':[media(text(a),553,383) for a in [0x47293c,0x47294c]],
    'buttons':[media(text(a),551,y) for a,y in [(0x473310,259),(0x473300,313),(0x4732f0,151),(0x4732e0,205)]],
    'disabledButtons':[media(text(a),551,y) for a,y in [(0x4732cc,259),(0x4732b8,313)]],
    'emptyLabel':text(0x47332c),'sexLabels':[text(0x4732b4),text(0x4732b0)],
    'marriageLabels':[text(0x47292c),text(0x472934)],
    'missionRankLabels':[text(a) for a in [0x4729cc,0x4729c0,0x4729b8,0x4729b0,0x4729a4,0x472998,0x47298c,0x472980,0x472974,0x472968,0x47295c]],
    'initialMissionRankLabel':text(0x4729d8),
    'descriptions':descriptions,
    'careerRankNames':[[pointer(0x476f94+(career*5+rank)*4) for rank in range(1,6)] for career in range(25)],
    'salaryBase':[number(0x47f078+i*2,'<h') for i in range(25)],
    'salaryMultipliers':[number(0x477a4c+i*4,'<i') for i in range(6)],
    'spouseSalary':[number(0x477a64+i*2,'<h') for i in range(16)],
    'investmentRows':[[number(0x47ae20+(i*5+j)*4,'<i') for j in range(5)] for i in range(4)],
    'texts':[
        {'field':name,'x':x,'y':y,'color':255,'width':width}
        for name,x,y,width in [
            ('name',208,78,11),('sex',304,78,11),('age',400,78,11),
            ('career',208,137,11),('salary',304,137,11),('affection',400,137,11),
            ('investment',208,196,11),('investmentReturn',304,196,11),('marriage',400,196,11),
            ('spouseSalary',208,255,11),('experience',304,255,11),('lifeLevel',400,255,11),
            ('money',49,172,14),('intelligence',49,208,14),('morality',49,244,14),('charm',49,280,14),('stamina',49,316,14),
        ]],
    'hpText':{'x':385,'y':297,'shadowX':386,'shadowY':298},
    'missionRank':{'valueX':65,'valueY':413,'textX':153,'textY':414},
    'mission':{'x':243,'y':405,'emptyY':413},
    'submenus':{'house':'0x0043b040','investment':'0x0042a120','careerHistory':'0x0041d8a0','skills':'0x0041cd20','skillsWrapper':'0x0041e050'},
}
path=ROOT/'assets/catalog/rules.json'
rules=json.loads(path.read_text(encoding='utf8'));data.update({key:value for key,value in rules.get('statusRendering',{}).items() if key not in data});rules['statusRendering']=data
path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'background':data['background']['resource'],'portraitCount':len(data['portraits']),'descriptionCount':len(descriptions),'evidence':data['evidence']},ensure_ascii=True))
