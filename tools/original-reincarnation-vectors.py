"""Execute original reincarnation arithmetic and resets, skipping only UI lists."""
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP

SOURCE=ROOT/'research/original/N2Game.exe'
PE=pefile.PE(str(SOURCE));BASE=PE.OPTIONAL_HEADER.ImageBase

def invoke(vm,address):
    stack=0x1001ff00
    vm.mem_write(stack,struct.pack('<I',0x1000));vm.reg_write(UC_X86_REG_ESP,stack)
    vm.emu_start(address,0x1000,count=100000)
    assert vm.reg_read(UC_X86_REG_EIP)==0x1000

def return_call(vm,arguments=0):
    stack=vm.reg_read(UC_X86_REG_ESP)
    destination=struct.unpack('<I',vm.mem_read(stack,4))[0]
    vm.reg_write(UC_X86_REG_ESP,stack+4+arguments)
    vm.reg_write(UC_X86_REG_EIP,destination)

def case(index,generation,experience,stats):
    vm=Uc(UC_ARCH_X86,UC_MODE_32)
    vm.mem_map(BASE,(PE.OPTIONAL_HEADER.SizeOfImage+0xfff)&~0xfff)
    vm.mem_write(BASE,PE.get_memory_mapped_image());vm.mem_map(0x10000000,0x20000)
    invoke(vm,0x4144c0)
    # Fill every packed word so the fixture detects fields the original leaves
    # intact as well as all explicit resets, including overlapping int32 stores.
    vm.mem_write(0x4b6478,struct.pack('<98h',*[index*3+7]*98))
    for address,value in [(0x4b64c0,generation),(0x4b6488,experience),*zip([0x4b6492,0x4b6490,0x4b648e,0x4b6494],stats)]:
        vm.mem_write(address,struct.pack('<h',value))
    vm.mem_write(0x4b6508,struct.pack('<26h',*[i%5 for i in range(26)]))
    career_levels=[i%7 for i in range(25)]
    vm.mem_write(0x4b6458,bytes(career_levels))
    initial=list(vm.mem_read(0x4b6478,0xc4))
    vm.hook_add(UC_HOOK_CODE,lambda uc,a,n,u:return_call(uc,4 if a==0x419320 else 0) if a in [0x419320,0x4193c0] else None)
    invoke(vm,0x41b0c0)
    return {'generation':generation,'experience':experience,'stats':stats,'initialPlayer':initial,
            'expectedPlayer':list(vm.mem_read(0x4b6478,0xc4)),
            'initialCareerLevels':career_levels,'expectedCareerLevels':list(vm.mem_read(0x4b6458,25)),
            'expectedOrdinaryItems':list(struct.unpack('<13h',vm.mem_read(0x4b9ef8,26))),
            'expectedSpecialItems':list(struct.unpack('<27h',vm.mem_read(0x4b9f14,54)))}

cases=[case(i,*args) for i,args in enumerate([
    (1,1000,[6000,6000,6000,6000]),(2,9999,[9999,9999,9999,9999]),
    (10,0,[0,100,9999,32767]),(50,9999,[9999,0,-32768,32767]),
    (164,32767,[32767,-32768,-1,1]),(32767,-32768,[-32768]*4),
])]
out={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
     'function':'0x0041b0c0','method':'Original x86 in Unicorn; only linked-list UI history allocation/deallocation helpers are hooked',
     'cases':cases}
(ROOT/'tests/fixtures/original-reincarnation.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(f'Generated {len(cases)} full packed-player reincarnation cases')
