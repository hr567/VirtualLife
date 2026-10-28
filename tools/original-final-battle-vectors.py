"""Run the final encounter's original x86 arithmetic and combat frame loop.

Only graphics/audio/input devices are replaced. Card generation, attack selection,
combination arithmetic, item effects and animation clocks execute original code.
"""
import hashlib,json,random,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import *
SOURCE=ROOT/'research/original/N2Game.exe';PE=pefile.PE(str(SOURCE));RULES=json.loads((ROOT/'assets/catalog/rules.json').read_text(encoding='utf8'))['finalBattle']
STACK=0x1001e000;STOP=0x30000ff0
def w32(vm,a,v):vm.mem_write(a,struct.pack('<I',v&0xffffffff))
def w16(vm,a,v):vm.mem_write(a,struct.pack('<H',v&65535))
def wb(vm,a,v):vm.mem_write(a,bytes([v&255]))
def i32(vm,a):return struct.unpack('<i',vm.mem_read(a,4))[0]
def i16(vm,a):return struct.unpack('<h',vm.mem_read(a,2))[0]
def i8(vm,a):return struct.unpack('<b',vm.mem_read(a,1))[0]
def u32(vm,a):return struct.unpack('<I',vm.mem_read(a,4))[0]
def ret(vm,n=0,value=0):
 sp=vm.reg_read(UC_X86_REG_ESP);vm.reg_write(UC_X86_REG_EAX,value&0xffffffff);vm.reg_write(UC_X86_REG_EIP,u32(vm,sp));vm.reg_write(UC_X86_REG_ESP,sp+4+n)
def machine():
 vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(0,4096);vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(0x400000,PE.get_memory_mapped_image());vm.mem_map(0x10000000,0x20000);vm.mem_map(0x20000000,0x100000);vm.mem_map(0x30000000,4096);vm.reg_write(UC_X86_REG_ESP,STACK);w32(vm,STACK,STOP);return vm
def call(vm,address,end=STOP):
 vm.reg_write(UC_X86_REG_ESP,STACK);w32(vm,STACK,STOP)
 try:vm.emu_start(address,end,count=1000000)
 except Exception:print('failed',hex(address),hex(vm.reg_read(UC_X86_REG_EIP)));raise
 assert vm.reg_read(UC_X86_REG_EIP)==end,(hex(address),hex(vm.reg_read(UC_X86_REG_EIP)))
def card_write(vm,cards):
 w16(vm,0x4ba7b4,5);w16(vm,0x4ba7c6,len(cards))
 for i,card in enumerate(cards):
  wb(vm,0x4ba3f0+i*12,card['kind']);w32(vm,0x4ba3f4+i*12,card['attack']);w32(vm,0x4ba3f8+i*12,card['defense']);wb(vm,0x4ba668+i,card.get('selected',False));w16(vm,0x4ba658+i*2,card.get('x',RULES['cardPositions'][i][0]))
def card_read(vm):return [{'kind':i8(vm,0x4ba3f0+i*12),'attack':i32(vm,0x4ba3f4+i*12),'defense':i32(vm,0x4ba3f8+i*12),'selected':i8(vm,0x4ba668+i)==1,'x':i16(vm,0x4ba658+i*2)}for i in range(i16(vm,0x4ba7c6))]

