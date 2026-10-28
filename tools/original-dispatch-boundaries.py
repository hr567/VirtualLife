"""Observe the original dispatcher's non-advancing default branch.

The real 0x40c2e0 instructions execute. Only table loading and presentation
calls are replaced; the loader supplies the unmodified first matching record.
A repeated load is observed eight times before the harness stops the loop.
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
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EAX, UC_X86_REG_EIP

SOURCE = ROOT / 'research/original/N2Game.exe'
PE = pefile.PE(str(SOURCE))
CATALOG = json.loads((ROOT / 'assets/catalog/events.json').read_text(encoding='utf8'))
NODES = {}
for record in CATALOG['program']['nodes']:
    words = record['words']
    key = struct.unpack('<h', struct.pack('<H', words[0]))[0]
    NODES.setdefault(key, words)


def observe(entry):
    vm = Uc(UC_ARCH_X86, UC_MODE_32)
    base = PE.OPTIONAL_HEADER.ImageBase
    vm.mem_map(base, (PE.OPTIONAL_HEADER.SizeOfImage + 0xfff) & ~0xfff)
    vm.mem_write(base, PE.get_memory_mapped_image())
    vm.mem_map(0x10000000, 0x20000)
    stack = 0x1001ff00
    vm.mem_write(stack, struct.pack('<II', 0x1000, entry))
    vm.reg_write(UC_X86_REG_ESP, stack)
    loads, presentation, repeated = [], [], 0

    def i32(address):
        return struct.unpack('<i', vm.mem_read(address, 4))[0]

    def return_call(value=0, pop=0):
        esp = vm.reg_read(UC_X86_REG_ESP)
        vm.reg_write(UC_X86_REG_EAX, value)
        vm.reg_write(UC_X86_REG_EIP, i32(esp))
        vm.reg_write(UC_X86_REG_ESP, esp + 4 + pop)

    def hook(vm, address, _size, _data):
        nonlocal repeated
        esp = vm.reg_read(UC_X86_REG_ESP)
        if address == 0x409cb0:
            node = i32(esp + 4)
            repeated = repeated + 1 if loads and loads[-1] == node else 1
            loads.append(node)
            if repeated == 8:
                vm.emu_stop()
                return
            words = NODES.get(node)
            if words is not None:
                vm.mem_write(i32(esp + 8), struct.pack('<6H', *words))
            return_call(int(words is not None))
        elif address in (0x40a950, 0x40b1a0, 0x40b360, 0x40b430, 0x40b640):
            argc = 2 if address in (0x40a950, 0x40b1a0) else 3
            presentation.append({'function': f'0x{address:08x}', 'args': [i32(esp + 4 + 4 * i) for i in range(argc)]})
            return_call()
        elif address == 0x456c90:
            return_call(pop=4)
        elif address in (0x425020, 0x409610, 0x456c20, 0x425270):
            return_call()

    vm.hook_add(UC_HOOK_CODE, hook)
    vm.emu_start(0x40c2e0, 0x1000, count=100000)
    assert repeated == 8, hex(vm.reg_read(UC_X86_REG_EIP))
    assert NODES[loads[-1]][1] == 0
    return {'entry': entry, 'loads': loads, 'loopNode': loads[-1],
            'loopRecord': NODES[loads[-1]], 'presentation': presentation,
            'outcome': 'original dispatcher repeats the same opcode-0 record without advancing'}


output = {
    'sourceSha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
    'function': '0x0040c2e0',
    'method': 'Original x86 dispatcher; first-match catalog loader and presentation hooks; stop after eight repeated loads',
    'cases': [observe(entry) for entry in (0, 661, 26287)],
}
target = ROOT / 'tests/fixtures/original-dispatch-boundaries.json'
target.write_text(json.dumps(output, ensure_ascii=False, indent=2) + '\n', encoding='utf8')
print(f'Observed {len(output["cases"])} original opcode-0 loops')
