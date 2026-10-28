"""Execute recovered tracking-minigame setup and reward code for test vectors."""
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EBX, UC_X86_REG_EIP

SOURCE = ROOT / 'research/original/N2Game.exe'
PE = pefile.PE(str(SOURCE))

def create_vm():
    vm = Uc(UC_ARCH_X86, UC_MODE_32)
    vm.mem_map(0x400000, (PE.OPTIONAL_HEADER.SizeOfImage + 4095) & ~4095)
    vm.mem_write(0x400000, PE.get_memory_mapped_image())
    vm.mem_map(0x10000000, 0x20000)
    vm.mem_map(0x20000000, 0x10000)
    vm.reg_write(UC_X86_REG_ESP, 0x1001ff00)
    return vm

def i32(vm, address): return struct.unpack('<i', vm.mem_read(address, 4))[0]
def i16(vm, address): return struct.unpack('<h', vm.mem_read(address, 2))[0]
def write32(vm, address, value): vm.mem_write(address, struct.pack('<I', value & 0xffffffff))

def shuffle(seed, revealed):
    vm = create_vm()
    write32(vm, 0x4a3f84, seed)
    vm.mem_write(0x4b6a70, bytes([revealed]))
    for index in range(50):
        obj = 0x20000000 + index * 128
        write32(vm, 0x4b6960 + index * 4, obj)
        write32(vm, obj + 12, 140 + index // 10 * 70)
        write32(vm, obj + 16, 220)
    vm.emu_start(0x4282a0, 0x42835f, count=10000)
    assert vm.reg_read(UC_X86_REG_EIP) == 0x42835f
    stack = vm.reg_read(UC_X86_REG_ESP)
    return {'seed': seed, 'revealed': revealed, 'target': vm.mem_read(0x4b6930, 1)[0],
            'destinations': list(vm.mem_read(stack + 0x2c, 5)),
            'sprites': [i32(vm, stack + 0x3c + index * 4) for index in range(5)],
            'verticalDirections': [i32(vm, stack + 0x50 + index * 4) for index in range(5)],
            'rngState': i32(vm, 0x4a3f84) & 0xffffffff}

def reward(correct, intelligence):
    vm = create_vm()
    vm.reg_write(UC_X86_REG_EBX, correct)
    vm.mem_write(0x4b6492, struct.pack('<h', intelligence))
    def hook(vm, address, size, _):
        if address == 0x429140:
            stack = vm.reg_read(UC_X86_REG_ESP)
            destination = i32(vm, stack)
            vm.reg_write(UC_X86_REG_ESP, stack + 4)
            vm.reg_write(UC_X86_REG_EIP, destination)
    vm.hook_add(UC_HOOK_CODE, hook)
    vm.emu_start(0x4295d7, 0x429657, count=10000)
    assert vm.reg_read(UC_X86_REG_EIP) == 0x429657
    return {'correct': correct, 'before': intelligence, 'after': i16(vm, 0x4b6492)}

out = {'sourceSha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
       'method': 'Original x86 instructions executed with Unicorn; reward animation only is hooked',
       'shuffleEntry': '0x004282a0', 'rewardEntry': '0x004295d7',
       'shuffles': [shuffle(seed, target) for seed in [0, 1, 42, 1999, 0xffffffff] for target in range(5)],
       'rewards': [reward(correct, intelligence) for correct in range(4)
                   for intelligence in [0, 1, 99, 1000, 9500, 9999, 32700, -100]]}
destination = ROOT / 'tests/fixtures/original-minigames.json'
destination.write_text(json.dumps(out, indent=2) + '\n', encoding='utf8')
print(f'Generated {len(out["shuffles"])} shuffle and {len(out["rewards"])} reward vectors')
