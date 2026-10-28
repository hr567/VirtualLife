"""Execute the original gas game loop, replacing only rendering/audio/I/O."""
import hashlib
import json
import struct
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_EBP,UC_X86_REG_EIP,UC_X86_REG_EAX,UC_X86_REG_ECX
SOURCE=ROOT/'research/original/N2Game.exe';PE=pefile.PE(str(SOURCE));BASE=PE.OPTIONAL_HEADER.ImageBase
STACK=0x1001e000
TARGETS=[(0x4b68cc,434,344,8,214),(0x4b68d0,434,344,4,171),(0x4b68d4,434,344,2,71),(0x4b68d8,434,344,2,71)]
ANIMATIONS=[(0x4b68ec,222,188,1,71),(0x4b6874,180,188,28,142),(0x4b6908,199,249,2,785),(0x4b685c,224,195,46,71),
 (0x4b68bc,185,180,20,71),(0x4b68f4,173,166,16,71),(0x4b68a4,192,137,5,71),(0x4b687c,180,164,23,71),(0x4b68b8,225,276,2,71),
 (0x4b6860,213,417,4,71),(0x4b6864,289,417,4,71),(0x4b6868,365,417,4,71)]
def w32(vm,address,value):vm.mem_write(address,struct.pack('<I',value&0xffffffff))
def r32(vm,address):return struct.unpack('<i',vm.mem_read(address,4))[0]
def ret(vm,args=0,value=0):
    stack=vm.reg_read(UC_X86_REG_ESP);target=r32(vm,stack)
    vm.reg_write(UC_X86_REG_EAX,value&0xffffffff);vm.reg_write(UC_X86_REG_ESP,stack+4+args);vm.reg_write(UC_X86_REG_EIP,target)
def animation(vm,pointer):
    address=r32(vm,pointer)
    return {key:(bool(r32(vm,address+offset)) if key=='initialFrame' else r32(vm,address+offset)) for key,offset in [('x',12),('y',16),('frame',28),('lastTick',40),('initialFrame',48),('interval',52)]}
def snapshot(vm,draws):
    return {'attempts':r32(vm,0x4b686c),'successes':vm.mem_read(STACK+0x17,1)[0],'results':[bool(v) for v in vm.mem_read(0x4b68e8,3)],
      'targetState':vm.mem_read(0x4b6910,1)[0],'targetSpeed':vm.mem_read(0x4b68e4,1)[0],'playerState':r32(vm,0x4b6890),'countdown':r32(vm,0x4b6894),'statePending':bool(r32(vm,0x4b6898)),
      'targets':[animation(vm,item[0]) for item in TARGETS],'animations':[animation(vm,item[0]) for item in ANIMATIONS],
      'draws':draws,'rngState':r32(vm,0x4a3f84)&0xffffffff}

