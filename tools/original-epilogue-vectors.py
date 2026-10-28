"""Execute ending and credits update functions from the original x86 executable.

Graphics, audio and DirectShow are replaced by deterministic devices; original
input, subtitle, seek, scroll and fade branches execute unchanged. DirectShow
position uses elapsed milliseconds and both objects begin at a zero clock.
"""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_ECX,UC_X86_REG_EIP,UC_X86_REG_EAX
source=ROOT/'research/original/N2Game.exe';pe=pefile.PE(str(source));base=pe.OPTIONAL_HEADER.ImageBase
rules=json.loads((ROOT/'assets/catalog/rules.json').read_text(encoding='utf8'))['epilogue']
OBJ=0x20000000;COM=0x20002000;VTABLE=0x20003000;STACK=0x1001f000;STOP=0x30000ff0
def w32(vm,a,v):vm.mem_write(a,struct.pack('<I',v&0xffffffff))
def r32(vm,a):return struct.unpack('<i',vm.mem_read(a,4))[0]
def ret(vm,args=0,value=0):
 sp=vm.reg_read(UC_X86_REG_ESP);to=r32(vm,sp);vm.reg_write(UC_X86_REG_EAX,value&0xffffffff);vm.reg_write(UC_X86_REG_ESP,sp+4+args);vm.reg_write(UC_X86_REG_EIP,to)
