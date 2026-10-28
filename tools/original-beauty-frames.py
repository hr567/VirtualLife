"""Trace original catwalk presentation state; only renderer, audio and clock are hooked."""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_ESI,UC_X86_REG_EIP,UC_X86_REG_EAX,UC_X86_REG_FPCW
source=ROOT/'research/original/N2Game.exe';pe=pefile.PE(str(source));base=pe.OPTIONAL_HEADER.ImageBase
rules=json.loads((ROOT/'assets/catalog/rules.json').read_text(encoding='utf8'))['competition'];STACK=0x1001d000
def w32(vm,a,v):vm.mem_write(a,struct.pack('<I',v&0xffffffff))
def r32(vm,a):return struct.unpack('<i',vm.mem_read(a,4))[0]
def ret(vm,args=0,value=0):
 sp=vm.reg_read(UC_X86_REG_ESP);to=r32(vm,sp);vm.reg_write(UC_X86_REG_EAX,value&0xffffffff);vm.reg_write(UC_X86_REG_ESP,sp+4+args);vm.reg_write(UC_X86_REG_EIP,to)
def snap_animation(vm,pointer):
 a=r32(vm,pointer)
 return {name:bool(r32(vm,a+offset))if name=='initialFrame'else r32(vm,a+offset)for name,offset in [('x',12),('y',16),('frame',28),('lastTick',40),('initialFrame',48)]}
sequences=[]
for sex,step in [(0,17),(1,71),(0,101)]:
 vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(base,(pe.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(base,pe.get_memory_mapped_image());vm.mem_map(0x10000000,0x20000);vm.mem_map(0x20000000,0x10000);vm.mem_map(0x30000000,0x1000)
 # Windows starts with the x87 default; the executable's CRT initializer
 # 0x46097d selects 53-bit precision through _controlfp(0x10000,0x30000).
 vm.reg_write(UC_X86_REG_FPCW,0x37f);vm.reg_write(UC_X86_REG_ESP,STACK);w32(vm,STACK,0x1000);vm.emu_start(0x46097d,0x1000,count=10000);assert vm.reg_read(UC_X86_REG_FPCW)==0x23f,hex(vm.reg_read(UC_X86_REG_FPCW))
 vm.reg_write(UC_X86_REG_ESP,STACK);vm.reg_write(UC_X86_REG_ESI,0);w32(vm,0x4a3f84,42);vm.mem_write(0x4b649e,struct.pack('<h',2));vm.mem_write(0x4b6494,struct.pack('<h',100));vm.mem_write(0x4b6498,struct.pack('<h',sex))
 w32(vm,0x4b6454,0x20000000);w32(vm,0x20000004,0x20000200);w32(vm,0x20000008,0x20000200);w32(vm,0x20000200,0x20000300)
 for offset in [4,8,28]:w32(vm,0x20000300+offset,0x30000020+offset)
 for pointer in [0x4e6f58,0x4e6f70,0x4e6f5c,*range(0x4e6fa4,0x4e6fc4,4)]:w32(vm,pointer,0x20000200)
 entries=[(0x4e6fc4,rules['beautyStage']),(0x4e6eb8,rules['judges']),(0x4e7050,rules['beautyPodium']),*[(0x4e7054+i*4,a)for i,a in enumerate(rules['podiumPlaces'])],*[(0x4e6fc8+i*12+j*4,a)for i,row in enumerate(rules['beautyActors'][sex])for j,a in enumerate(row)],(0x4e700c,{'frameCount':20,'interval':71,'x':0,'y':366})]
 for index,(pointer,a)in enumerate(entries):
  address=0x20001000+index*0x200;w32(vm,pointer,address);w32(vm,address,address+0x100)
  for offset,value in [(12,a['x']),(16,a['y']),(28,0),(36,a['frameCount']),(40,0),(48,1),(52,a['interval'])]:w32(vm,address+offset,value)
 for dll in pe.DIRECTORY_ENTRY_IMPORT:
  for imp in dll.imports:
   if imp.name==b'GetTickCount':w32(vm,imp.address,0x30000000)
 clock=[0];flips=[0]
 def hook(uc,address,size,_):
  if address==0x30000000:ret(uc,value=clock[0])
  elif address in [0x30000024,0x30000028]:ret(uc,12)
  elif address==0x3000003c:ret(uc,16)
  elif address in [0x458ef0,0x458550,0x45a370,0x45bae0,0x45ba70]:ret(uc,4)
  elif address in [0x4597a0,0x45a3e0,0x458610,0x45bd90,0x44ca10,0x45ab10]:ret(uc)
  elif address==0x456c20:flips[0]+=1;ret(uc);uc.emu_stop()
 vm.hook_add(UC_HOOK_CODE,hook);vm.emu_start(0x44a646,0x44a92b,count=10000);assert vm.reg_read(UC_X86_REG_EIP)==0x44a92b
 entry=0x44a92b;rows=[]
 for frame in range(10000):
  clock[0]+=step;before=flips[0];vm.emu_start(entry,0x44ae98,count=100000);entry=vm.reg_read(UC_X86_REG_EIP)
  if flips[0]==before:break
  state={'contestant':vm.mem_read(STACK+0x18,1)[0],'outerPhase':vm.mem_read(STACK+0x13,1)[0],'podiumPlace':vm.mem_read(STACK+0x12,1)[0]-1,
   'actors':[[snap_animation(vm,0x4e6fc8+i*12+j*4)for j in range(3)]for i in range(4)],'stage':snap_animation(vm,0x4e6fc4),'judges':snap_animation(vm,0x4e6eb8),'podium':snap_animation(vm,0x4e7050),'podiumPlaces':[snap_animation(vm,0x4e7054+i*4)for i in range(3)]}
  if entry==0x44a4a8:
   inner=STACK-0x30;state['walkPhase']=vm.mem_read(inner+0x13,1)[0];state['walkX']=struct.unpack('<d',vm.mem_read(inner+0x20,8))[0];state['walkY']=r32(vm,inner+0x18);state['walkComplete']=bool(r32(vm,inner+0x1c))
  rows.append({'elapsedMs':step,'resume':hex(entry),'after':state})
 assert entry==0x44ae98,hex(entry)
 sequences.append({'sex':sex,'step':step,'frames':rows})
out={'sourceSha256':hashlib.sha256(source.read_bytes()).hexdigest(),'method':'Original whole catwalk loop, CRT FPU initialization and FLC timing execute in Unicorn; only graphics/audio/time calls stubbed','fpuControlWord':'0x023f','sequences':sequences}
(ROOT/'tests/fixtures/original-beauty-frames.json').write_text(json.dumps(out,separators=(',',':')),encoding='utf8')
print(json.dumps({'sequences':[{k:s[k]for k in ['sex','step']}|{'frames':len(s['frames'])}for s in sequences]}))