sequences=[]
for name,seed,step,frames in [('idle',1,17,1500),('slow_idle',42,117,900),('hits',1999,17,2000),('misses',777,71,300),('yawning',2026,17,600)]:
    vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(BASE,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(BASE,PE.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x20000);vm.mem_map(0x20000000,0x10000);vm.mem_map(0x30000000,0x1000)
    w32(vm,0x4a3f84,seed);w32(vm,0x4b6454,0x20000000);w32(vm,0x20000000+4,0x20000200);w32(vm,0x20000000+8,0x20000200)
    w32(vm,0x20000200,0x20000300)
    for offset,args in [(4,12),(8,12),(28,16)]:w32(vm,0x20000300+offset,0x30000020+offset)
    for global_address in [0x4b68a0,0x4b68c4,0x4b68f0,0x4b689c,0x4b684c,0x4b6884,0x4b68a8,0x4b68e0,0x4b6850]:w32(vm,global_address,0x20000200)
    for index,(pointer,x,y,count,interval) in enumerate(TARGETS+ANIMATIONS):
        address=0x20001000+index*0x200;w32(vm,pointer,address);w32(vm,address,address+0x100);vm.mem_write(address+0x106,struct.pack('<H',interval))
        for offset,value in [(12,x),(16,y),(28,0),(36,count),(40,0),(48,1),(52,interval)]:w32(vm,address+offset,value)
    vm.mem_write(0x4b6878,b'\x03');vm.mem_write(0x4b68e4,b'\x03');w32(vm,0x4b686c,0);w32(vm,0x4b6890,0);w32(vm,0x4b6894,200);w32(vm,0x4b6898,0);vm.mem_write(0x4b6910,b'\0')
    vm.reg_write(UC_X86_REG_ESP,STACK);vm.reg_write(UC_X86_REG_EBP,0);w32(vm,STACK+0x18,4);clock=[0];pressed=[False];draws=[0];flips=[0]
    for dll in PE.DIRECTORY_ENTRY_IMPORT:
        for imp in dll.imports:
            if imp.name==b'GetTickCount':w32(vm,imp.address,0x30000000)
    def hook(uc,address,size,_):
        if address==0x45dec1:draws[0]+=1
        elif address==0x30000000:ret(uc,value=clock[0])
        elif address in [0x30000024,0x30000028]:ret(uc,12)
        elif address==0x3000003c:ret(uc,16)
        elif address in [0x458ef0,0x45a370,0x458550]:ret(uc,4)
        elif address==0x458570:ret(uc,8)
        elif address in [0x4597a0,0x45a3e0,0x45a440,0x458610]:ret(uc)
        elif address==0x458770:w32(uc,uc.reg_read(UC_X86_REG_ECX)+28,0);ret(uc)
        elif address==0x459ba0:ret(uc,4,int(pressed[0] and r32(uc,uc.reg_read(UC_X86_REG_ESP)+4)==0x39))
        elif address==0x456c20:flips[0]+=1;ret(uc);uc.emu_stop()
    vm.hook_add(UC_HOOK_CODE,hook)
    entry=0x42668e;rows=[];last_attempt=-1
    for frame in range(frames):
        clock[0]+=step;state=snapshot(vm,draws[0]);normal=entry in [0x42668e,0x426f42]
        target=state['targets'][state['targetState']]['x'];can_fire=state['playerState'] in [0,2]
        pressed[0]=(name=='hits' and normal and can_fire and 261<=target<=280) or (name=='misses' and normal and can_fire and frame%3==0) or (name=='yawning' and normal and state['playerState']==1 and frame%20==0)
        before=flips[0]
        vm.emu_start(entry,0x4273fe,count=30000)
        entry=vm.reg_read(UC_X86_REG_EIP)
        if flips[0]==before:break
        rows.append({'elapsedMs':step,'fire':pressed[0],'resume':hex(entry),'after':snapshot(vm,draws[0])})
        if rows[-1]['after']['attempts']>=3:break
    sequences.append({'name':name,'seed':seed,'frames':rows})
rewards=[]
for successes in range(4):
    for stamina in [0,1000,9900,9999,32760,-32768]:
        vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(BASE,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(BASE,PE.get_memory_mapped_image());vm.mem_map(0x10000000,0x20000)
        vm.reg_write(UC_X86_REG_ESP,STACK);vm.mem_write(STACK+0x17,bytes([successes]));vm.mem_write(0x4b648e,struct.pack('<h',stamina))
        vm.hook_add(UC_HOOK_CODE,lambda uc,address,size,_:ret(uc) if address==0x425490 else None)
        vm.emu_start(0x42737d,0x4273fe,count=1000)
        rewards.append({'successes':successes,'before':stamina,'after':struct.unpack('<h',vm.mem_read(0x4b648e,2))[0]})
result={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),'entry':'0x004265a0','sequences':sequences,'rewards':rewards}
destination=ROOT/'tests/fixtures/original-gas.json';destination.write_text(json.dumps(result,separators=(',',':')),encoding='utf8')
print(json.dumps({'frames':sum(len(s['frames']) for s in sequences),'sequences':[{'name':s['name'],'frames':len(s['frames']),'attempts':s['frames'][-1]['after']['attempts'],'successes':s['frames'][-1]['after']['successes'],'resume':sorted(set(f['resume'] for f in s['frames']))}for s in sequences]}))