def machine():
 vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(0,0x1000);vm.mem_map(base,(pe.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(base,pe.get_memory_mapped_image())
 vm.mem_map(0x10000000,0x20000);vm.mem_map(0x20000000,0x10000);vm.mem_map(0x30000000,0x1000)
 for offset in [0x14,0x18,0x44,0x68,8]:w32(vm,VTABLE+offset,0x30000100+offset)
 w32(vm,COM,VTABLE);w32(vm,0x4b6454,0x20004000)
 for offset in [0,4,8,0x10]:w32(vm,0x20004000+offset,COM)
 w32(vm,COM+0xa8,COM)
 imports={}
 for dll in pe.DIRECTORY_ENTRY_IMPORT:
  for imp in dll.imports:
   if imp.name in [b'GetTickCount',b'SetTextColor',b'SetBkMode',b'SetTextCharacterExtra',b'SelectObject',b'DrawTextA']:
    addr=0x30000200+len(imports)*4;imports[addr]=imp.name.decode();w32(vm,imp.address,addr)
 return vm,imports
def call(vm,address):
 vm.reg_write(UC_X86_REG_ESP,STACK);vm.reg_write(UC_X86_REG_ECX,OBJ);w32(vm,STACK,STOP)
 try:vm.emu_start(address,STOP,count=1000000)
 except Exception:
  print('failure',hex(address),hex(vm.reg_read(UC_X86_REG_EIP)),hex(vm.reg_read(UC_X86_REG_ESP)));raise
 assert vm.reg_read(UC_X86_REG_EIP)==STOP,hex(vm.reg_read(UC_X86_REG_EIP))
 assert vm.reg_read(UC_X86_REG_ESP)==STACK+4
 return bool(vm.reg_read(UC_X86_REG_EAX)&255)
movies=[]
for victory,step,escape_frame in [(False,17,None),(True,17,None),(False,201,None),(True,501,None),(True,17,10),(False,17,1000)]:
 vm,imports=machine();clock=[0];keys={};video=[0,0];seek=[0];rendered=[0];caption=[None];narration=[0]
 vm.mem_write(OBJ+4,b'\x01');w32(vm,OBJ+0x110,1);w32(vm,OBJ+0x10c,0x20005000)
 w32(vm,OBJ+0x244,0x4a2a48 if victory else 0x4a2af0);w32(vm,OBJ+0x24c,14 if victory else 9)
 for offset in [0x118,0x11c]:w32(vm,OBJ+offset,COM)
 def position():return video[1]+clock[0]-video[0]
 def hook(uc,address,size,_):
  sp=uc.reg_read(UC_X86_REG_ESP)
  if address in imports:
   name=imports[address];ret(uc,0 if name=='GetTickCount'else(20 if name=='DrawTextA'else 8),clock[0]if name=='GetTickCount'else 0)
  elif address==0x4597a0:ret(uc)
  elif address==0x459ba0:ret(uc,4,int(keys.pop(r32(uc,sp+4),False)))
  elif address==0x451b50:rendered[0]=position();ret(uc,value=int(r32(uc,OBJ+0x110)!=3 or position()<rules['outroMetadata']['durationMs']))
  elif address==0x451cb0:uc.mem_write(r32(uc,sp+4),struct.pack('<q',position()*10000));ret(uc,4,1)
  elif address==0x451c80:video[:]=[clock[0],struct.unpack('<q',uc.mem_read(sp+4,8))[0]//10000];seek[0]+=1;ret(uc,8,1)
  elif address==0x45a370:narration[0]=clock[0];ret(uc,4)
  elif address in [0x451750,0x45d754]:ret(uc)
  elif address==0x45d746:ret(uc,value=0x20005000)
  elif address==0x451730:ret(uc,value=uc.reg_read(UC_X86_REG_ECX))
  elif address==0x4517a0:video[:]=[clock[0],0];rendered[0]=0;caption[0]=None;seek[0]+=1;ret(uc,20,1)
  elif address==0x452526:caption[0]=r32(uc,OBJ+0x250)
  elif address in [0x30000114,0x30000144,0x30000168]:ret(uc,24 if address==0x30000114 else 8)
 for address in [*imports,0x4597a0,0x459ba0,0x451b50,0x451cb0,0x451c80,0x45a370,0x451750,0x45d754,0x45d746,0x451730,0x4517a0,0x452526,0x30000114,0x30000144,0x30000168]:vm.hook_add(UC_HOOK_CODE,hook,begin=address,end=address)
 frames=[]
 for frame in range(10000):
  clock[0]+=step;escape=frame==escape_frame;keys[1]=escape;alive=call(vm,0x452260)
  after={'phase':'finished'if not alive else('outro'if r32(vm,OBJ+0x110)==3 else'subtitles'),'clockMs':clock[0],
   'subtitleIndex':r32(vm,OBJ+0x250),'displayedSubtitle':caption[0],'segment':r32(vm,OBJ+0x248),
   'videoTimeMs':position(),'displayedVideoTimeMs':rendered[0],'narrationStarted':bool(vm.mem_read(OBJ+0x228,1)[0]),
   'narrationStartedMs':narration[0],'lastSubtitleMs':r32(vm,OBJ+0x254),'seekSerial':seek[0]}
  frames.append({'elapsedMs':step,'escape':escape,'after':after})
  if not alive:break
 assert not alive
 movies.append({'victory':victory,'step':step,'frames':frames})
credits=[]
for step,events in [(17,{}),(51,{0:['sDown']}),(50,{0:['escape'],49:['escape'],50:['sDown'],100:['sUp'],101:['escape']}),(2500,{0:['escape'],1:['escape']}),
                    (51,{0:['sDown'],5:['sUp','sDown','sUp'],7:['sDown'],8:['sDown'],10:['sUp'],13:['sDown','sUp'],52:['escape','sDown'],54:['sUp','escape']})]:
 vm,imports=machine();clock=[0];key_events=[]
 # Run the real 4597a0 poll and 459b60/459ba0 last-event helpers. Only the
 # DirectInput device calls are replaced; every poll replaces the event count.
 key_object=0x20006000;key_device=0x20007000;key_vtable=0x20007100
 w32(vm,0x4b640c,key_object);w32(vm,key_object+4,key_device);w32(vm,key_object+0x330,1)
 w32(vm,key_device,key_vtable);w32(vm,key_vtable+0x24,0x30000300);w32(vm,key_vtable+0x28,0x30000304)
 for offset,value in [(0x208,360),(0x20c,-1),(0x18c,rules['credits']['paragraphs'][-1]['y']),(0x214,-1),(0x110,COM)]:w32(vm,OBJ+offset,value)
 vm.mem_write(OBJ+0x108,b'\x01');vm.mem_write(OBJ+0x620,b'\xe0\xe0\xe0')
 def hook(uc,address,size,_):
  sp=uc.reg_read(UC_X86_REG_ESP)
  if address in imports:
   assert imports[address]=='GetTickCount';ret(uc,value=clock[0])
  elif address==0x30000300:ret(uc,12)
  elif address==0x30000304:
   data=r32(uc,sp+12);count=r32(uc,sp+16)
   for index,action in enumerate(key_events):uc.mem_write(data+index*16,struct.pack('<IIII',1 if action=='escape'else 31,0x80 if action=='sDown'else 0,clock[0],index))
   w32(uc,count,len(key_events));ret(uc,20)
  elif address==0x456c20:ret(uc)
  elif address==0x45d260:ret(uc,value=0)
  elif address==0x30000114:ret(uc,24)
 for address in [*imports,0x30000300,0x30000304,0x456c20,0x45d260,0x30000114]:vm.hook_add(UC_HOOK_CODE,hook,begin=address,end=address)
 frames=[]
 for frame in range(10000):
  actions=events.get(frame,[])
  key_events[:]=actions
  clock[0]+=step;alive=call(vm,0x452de0)
  after={'clockMs':clock[0],'offset':r32(vm,OBJ+0x208),'speed':r32(vm,OBJ+0x20c),'lastScrollMs':r32(vm,OBJ+0x210),
   'fadeStartedMs':r32(vm,OBJ+0x10c),'fadingIn':bool(vm.mem_read(OBJ+0x108,1)[0]),'fadingOut':bool(vm.mem_read(OBJ+0x109,1)[0]),
   'brightness':vm.mem_read(OBJ+0xa20,1)[0]if alive else None,'finished':not alive}
  frames.append({'elapsedMs':step,'actions':actions,'after':after})
  if not alive:break
 assert not alive
 credits.append({'step':step,'frames':frames})
out={'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),'method':__doc__,'movies':movies,'credits':credits}
(ROOT/'tests/fixtures/original-epilogue.json').write_text(json.dumps(out,separators=(',',':')),encoding='utf8')
print(json.dumps({'movieFrames':sum(len(row['frames'])for row in movies),'creditsFrames':sum(len(row['frames'])for row in credits)}))
