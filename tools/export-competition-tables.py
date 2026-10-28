"""Export the reference game's student competition tables, text and animation metadata."""
import json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
pe=pefile.PE(str(ROOT/'research/original/N2Game.exe'));base=pe.OPTIONAL_HEADER.ImageBase
def i16(a):return struct.unpack('<h',pe.get_data(a-base,2))[0]
def u32(a):return struct.unpack('<I',pe.get_data(a-base,4))[0]
def i32(a):return struct.unpack('<i',pe.get_data(a-base,4))[0]
def rawtext(a):return pe.get_string_at_rva(a-base).decode('gbk')
def text(a):return pe.get_string_at_rva(u32(a)-base).decode('gbk')
def resource(a):return 'install/'+text(a).replace('\\','/').lower()
manifest=json.loads((ROOT/'assets/manifest.json').read_text(encoding='utf8'))
objects={o['sha256']:o['path']for o in manifest['objects']}
def animation(path,x=0,y=0,divisor=1):
 data=(ROOT/objects[manifest['resources'][path]['sha256']]).read_bytes()
 kind,frames,width,height=struct.unpack_from('<4H',data,4)
 speed=struct.unpack_from('<H',data,16)[0]
 interval=speed*1000//70 if kind==0xaf11 else struct.unpack_from('<I',data,16)[0]
 return {'resource':path,'x':x,'y':y,'frameCount':frames,'interval':interval//divisor}
path=ROOT/'assets/catalog/rules.json';rules=json.loads(path.read_text(encoding='utf8'))
running=rules.get('competition',{}).get('running')
rules['competition']={
 'charmThresholds':[[i16(0x4987cc+stage*6+n*2)for n in range(3)]for stage in range(3)],
 'staminaThresholds':[[i16(0x4987de+stage*6+n*2)for n in range(3)]for stage in range(3)],
 'prizes':[i16(0x4987ee+n*2)for n in range(4)],
 'menu':[[text(0x4985e4+stage*16+n*4)for n in range(4)]for stage in range(3)],
 'beautyIntro':[[text(0x4983f8+sex*28+n*4)for n in range(7)]for sex in range(2)],
 'runningIntro':[[text(0x498448+sex*28+n*4)for n in range(7)]for sex in range(2)],
 'beautyOpponents':[[text(0x498430+sex*12+n*4)for n in range(3)]for sex in range(2)],
 'runningOpponents':[[text(0x498490+sex*12+n*4)for n in range(3)]for sex in range(2)],
 'quizText':[text(0x4984a8+n*4)for n in range(11)],
 'beautyText':[text(0x498614+n*4)for n in range(7)],
 'runningText':[text(0x498630+n*4)for n in range(5)],
 'balloon':animation('install/ani/ball1.flc',280,0),
 'openingAnimation':animation(resource(0x4986d0),120,62),
 'beautyActors':[[[animation(resource(0x4986f0+sex*12+n*4),120,62,2)for n in range(3)]]+
                 [[animation(resource(0x498708+actor*12+n*4),120,62,2)for n in range(3)]for actor in range(3)]for sex in range(2)],
 'beautyStage':animation(resource(0x4986ec),120,62),
 'judges':animation(resource(0x4986e8),120,62,4),
 'beautyPodium':animation(resource(0x4983b0),i16(0x4983a0),i16(0x4983a2)),
 'podiumPlaces':[animation(resource(0x4983bc+n*4),i16(0x4983a4+n*4),i16(0x4983a6+n*4))for n in range(3)],
 'rankAnimations':[animation(resource(0x4986d8+n*4),183,87)for n in range(3)],
 'moneyAnimation':animation(resource(0x4986d4),136,0),
 'quizFeedback':[animation('install/harvest/w01.flc',364,217),animation('install/harvest/r01.flc',364,217)],
 'beautyWalkStep':struct.unpack('<d',pe.get_data(0x46b280-base,8))[0],
 'rendering':{
  'background':{'resource':resource(0x498648),'x':0,'y':0},
  'menuHost':{'resource':resource(0x4986c8),'x':-60,'y':130,'transparent':True},
  'hostAnimation':animation(resource(0x4986cc),0,366),
  'talkBackground':{'resource':resource(0x49864c),'x':97,'y':366,'transparent':True},
  'talkText':{'x':140,'y':370,'width':460,'height':90,'color':'#0a0a0a','fontSize':16},
  'menuTitle':{'resource':resource(0x49836c),'x':152,'y':76,'transparent':True},
  'menuText':{'x':200,'y':86,'width':420,'height':120,'color':'#0a0a0a','fontSize':16},
  'menuButtons':[{'resource':'install/'+rawtext(0x498658+n*40).replace('\\','/').lower(),'x':i32(0x498650+n*40),'y':i32(0x498654+n*40),'width':150,'height':40}for n in range(3)],
  'introBackgrounds':[[resource(0x4983d8+sex*16+n*4)for n in range(4)]for sex in range(2)],
  'scorePanelResource':resource(0x498358),
  'scorePanelPositions':[[i16(0x4983c8+n*4)-10,i16(0x4983ca+n*4)]for n in range(4)],
  'scoreNamePositions':[[i16(0x4983c8+n*4)+40,i16(0x4983ca+n*4)+1]for n in range(4)],
  'scoreValuePositions':[[i16(0x4983c8+n*4)+40,i16(0x4983ca+n*4)+17]for n in range(4)],
  'scoreLabels':[rawtext(0x49b294),rawtext(0x49b29c)],
  'portraitPositions':[[i16(0x4985d4+n*4),i16(0x4985d6+n*4)]for n in range(4)],
  'beautyPlayerPortraits':[[[resource(0x498564+(sex*3+stage)*8+n*4)for n in range(2)]for stage in range(2,5)]for sex in range(2)],
  'beautyOtherPortraits':[[resource(0x49852c+actor*8+n*4)for n in range(2)]for actor in range(3)],
  'judgeScorePositions':[[i16(0x4987c0+n*4),i16(0x4987c2+n*4)]for n in range(3)],
  'podiumNamePositions':[[i16(0x4983a4+n*4)+120,i16(0x4983a6+n*4)+20]for n in range(3)],
  'quizText':{'x':168,'y':166,'width':300,'height':230,'color':'#ffffff','fontSize':16},
  'quizPaletteSource':resource(0x49835c),
  'quizScoreBackground':{'resource':resource(0x498354),'x':548,'y':55},
  'quizScoreResources':[resource(0x49835c+n*4)for n in range(4)],
  'quizScorePosition':{'x':544,'baseY':333,'stepY':-30},
  'quizSelection':animation('install/public/fselt.flc'),
  'music':resource(0x498780),
  'sounds':[resource(0x498780+n*4)for n in range(16)],
  'quizSounds':[resource(0x4955dc+n*4)for n in range(4)],
 },
 'evidenceFunctions':['0x004193e0','0x0044c010','0x0044a4d0','0x004495a0','0x0044b260'],
}
if running is not None:rules['competition']['running']=running
path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({k:rules['competition'][k]for k in ['charmThresholds','staminaThresholds','prizes']}))
