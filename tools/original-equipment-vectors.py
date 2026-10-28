"""Original x86 equipment purchase/research differential observations."""
import hashlib
import json
import struct
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32, UC_HOOK_CODE
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP
SOURCE = ROOT / 'research/original/N2Game.exe'
PE = pefile.PE(str(SOURCE))

def write32(vm, address, value): vm.mem_write(address, struct.pack('<I', value & 0xffffffff))
def read32(vm, address): return struct.unpack('<i', vm.mem_read(address, 4))[0]

def execute(mode, item, money, owned, slots, level=0):
    vm = Uc(UC_ARCH_X86, UC_MODE_32)
    vm.mem_map(0x400000, (PE.OPTIONAL_HEADER.SizeOfImage + 4095) & ~4095)
    vm.mem_write(0x400000, PE.get_memory_mapped_image())
    vm.mem_map(0x10000000, 0x20000)
    vm.reg_write(UC_X86_REG_ESP, 0x1001ff00)
    known = [{'learned': index in owned, 'level': level if index == item else 0} for index in range(18)]
    carried = [{'id': value, 'level': level if value == item else 0} for value in slots]
    vm.mem_write(0x4b9ed4, bytes(value for record in known for value in [int(record['learned']), record['level']]))
    vm.mem_write(0x47ad64, b''.join(struct.pack('<hh', record['id'], record['level']) for record in carried))
    write32(vm, 0x4b648a, money)
    write32(vm, 0x47ad60, 1)
    write32(vm, 0x4e5dc0, item)
    write32(vm, 0x4e5dfc, 0)
    vm.mem_write(0x4e5d9c, bytes([item]))
    def hook(vm, address, size, _):
        if address in [0x45bd90, 0x456eb0]:
            stack = vm.reg_read(UC_X86_REG_ESP)
            destination = read32(vm, stack)
            vm.reg_write(UC_X86_REG_EIP, destination)
            vm.reg_write(UC_X86_REG_ESP, stack + (16 if address == 0x456eb0 else 4))
    vm.hook_add(UC_HOOK_CODE, hook)
    start, stop = (0x43d98b, 0x43da14) if mode == 'purchase' else (0x43dc23, 0x43dd2f)
    vm.emu_start(start, stop, count=10000)
    assert vm.reg_read(UC_X86_REG_EIP) == stop
    raw = vm.mem_read(0x4b9ed4, 36)
    after_known = [{'learned': bool(raw[index * 2]), 'level': raw[index * 2 + 1]} for index in range(18)]
    after_carried = [dict(zip(['id', 'level'], struct.unpack('<hh', vm.mem_read(0x47ad64 + index * 4, 4)))) for index in range(8)]
    return {'mode': mode, 'id': item, 'before': {'money': money, 'known': known, 'carried': carried, 'first': True},
            'after': {'money': read32(vm, 0x4b648a), 'known': after_known, 'carried': after_carried, 'first': read32(vm, 0x47ad60) == 1}}

cases = []
for item in range(18):
    price = struct.unpack('<i', PE.get_data(0x93cd8 + item * 4, 4))[0]
    owned = [index for index in range(18) if index != item][:8]
    cases.extend([execute('purchase', item, price, [], [-1] * 8),
                  execute('purchase', item, price - 1, [], [-1] * 8),
                  execute('purchase', item, price, owned, [-1] * 8)])
    slots = [item] + [-1] * 7
    full = [index for index in range(1, 18) if index != item][:7] + [item]
    with_zero = [0] + [index for index in range(1, 18) if index != item][:6] + [item]
    cases.extend([execute('research', item, price, [item], slots),
                  execute('research', item, price, [item], full),
                  execute('research', item, price, [item], with_zero),
                  execute('research', item, price, [item], slots, 3),
                  execute('research', item, price, [item], slots, 4),
                  execute('research', item, price // 2 - 1, [item], slots)])
out = {'sourceSha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
       'method': 'Unmodified original x86 purchase/research branches; text reset and palette presentation hooked', 'cases': cases}
(ROOT / 'tests/fixtures/original-equipment.json').write_text(json.dumps(out, indent=2) + '\n', encoding='utf8')
print(f'Generated {len(cases)} equipment purchase/research vectors')
