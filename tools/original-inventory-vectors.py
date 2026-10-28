"""Run unchanged x86 daily inventory mutation branches, without graphics hooks."""
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EIP

SOURCE = ROOT / 'research/original/N2Game.exe'
PE = pefile.PE(str(SOURCE))
STACK = 0x1001f000

def put(vm, address, value, size=4): vm.mem_write(address, (value & ((1 << (size * 8)) - 1)).to_bytes(size, 'little'))
def read(vm, address, size=4): return int.from_bytes(vm.mem_read(address, size), 'little', signed=True)
def machine():
    vm = Uc(UC_ARCH_X86, UC_MODE_32)
    vm.mem_map(0x400000, (PE.OPTIONAL_HEADER.SizeOfImage + 4095) & ~4095)
    vm.mem_write(0x400000, PE.get_memory_mapped_image())
    vm.mem_map(0x10000000, 0x20000)
    vm.reg_write(UC_X86_REG_ESP, STACK)
    return vm

def execute(vm, start, stop):
    vm.emu_start(start, stop, count=10000)
    assert vm.reg_read(UC_X86_REG_EIP) == stop

def item(action, item_id, hp, max_hp):
    vm = machine()
    address, count = (0x4b9f14, 27) if action == 'dropSpecial' else (0x4b9ef8, 13)
    counts = [0] * count
    counts[item_id] = 2
    counts[(item_id + 1) % count] = 1
    for index, value in enumerate(counts): put(vm, address + index * 2, value, 2)
    put(vm, 0x4b6484, max_hp, 2)
    put(vm, 0x4b6486, hp, 2)
    # The selected row is 1; the list retains duplicate physical item copies.
    put(vm, STACK + 0x18, 1)
    put(vm, STACK + 0x10, 3)
    put(vm, STACK + (0xec if action == 'dropSpecial' else 0xc0), item_id)
    if action == 'use': start, stop = 0x4054f2, 0x405556
    elif action == 'dropOrdinary': start, stop = 0x405199, 0x4051b7
    else: start, stop = 0x406482, 0x40649c
    execute(vm, start, stop)
    return {'action': action, 'id': item_id,
            'before': {'counts': counts, 'hp': hp, 'maxHp': max_hp},
            'after': {'counts': [read(vm, address + index * 2, 2) for index in range(count)],
                      'hp': read(vm, 0x4b6486, 2), 'maxHp': read(vm, 0x4b6484, 2)}}

def equipment(action, item_id, mode):
    vm = machine()
    equipped = [-1] * 6
    for group in range(6): equipped[group] = group * 3
    equipped[item_id // 3] = [-1, item_id, item_id // 3 * 3 + (item_id + 1) % 3][mode % 3]
    known = [{'learned': index in [item_id, (item_id + 4) % 18], 'level': 4 if index == item_id else 0} for index in range(18)]
    carried = [{'id': -1, 'level': 0} for _ in range(8)]
    carried[6] = {'id': item_id, 'level': 4}
    carried[3] = {'id': (item_id + 4) % 18, 'level': 0}
    if mode >= 3: carried[1] = {'id': item_id, 'level': 2}
    vm.mem_write(0x4b9ed4, bytes(value for record in known for value in [int(record['learned']), record['level']]))
    vm.mem_write(0x47ad64, b''.join(struct.pack('<hh', record['id'], record['level']) for record in carried + [{'id': -1, 'level': 0}] * 2))
    vm.mem_write(0x47ad94, bytes(value & 255 for value in equipped))
    put(vm, STACK + 0x14, 1)
    put(vm, STACK + 0x10, 3)
    put(vm, STACK + 0xb4, item_id)
    start, stop = {'wear': (0x4071f7, 0x40721e), 'unequip': (0x407433, 0x40745b), 'drop': (0x407731, 0x4077a7)}[action]
    execute(vm, start, stop)
    raw = vm.mem_read(0x4b9ed4, 36)
    after_known = [{'learned': bool(raw[index * 2]), 'level': raw[index * 2 + 1]} for index in range(18)]
    after_carried = [dict(zip(['id', 'level'], struct.unpack('<hh', vm.mem_read(0x47ad64 + index * 4, 4)))) for index in range(8)]
    # Native UI scans ten storage words, but all acquisition paths only fill
    # the first eight. New-game initialization leaves the final two unused.
    assert bytes(vm.mem_read(0x47ad84, 8)) == struct.pack('<hhhh', -1, 0, -1, 0)
    return {'action': action, 'id': item_id, 'mode': mode,
            'before': {'known': known, 'carried': carried, 'equipped': equipped},
            'after': {'known': after_known, 'carried': after_carried,
                      'equipped': [read(vm, 0x47ad94 + index, 1) for index in range(6)]}}

items = [item('use', index, hp, max_hp) for index in range(13)
         for hp, max_hp in [(0, 100), (1, 101), (73, 101), (100, 100), (200, 100),
                            (-10, 100), (1, 1), (30000, 32000), (0, 0)]]
items += [item('dropOrdinary', index, 61, 100) for index in range(13)]
items += [item('dropSpecial', index, 61, 100) for index in range(27)]
equipment_cases = [equipment(action, index, mode) for action in ['wear', 'unequip', 'drop']
                   for index in range(18) for mode in range(6)]
out = {'sourceSha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
       'method': 'Unmodified native inventory mutation spans 0x4054f2/0x405199/0x406482/0x4071f7/0x407433/0x407731. No arithmetic or calls are hooked. Drop branches begin after accepted confirmation.',
       'items': items, 'equipment': equipment_cases}
(ROOT / 'tests/fixtures/original-inventory.json').write_text(json.dumps(out, separators=(',', ':')) + '\n', encoding='utf8')
print(f'Generated {len(items)} item and {len(equipment_cases)} equipment x86 inventory observations')
