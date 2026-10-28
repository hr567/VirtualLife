"""Execute the original hammer target update and scoring functions in Unicorn."""
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_ECX, UC_X86_REG_EAX, UC_X86_REG_EIP

SOURCE = ROOT / 'research/original/N2Game.exe'
PE = pefile.PE(str(SOURCE))
OBJECT = 0x20000000
STOP = 0x2000fff0
def i32(vm, address): return struct.unpack('<i', vm.mem_read(address, 4))[0]
def write32(vm, address, value): vm.mem_write(address, struct.pack('<I', value & 0xffffffff))

def create_vm(seed):
    vm = Uc(UC_ARCH_X86, UC_MODE_32)
    vm.mem_map(0x400000, (PE.OPTIONAL_HEADER.SizeOfImage + 4095) & ~4095)
    vm.mem_write(0x400000, PE.get_memory_mapped_image())
    vm.mem_map(0x10000000, 0x20000)
    vm.mem_map(OBJECT, 0x10000)
    vm.mem_map(0x30000000, 0x1000)
    write32(vm, 0x4a3f84, seed)
    clock = [0]
    imported = {}
    for module in PE.DIRECTORY_ENTRY_IMPORT:
        for imp in module.imports:
            name = imp.name.decode() if imp.name else ''
            if name in ['GetTickCount', 'IntersectRect']:
                stub = 0x30000000 + len(imported) * 16
                write32(vm, imp.address, stub)
                imported[stub] = name
    def ret(arg_bytes=0, value=0):
        stack = vm.reg_read(UC_X86_REG_ESP)
        vm.reg_write(UC_X86_REG_EAX, value & 0xffffffff)
        vm.reg_write(UC_X86_REG_ESP, stack + 4 + arg_bytes)
        vm.reg_write(UC_X86_REG_EIP, i32(vm, stack))
    def hook(vm, address, size, _):
        if address == 0x45a370: ret(4)
        elif address == 0x453ab0: ret(8)
        elif address in [0x4585c0, 0x454910] and getattr(vm, 'skip_rendering', False): ret()
        elif imported.get(address) == 'GetTickCount': ret(value=clock[0])
        elif imported.get(address) == 'IntersectRect':
            stack = vm.reg_read(UC_X86_REG_ESP)
            left = struct.unpack('<4i', vm.mem_read(i32(vm, stack + 8), 16))
            right = struct.unpack('<4i', vm.mem_read(i32(vm, stack + 12), 16))
            ret(12, int(max(left[0], right[0]) < min(left[2], right[2]) and max(left[1], right[1]) < min(left[3], right[3])))
    vm.hook_add(UC_HOOK_CODE, hook)
    return vm, clock

def call(vm, entry, args=()):
    stack = 0x1001ff00
    write32(vm, stack, STOP)
    for index, value in enumerate(args): write32(vm, stack + 4 + index * 4, value)
    vm.reg_write(UC_X86_REG_ESP, stack)
    vm.reg_write(UC_X86_REG_ECX, OBJECT)
    vm.emu_start(entry, STOP, count=10000)
    assert vm.reg_read(UC_X86_REG_EIP) == STOP
    return vm.reg_read(UC_X86_REG_EAX)

def state(vm):
    return {key: i32(vm, OBJECT + offset) for key, offset in [('kind', 4), ('phase', 8), ('hits', 12), ('frame', 16), ('lastTick', 24), ('spriteY', 32)]}

def sequence(seed, times):
    vm, clock = create_vm(seed)
    rows = []
    for time in times:
        clock[0] = time
        call(vm, 0x454910)
        rows.append({'time': time, **state(vm), 'rngState': i32(vm, 0x4a3f84) & 0xffffffff})
    return {'seed': seed, 'ticks': rows}

def hit(kind, hits, phase, x, y):
    vm, clock = create_vm(0)
    clock[0] = 100
    for offset, value in [(4,kind),(8,phase),(12,hits),(44,100),(48,100)]: write32(vm, OBJECT+offset,value)
    score = call(vm, 0x4547a0, [x,y,x+35,y+17])
    if score & 0x80000000: score -= 0x100000000
    return {'kind':kind,'hits':hits,'phase':phase,'x':x,'y':y,'score':score,'after':state(vm)}

def reward(score, before):
    vm, clock = create_vm(0)
    vm.skip_rendering = True
    clock[0] = 3001
    write32(vm, OBJECT+4, score)
    vm.mem_write(OBJECT+0x1b00,b'\x01')
    vm.mem_write(OBJECT+0x63c,b'\x01')
    vm.mem_write(0x4b6490,struct.pack('<h',before))
    vm.reg_write(UC_X86_REG_ECX,OBJECT)
    vm.reg_write(UC_X86_REG_ESP,0x1001ff00)
    vm.emu_start(0x4560a0,0x45620b,count=10000)
    assert vm.reg_read(UC_X86_REG_EIP)==0x45620b
    return {'score':score,'before':before,'after':struct.unpack('<h',vm.mem_read(0x4b6490,2))[0]}

out = {'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
       'method':'Original x86 target update and hit scoring; clock, sound and rectangle intersection are hooked',
       'sequences':[sequence(seed, range(16, 5001, 16)) for seed in [1,42,1999,0xffffffff]],
       'hits':[hit(kind,hits,phase,x,y) for kind in range(3) for hits in range(4) for phase in [1,2,3] for x,y in [(140,130),(0,0),(185,158)]],
       'rewards':[reward(score,before) for score in [80,119,120,149,150,999] for before in [0,1,99,9500,9999,-100,32700]]}
destination = ROOT / 'tests/fixtures/original-hammer.json'
destination.write_text(json.dumps(out,indent=2)+'\n',encoding='utf8')
print(f'Generated {len(out["sequences"])} frame sequences and {len(out["hits"])} hit cases')