def numeric():
 cases=[];r=random.Random(1999)
 for career in range(-1,25):
  for level,equipped in [(1,0),(50,6),(15,3)]:
   vm=machine();w16(vm,0x4b64a2,career);w16(vm,0x4b64a4,level);vm.mem_write(0x47ad94,bytes([0]*equipped+[255]*(6-equipped)));wb(vm,0x47ad5c,-1);wb(vm,0x47ada5,-1);wb(vm,0x4b9ecd,0)
   call(vm,0x441a81,0x441b0b);cases.append({'career':career,'lifeLevel':level,'equippedCount':equipped,'hp':i32(vm,0x4e655c),'power':i16(vm,0x4e640c)})
 combinations=[];items=[]
 for n in range(300):
  cards=[{'kind':r.randrange(4),'attack':r.choice([r.randrange(0,1201),32000,65540]),'defense':r.randrange(0,1001),'selected':r.choice([True,False])}for _ in range(5)]
  power=r.choice([3,12,20,30,42,100]);shield=r.randrange(3)
  vm=machine();card_write(vm,cards);w16(vm,0x4e640c,power);wb(vm,0x4e67e6,shield);call(vm,0x443160)
  combinations.append({'cards':cards,'power':power,'shield':shield,'attack':i16(vm,0x4e6500),'defense':i16(vm,0x4e6720),'afterShield':i8(vm,0x4e67e6)})
  if n<100:
   for item in range(10):
    vm=machine();card_write(vm,cards);wb(vm,0x4e6502,item);w32(vm,0x4e6780,0);w32(vm,0x4e645c,0);call(vm,0x443880)
    items.append({'cards':cards,'item':item,'used':i32(vm,0x4e6780)==1,'selectionChanged':i32(vm,0x4e645c)==1,'after':card_read(vm)})
 return cases,combinations,items

def valuations():
 result=[];r=random.Random(441860)
 for case in range(130):
  career=case%26-1;style=r.randrange(-1,4);furniture=0 if style<0 else r.randrange(16);investment=0 if style<0 else r.randrange(1,14);spouse=r.randrange(-1,16)
  fields=[[0x4b648a,r.choice([0,99999999,5000000]),4],[0x4b64a2,career,2],[0x4b64a4,r.randrange(1,51),2],[0x4b64a6,r.randrange(1,6),2],
   *[[address,r.randrange(10000),2]for address in [0x4b6492,0x4b6490,0x4b6494,0x4b648e]],
   [0x4b64f2,r.randrange(26),2],[0x4b64e2,r.randrange(101),2],[0x4b64d4,r.randrange(101),2]]
  wins=[r.randrange(1000)for _ in range(15)];vm=machine()
  for address,value,width in fields:{2:w16,4:w32}[width](vm,address,value)
  wb(vm,0x47ad5c,style);w16(vm,0x4b6ac8,furniture);wb(vm,0x4b9ecd,investment);wb(vm,0x47ada5,spouse)
  for index,value in enumerate(wins):w16(vm,0x4b9f6c+index*2,value)
  call(vm,0x441860)
  result.append({'playerFields':fields,'style':style,'furniture':furniture,'investmentStage':investment,'spouse':spouse,'wins':wins,'valuation':list(struct.unpack('<14i',vm.mem_read(0x4e6514,56)))})
 return result

