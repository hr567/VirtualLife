"""Original shared bonus animation: transitions, skip semantics and CRT RNG."""
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_ECX,UC_X86_REG_EAX,UC_X86_REG_EIP
SOURCE=ROOT/'research/original/N2Game.exe'
PE=pefile.PE(str(SOURCE))
OBJECT=0x20000000
STOP=0x2000fff0
def write32(vm,address,value):vm.mem_write(address,struct.pack('<I',value&0xffffffff))
def i32(vm,address):return struct.unpack('<i',vm.mem_read(address,4))[0]

def sequence(before,delta,skip_at):
    vm=Uc(UC_ARCH_X86,UC_MODE_32)
    vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095)
    vm.mem_write(0x400000,PE.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x20000)
    vm.mem_map(OBJECT,0x10000)
    vm.mem_map(0x30000000,0x1000)
    clock=[0];skip=[False];units=[];draws=[0]
    write32(vm,0x4a3f84,42)
    for module in PE.DIRECTORY_ENTRY_IMPORT:
        for imp in module.imports:
            if imp.name==b'GetTickCount':write32(vm,imp.address,0x30000000)
    write32(vm,0x4b6454,0x20001000);write32(vm,0x20001008,0x20002000)
    write32(vm,0x200020a8,0x20003000);write32(vm,0x20003000,0x20004000)
    write32(vm,0x20004014,0x30000010)
    write32(vm,OBJECT+0x194,0x20005000)
    write32(vm,0x4b640c,0x20006000)
    write32(vm,OBJECT+0x1c,10)
    def ret(args=0,value=0):
        stack=vm.reg_read(UC_X86_REG_ESP)
        vm.reg_write(UC_X86_REG_EAX,value&0xffffffff)
        vm.reg_write(UC_X86_REG_ESP,stack+4+args)
        vm.reg_write(UC_X86_REG_EIP,i32(vm,stack))
    def hook(vm,address,size,_):
        if address==0x30000000:ret(value=clock[0])
        elif address==0x30000010:ret(24)
        elif address in [0x4597a0,0x45a420,0x4585c0]:ret()
        elif address==0x459ba0:ret(4,int(skip[0]))
        elif address==0x45a370:ret(4)
        elif address==0x45dec1:
            next_seed=(i32(vm,0x4a3f84)*214013+2531011)&0xffffffff
            units.append(((next_seed>>16)&0x7fff)%10);draws[0]+=1
    vm.hook_add(UC_HOOK_CODE,hook)
    def call(entry,args=()):
        stack=0x1001ff00;write32(vm,stack,STOP)
        for index,value in enumerate(args):write32(vm,stack+4+index*4,value)
        vm.reg_write(UC_X86_REG_ESP,stack);vm.reg_write(UC_X86_REG_ECX,OBJECT)
        vm.emu_start(entry,STOP,count=200000)
        assert vm.reg_read(UC_X86_REG_EIP)==STOP
        return vm.reg_read(UC_X86_REG_EAX)&255
    call(0x453ab0,[before,delta])
    rows=[]
    for frame in range(1000):
        clock[0]=frame*16;skip[0]=skip_at is not None and clock[0]>=skip_at;units.clear()
        active=call(0x453b00)
        rows.append({'time':clock[0],'active':bool(active),'fields':[i32(vm,OBJECT+i*4) for i in range(9)],
                     'units':list(units),'rngState':i32(vm,0x4a3f84)&0xffffffff,'rngDraws':draws[0]})
        if not active:break
    assert not rows[-1]['active']
    return {'before':before,'delta':delta,'skipAt':skip_at,'frames':rows}

out={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
     'method':'Original 0x453ab0, 0x453b00 and 0x453e30 execute; only clock, drawing, input and sound are hooked',
     'cases':[sequence(before,delta,skip) for before,delta,skip in [(1000,0,None),(1000,3,None),(1000,500,None),(1000,-30,None),(100,-500,None),(1000,500,2000),(1000,-500,2000)]]}
(ROOT/'tests/fixtures/original-bonus.json').write_text(json.dumps(out,indent=2)+'\n',encoding='utf8')
print(f'Generated {sum(len(c["frames"]) for c in out["cases"])} original bonus frames')
