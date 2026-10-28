"""Execute original promotion branches and salary arithmetic for parity fixtures.

The promotion function stops before sound/animation creation. Salary formatting
and drawing are hooked; original math, branches, casts and money stores execute.
"""
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EAX, UC_X86_REG_EIP, UC_X86_REG_EDI

SOURCE = ROOT / 'research/original/N2Game.exe'
PE = pefile.PE(str(SOURCE))
BASE = PE.OPTIONAL_HEADER.ImageBase

def write(vm, address, value, width=2):
    vm.mem_write(address, int(value).to_bytes(width, 'little', signed=True))

def create_vm():
    vm = Uc(UC_ARCH_X86, UC_MODE_32)
    vm.mem_map(0, 0x1000)
    vm.mem_map(BASE, (PE.OPTIONAL_HEADER.SizeOfImage + 0xfff) & ~0xfff)
    vm.mem_write(BASE, PE.get_memory_mapped_image())
    vm.mem_map(0x10000000, 0x20000)
    invoke(vm, 0x4144c0)
    return vm

def invoke(vm, address, *args, until=0x1000):
    stack = 0x1001ff00
    vm.mem_write(stack, struct.pack('<'+'I'*(len(args)+1), 0x1000, *[a & 0xffffffff for a in args]))
    vm.reg_write(UC_X86_REG_ESP, stack)
    vm.emu_start(address, until, count=100000)
    assert vm.reg_read(UC_X86_REG_EIP) == until, hex(vm.reg_read(UC_X86_REG_EIP))

def return_call(vm):
    stack=vm.reg_read(UC_X86_REG_ESP)
    destination=struct.unpack('<I',vm.mem_read(stack,4))[0]
    vm.reg_write(UC_X86_REG_ESP,stack+4)
    vm.reg_write(UC_X86_REG_EIP,destination)

def promotion_case(name,career,rank,stats,items=None,sex=0,override=0):
    vm=create_vm()
    for address,value in [(0x4b64a2,career),(0x4b64a6,rank),(0x4b6498,sex),(0x4b64e4,override),*zip([0x4b6490,0x4b6492,0x4b648e,0x4b6494],stats)]:
        write(vm,address,value)
    inventory=[0]*27
    for item in items or []: inventory[item]=1
    vm.mem_write(0x4b9f14,struct.pack('<27h',*inventory))
    invoke(vm,0x423780,0,until=0x423987)
    decision=vm.reg_read(UC_X86_REG_EDI)
    assert decision in [0,1,2]
    return {'name':name,'career':career,'rank':rank,'stats':stats,'special':inventory,'sex':sex,'override':override,
            'expectedRank':rank+{0:1,1:-1,2:0}[decision]}

promotion=[]
for career in range(24):
    for rank,amount in [(1,9999),(2,9999),(4,0),(5,9999)]:
        promotion.append(promotion_case(f'career{career}-rank{rank}-stats{amount}',career,rank,[amount]*4))
for career,primary in [(8,8),(13,14),(15,17)]:
    for sex in [0,1]:
        for items in [[primary],[primary+1],[primary,primary+1]]:
            promotion.append(promotion_case(f'career{career}-sex{sex}-items{items}',career,3,[9999]*4,items,sex))
    promotion.append(promotion_case(f'career{career}-override',career,3,[9999]*4,[],0,1))
for career in [-1,24]: promotion.append(promotion_case(f'career{career}',career,5,[9999]*4))

salary=[]
for career in range(-1,25):
    for rank,spouse,sex,landed in [(1,-1,0,0),(5,3,1,1)]:
        vm=create_vm()
        for address,value in [(0x4b64a2,career),(0x4b64a6,rank),(0x4b6498,sex)]: write(vm,address,value)
        write(vm,0x47ada5,spouse,1)
        write(vm,0x4b648a,12345,4)
        vm.hook_add(UC_HOOK_CODE,lambda uc,a,n,u: return_call(uc) if a in [0x45e47b,0x40e580] else None)
        invoke(vm,0x424210,0,landed,0)
        money=struct.unpack('<i',vm.mem_read(0x4b648a,4))[0]
        salary.append({'career':career,'rank':rank,'spouse':spouse,'sex':sex,'landed':bool(landed),'initialMoney':12345,'expectedMoney':money})

result={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        'method':'Original x86 in Unicorn; promotion stops at 0x00423987 before UI; salary hooks only integer formatting and text drawing',
        'functions':{'promotion':'0x00423780','salary':'0x00424210'},'promotion':promotion,'salary':salary,
        'unemployedBaseSalary':struct.unpack('<h',PE.get_data(0x47f076-BASE,2))[0]}
out=ROOT/'tests/fixtures/original-payday.json'
out.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(f'Generated {len(promotion)} promotion cases and {len(salary)} salary cases')
