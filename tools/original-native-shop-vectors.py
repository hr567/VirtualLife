"""Execute original native shop selection and property-income x86 blocks."""
import hashlib
import json
import struct
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP,UC_X86_REG_EIP,UC_X86_REG_EBX,UC_X86_REG_EBP
SOURCE=ROOT/'research/original/N2Game.exe';PE=pefile.PE(str(SOURCE));BASE=PE.OPTIONAL_HEADER.ImageBase
def vm_new():
    vm=Uc(UC_ARCH_X86,UC_MODE_32)
    vm.mem_map(BASE,(PE.OPTIONAL_HEADER.SizeOfImage+0xfff)&~0xfff);vm.mem_write(BASE,PE.get_memory_mapped_image())
    vm.mem_map(0x10000000,0x20000);vm.reg_write(UC_X86_REG_ESP,0x1001ff00)
    return vm
def write(vm,address,value,width=2):vm.mem_write(address,int(value).to_bytes(width,'little',signed=True))
def run(vm,start,stop):
    vm.emu_start(start,stop,count=100000)
    assert vm.reg_read(UC_X86_REG_EIP)==stop
def stock(seed,stage,career,sex):
    vm=vm_new();write(vm,0x4a3f84,seed,4);write(vm,0x4b64a2,career);write(vm,0x4b6498,sex)
    write(vm,0x4e5c50,6 if stage<4 else 10,4);write(vm,0x4e5d74,7 if stage<4 else 27,4)
    vm.reg_write(UC_X86_REG_EBX,0);vm.reg_write(UC_X86_REG_EBP,18)
    draws=[]
    vm.hook_add(UC_HOOK_CODE,lambda uc,a,n,u:draws.append(a) if a==0x45dec1 else None)
    run(vm,0x439be7,0x439d06)
    return {'seed':seed,'stage':stage,'career':career,'sex':sex,'draws':len(draws),
            'ordinary':list(struct.unpack('<3i',vm.mem_read(0x4e5ce8,12))),
            'special':list(struct.unpack('<3i',vm.mem_read(0x4e5cf4,12)))}
def income(stage,money):
    vm=vm_new();write(vm,0x4b9ecd,stage,1);write(vm,0x4b648a,money,4);vm.reg_write(UC_X86_REG_EBP,0)
    run(vm,0x42aa92,0x42ab64)
    return {'stage':stage,'initialMoney':money,'expectedMoney':struct.unpack('<i',vm.mem_read(0x4b648a,4))[0]}
cases=[stock(seed,stage,career,sex) for seed in [1,42,777] for stage,careers in [(2,[-1]),(6,range(-1,25))] for career in careers for sex in [0,1]]
incomes=[income(stage,money) for stage in range(2,14) for money in [0,12345,2147400000]]
result={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        'method':'Unicorn executes original selection block0x00439be7..00439d06 with loop registers initialized to original entry values and property income block0x0042aa92..0042ab64',
        'stock':cases,'income':incomes}
out=ROOT/'tests/fixtures/original-native-shops.json';out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(f'Generated {len(cases)} shop stock cases and {len(incomes)} property income cases')
