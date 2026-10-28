"""Export exact running-contest resources and coordinates from N2Game.exe."""
import json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
pe=pefile.PE(str(ROOT/'research/original/N2Game.exe'));base=pe.OPTIONAL_HEADER.ImageBase
def i16(a):return struct.unpack('<h',pe.get_data(a-base,2))[0]
def u32(a):return struct.unpack('<I',pe.get_data(a-base,4))[0]
def raw(a):return pe.get_string_at_rva(a-base).decode('gbk')
def resource(a):return 'install/'+raw(u32(a)).replace('\\','/').lower()
manifest=json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'));objects={o['sha256']:o['path']for o in manifest['objects']}
def animation(path,x=0,y=0):
    data=(ROOT/objects[manifest['resources'][path]['sha256']]).read_bytes();kind,count=struct.unpack_from('<2H',data,4)
    speed=struct.unpack_from('<H',data,16)[0];interval=speed*1000//70 if kind==0xaf11 else struct.unpack_from('<I',data,16)[0]
    return {'resource':path,'x':x,'y':y,'frameCount':count,'interval':interval}
def export():
    path=ROOT/'assets/catalog/rules.json';rules=json.loads(path.read_text(encoding='utf8'))
    rules['competition']['running']={
        'scorePauseText':raw(0x49b270),
        'actors':[[[animation(resource(0x4987f8+sex*12+phase*4),243,176)for phase in range(3)]]+
                  [[animation(resource(0x498810+actor*12+phase*4),243,176)for phase in range(3)]for actor in range(3)]for sex in range(2)],
        'track':animation(resource(0x49883c),120,62),
        'launch':animation(resource(0x498834),37,-34),'flight':animation(resource(0x498838),37,-34),
        'hand':animation(resource(0x498350),560,423),
        'host':animation(resource(0x4986cc),0,366),
        'podium':animation(resource(0x4983b4),i16(0x4983a0),i16(0x4983a2)),
        'podiumPlaces':[animation(resource(0x4983bc+place*4),i16(0x4983a4+place*4),i16(0x4983a6+place*4))for place in range(3)],
        'rendering':{
            'background':resource(0x4986e4),
            'playerPortraits':[[[resource(0x498594+(sex*3+stage)*8+selected*4)for selected in range(2)]for stage in range(2,5)]for sex in range(2)],
            'otherPortraits':[[resource(0x49855c+actor*8+selected*4)for selected in range(2)]for actor in range(3)],
            'scoreLabels':[raw(0x49b294),raw(0x49b28c)],
            'scorePauseText':raw(0x49b270),
            'sounds':{'start':resource(0x498780+7*4),'launch':resource(0x498780+8*4),'result':resource(0x498780+10*4)},
        },
    }
    path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print(json.dumps(rules['competition']['running'],ensure_ascii=False,indent=2))
if __name__=='__main__':export()