FIELDS={
 'hp':(0x4e655c,4),'enemyHp':(0x4e650c,4),'power':(0x4e640c,2),'attack':(0x4e6500,2),'defense':(0x4e6720,2),'enemyDefense':(0x4e6560,2),
 'combatState':(0x4e6714,1),'enemyAnimation':(0x4e64f4,4),'round':(0x4e6458,2),'shield':(0x4e67e6,1),
 'displayedHp':(0x4ba444,4),'displayedEnemyHp':(0x4ba784,4),'hpDelta':(0x4ba3dc,2),'enemyHpDelta':(0x4ba662,2),'hpTextY':(0x4ba6b0,2),'enemyTextY':(0x4ba74e,2),
 'activeItem':(0x4e6502,1),'itemUsed':(0x4e6780,4),'selectionChanged':(0x4e645c,4),'cardsMoving':(0x4ba59c,2),'itemScroll':(0x4e649c,1),
}
def sequence(seed,limit=1500,initial=None,strategy='normal',step=17):
 vm=machine();objects={};metas={};clock=[0];current={};draws=[0];frames=[];base=STACK-0x700;layers=[];sounds=[];sound_stops=[];sound_serial=[0];allocations=[0]
 w32(vm,0x20000004,0x30000000);w32(vm,0x20000008,0x30000000)
 def obj(address,meta=None,period=None):
  address=int(address);target=0x20001000+allocations[0]*0x500;allocations[0]+=1;objects[address]=target;w32(vm,address,target);w32(vm,target,0x20000000)
  meta=meta or {'frameCount':10,'frameDurationMs':71};metas[address]=meta
  for off,value in [(0x1c,0),(0x24,meta['frameCount']),(0x28,0),(0x30,1),(0x34,meta['frameDurationMs']if period is None else period)]:w32(vm,target+off,value)
  return target
 pointers=[0x4e6558,0x4e63d4,0x4e63d8,0x4e63d0,0x4e6564,0x4e6408,0x4e6504,0x4e65e0,0x4e6724,0x4e64ec]
 pointers+=list(range(0x4e6418,0x4e6450,4))+list(range(0x4e63dc,0x4e6408,4))+list(range(0x4e6728,0x4e6738,4))+list(range(0x4e64a0,0x4e64c8,4))
 pointers+=list(range(0x4e65e4,0x4e660c,4))+list(range(0x4e64cc,0x4e64f0,4))
 for a in sorted(set(pointers)):obj(a)
 for n,meta in enumerate(RULES['enemyAnimations']):obj(0x4e6664+n*4,meta,0)
 for n,meta in enumerate(RULES['playerAnimations']):obj(0x4e6574+n*4,meta,0)
 obj(0x4e64f0,RULES['recoveryAnimation'],0)
 for n in range(2):obj(0x4e6784+n*4,RULES['resourcesByAddress'][f'0x{0x495eac+n*4:08x}'])
 obj(0x4e6558,RULES['resourcesByAddress']['0x004961fc'])
 for n in range(9):obj(0x4e64cc+n*4,RULES['resourcesByAddress'][f'0x{0x495e88+n*4:08x}'])
 for n in range(10):obj(0x4e65e4+n*4,RULES['resourcesByAddress'][f'0x{0x495e60+n*4:08x}'])
 dynamic=set([0x4e6558,0x4e64f0,*range(0x4e6574,0x4e65a8,4),*range(0x4e6664,0x4e66c8,4),*range(0x4e64cc,0x4e64f0,4),*range(0x4e65e4,0x4e660c,4),0x4e6784,0x4e6788])
 for address in dynamic:obj(address,RULES['runtimeObjects'][f'0x{address:08x}'])
 reverse={value:key for key,value in objects.items()}
 sound_objects={}
 for n,address in enumerate([0x4e6568,0x4e656c,*range(0x4e660c,0x4e665c,4),*range(0x4e66cc,0x4e6710,4),0x4e6718,0x4e671c,*range(0x4e6798,0x4e67d4,4)]):
  target=0x20090000+n*16;w32(vm,address,target);sound_objects[target]=address
 w32(vm,0x4b6454,0x200fe000);w32(vm,0x200fe004,0x200fe100);w32(vm,0x200fe008,0x200fe200);w32(vm,0x46b110,0x30000020);w32(vm,0x4a3f84,seed)
 for name,(a,width)in FIELDS.items():{1:wb,2:w16,4:w32}[width](vm,a,{'hp':30000,'enemyHp':30000,'power':20,'displayedHp':30000,'displayedEnemyHp':30000,'hpTextY':300,'activeItem':-1}.get(name,0))
 wb(vm,0x4e665c,-1);w16(vm,0x4e6508,0);w16(vm,0x4ba7b4,5);w16(vm,0x4ba7c6,0)
 for n in range(5):w16(vm,0x4ba658+n*2,580);wb(vm,0x4ba668+n,0)
 if initial:
  for key,value in initial.items():
   if key in FIELDS:a,width=FIELDS[key];{1:wb,2:w16,4:w32}[width](vm,a,value)
   elif key=='items':w16(vm,0x4e6508,len(value));vm.mem_write(0x4e6760,bytes(value))
   elif key=='cards':card_write(vm,value)
 # Hardware drawing stubs. The two jumps omit pixel loops/text composition,
 # leaving the original bar interpolation and countdown instructions intact.
 def hook(uc,address,size,_):
  sp=uc.reg_read(UC_X86_REG_ESP)
  if address==0x30000000:ret(uc,12)
  elif address==0x30000020:ret(uc,value=clock[0])
  elif address in [0x458ef0,0x45bae0]:ret(uc,4)
  elif address in [0x45a370,0x45a3e0]:
   pointer=sound_objects.get(uc.reg_read(UC_X86_REG_ECX));resource=RULES.get('runtimeObjects',{}).get(f'0x{pointer:08x}',{}) if pointer else {}
   path=resource.get('sexResources',[resource.get('assetPath')]*2)[(initial or {}).get('sex',0)]
   assert path,hex(pointer or 0)
   (sounds if address==0x45a370 else sound_stops).append(path);sound_serial[0]+=1;ret(uc,4 if address==0x45a370 else 0)
  elif address in [0x458530,0x458550,0x458570]:
   target=uc.reg_read(UC_X86_REG_ECX);pointer=reverse.get(target)
   if pointer in dynamic:
    meta=metas[pointer];opaque=address==0x458530
    layers.append({'assetPath':meta['assetPath'],'frame':i32(uc,target+0x1c),'x':meta['x'],'y':meta['y'],'transparentIndex':None if opaque else meta.get('transparentIndex'),'firstPixelTransparent':not opaque and meta.get('firstPixelTransparent',False)})
   ret(uc,8 if address==0x458570 else 4)
  elif address in [0x458610,0x4597a0,0x45bd90,0x45a8d0,0x458c50,0x458bf0,0x458c70]:ret(uc)
  elif address==0x45ab10:ret(uc)
  elif address==0x45ba70:ret(uc,4)
  elif address==0x45a940:ret(uc,4)
  elif address==0x458ba0:ret(uc,16)
  elif address in [0x458c90,0x458ce0,0x4596e0,0x457100]:ret(uc,16)
  elif address in [0x4599b0,0x4599e0]:ret(uc,16)
  elif address==0x459a20:
   bounds=[i32(uc,sp+4+n*4)for n in range(4)];position=current.get('position');ret(uc,16,int(position is not None and bounds[0]<=position[0]<bounds[2]and bounds[1]<=position[1]<bounds[3]))
  elif address==0x45a090:w32(uc,uc.reg_read(UC_X86_REG_ECX)+0x1c,int(current.get('action')=='attack'));ret(uc,4)
  elif address==0x45dec1:draws[0]+=1
  elif address==0x4423c6:uc.reg_write(UC_X86_REG_EIP,0x4425fd)
  elif address==0x44261f:uc.reg_write(UC_X86_REG_EIP,0x442735)
  elif address==0x4427bd:uc.reg_write(UC_X86_REG_EIP,0x4428a6)
  elif address==0x456c20:uc.emu_stop()
 addresses=[0x30000000,0x30000020,0x458ef0,0x458530,0x458550,0x45a370,0x45bae0,0x458570,0x458610,0x4597a0,0x45a3e0,0x45bd90,0x45a8d0,0x458c50,0x458bf0,0x458c70,0x45ab10,0x45ba70,0x45a940,0x458ba0,0x458c90,0x458ce0,0x4596e0,0x457100,0x4599b0,0x4599e0,0x459a20,0x45a090,0x45dec1,0x4423c6,0x44261f,0x4427bd,0x456c20]
 for a in addresses:vm.hook_add(UC_HOOK_CODE,hook,begin=a,end=a)
 def track(a):return {'frame':i32(vm,objects[a]+0x1c),'lastTick':i32(vm,objects[a]+0x28),'first':i32(vm,objects[a]+0x30)==1}
 def snapshot():
  result={name:{1:i8,2:i16,4:i32}[width](vm,a)for name,(a,width)in FIELDS.items()}
  for name in ['itemUsed','selectionChanged','cardsMoving']:result[name]=bool(result[name])
  result.update(cards=card_read(vm),items=list(vm.mem_read(0x4e6760,max(0,i16(vm,0x4e6508)))),selectedItem=None if i8(vm,0x4e665c)<0 else i8(vm,0x4e665c)+i8(vm,0x4e649c),
   enemyTracks=[track(0x4e6664+n*4)for n in range(25)],playerTracks=[track(0x4e6574+n*4)for n in range(13)],recoveryTrack=track(0x4e64f0),shieldTracks=[track(0x4e6784+n*4)for n in range(2)],
   backgroundTrack=track(0x4e6558),rightTracks=[track(0x4e64cc+n*4)for n in range(9)],leftTracks=[track(0x4e65e4+n*4)for n in range(10)],layers=layers.copy(),sounds=sounds.copy(),soundStops=sound_stops.copy(),soundSerial=sound_serial[0],
   rngState=u32(vm,0x4a3f84),rngDraws=draws[0],finished=i32(vm,base+0x18)!=0)
  return result
 actions=[];item_used=False
 vm.reg_write(UC_X86_REG_ESP,base);w32(vm,base+0x18,0)
 for frame in range(limit):
  current.clear();layers.clear();sounds.clear();sound_stops.clear();cards=card_read(vm);state=i8(vm,0x4e6714)
  if state==0 and i16(vm,0x4ba59c)==0 and len(cards)==5:
   selected=[n for n,c in enumerate(cards)if c['selected']]
   if strategy.startswith('item')and not item_used:
    if not selected:current.update(action='selectCard',index=0)
    elif i8(vm,0x4e665c)<0:current.update(action='selectItem',index=0)
    else:current.update(action='useItem');item_used=True
   elif not selected:current.update(action='selectCard',index=0)
   elif strategy=='combo'and any(not c['selected']and c['kind']==cards[selected[0]]['kind']for c in cards):current.update(action='selectCard',index=next(n for n,c in enumerate(cards)if not c['selected']and c['kind']==cards[selected[0]]['kind']))
   else:current.update(action='attack')
  action={key:value for key,value in current.items()}
  if current.get('action')=='selectCard':current['position']=RULES['cardPositions'][current['index']]
  elif current.get('action')=='selectItem':current['position']=RULES['heldBattlePositions'][current['index']]
  elif current.get('action')=='useItem':current['position']=[540,346]
  clock[0]+=step
  try:vm.emu_start(0x4445f9,STOP,count=1000000)
  except Exception:print('frame fail',frame,hex(vm.reg_read(UC_X86_REG_EIP)),hex(vm.reg_read(UC_X86_REG_ESP)));raise
  assert vm.reg_read(UC_X86_REG_EIP)==0x456c20,(frame,hex(vm.reg_read(UC_X86_REG_EIP)))
  # The render hook stops inside its call, before a return is executed.
  vm.reg_write(UC_X86_REG_ESP,base)
  after=snapshot();frames.append({'elapsedMs':step,**action,'after':after})
  if after['finished']:break
 return {'seed':seed,'initial':initial or {},'strategy':strategy,'frames':frames}

if __name__=='__main__':
 initial,combos,items=numeric();sequences=[]
 for seed,strategy,hp,power in [(1,'normal',30000,20),(42,'combo',30000,42),(1999,'normal',2000,3),(42,'combo',60000,100)]:sequences.append(sequence(seed,3000,{'hp':hp,'displayedHp':hp,'power':power},strategy))
 for item in range(10):sequences.append(sequence(42,350,{'items':[item],'hp':59000,'displayedHp':59000},f'item{item}'))
 for count in range(33,37):sequences.append(sequence(1999,350,{'items':[9]*32+[0]*(count-32),'hp':59000,'displayedHp':59000},'item-alias'))
 result={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),'initialization':initial,'valuations':valuations(),'combinations':combos,'items':items,'sequences':sequences}
 (ROOT/'tests/fixtures/original-final-battle.json').write_text(json.dumps(result,separators=(',',':')),encoding='utf8')
 print(json.dumps({'initialization':len(initial),'combinations':len(combos),'items':len(items),'frames':sum(len(s['frames'])for s in sequences)}))
