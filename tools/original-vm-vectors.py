"""Differential fixtures obtained by executing the reference x86 event rules.

Only file loading and presentation calls are hooked. The rule calculations,
branches, original rand(), casts, stores, and clamping execute unchanged.
The proprietary executable is local; generated fixtures contain observations.
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
BASE = PE.OPTIONAL_HEADER.ImageBase
MONEY = 0x4b648a
PLAYER_FIELDS = [0x4b6484, 0x4b6486, 0x4b6488, MONEY, 0x4b648e, 0x4b6490,
                 0x4b6492, 0x4b6494, 0x4b6496, 0x4b64a4, 0x4b64e2, 0x4b64e4]


def create_vm():
    vm = Uc(UC_ARCH_X86, UC_MODE_32)
    vm.mem_map(BASE, (PE.OPTIONAL_HEADER.SizeOfImage + 0xfff) & ~0xfff)
    vm.mem_write(BASE, PE.get_memory_mapped_image())
    vm.mem_map(0x10000000, 0x20000)
    invoke(vm, 0x4144c0)
    return vm


def invoke(vm, address, *args):
    stack = 0x1001ff00
    vm.mem_write(stack, struct.pack('<' + 'I' * (len(args) + 1), 0x1000, *[a & 0xffffffff for a in args]))
    vm.reg_write(UC_X86_REG_ESP, stack)
    vm.emu_start(address, 0x1000, count=1000000)
    assert vm.reg_read(UC_X86_REG_EIP) == 0x1000, hex(vm.reg_read(UC_X86_REG_EIP))
    return vm.reg_read(UC_X86_REG_EAX)


def read_i32(vm, address):
    return struct.unpack('<i', vm.mem_read(address, 4))[0]


def read_i16(vm, address):
    return struct.unpack('<h', vm.mem_read(address, 2))[0]


def write(vm, address, value, width=2):
    vm.mem_write(address, int(value).to_bytes(width, 'little', signed=True))


def return_call(vm, value=0, argument_bytes=0):
    stack = vm.reg_read(UC_X86_REG_ESP)
    destination = struct.unpack('<I', vm.mem_read(stack, 4))[0]
    vm.reg_write(UC_X86_REG_EAX, value & 0xffffffff)
    vm.reg_write(UC_X86_REG_ESP, stack + 4 + argument_bytes)
    vm.reg_write(UC_X86_REG_EIP, destination)


def snapshot(vm, fields):
    return {f'0x{a:08x}': read_i32(vm, a) if a == MONEY else read_i16(vm, a) for a in fields}


def selector_case(name, seed, fields):
    vm = create_vm()
    for address, value in fields.items():
        write(vm, address, value, 4 if address == MONEY else 2)
    write(vm, 0x4a3f84, seed, 4)
    observed = {'loads': [], 'rngDraws': 0, 'skillTriggers': []}

    def hook(vm, address, size, _):
        stack = vm.reg_read(UC_X86_REG_ESP)
        if address == 0x409cb0:
            node = read_i32(vm, stack + 4)
            observed['loads'].append(node)
            if len(observed['loads']) == 1:
                dest = read_i32(vm, stack + 8)
                vm.mem_write(dest, struct.pack('<6h', 1, 28, 0, 0, 0, 0))
                return_call(vm, 1)
            else:
                return_call(vm, 0)
        elif address == 0x45dec1:
            observed['rngDraws'] += 1
        elif address == 0x41c2a0:
            observed['skillTriggers'].append(read_i16(vm, stack + 4))
            return_call(vm)
        elif address == 0x456c90:
            return_call(vm, argument_bytes=4)
        elif address in (0x425020, 0x409610, 0x456c20):
            return_call(vm)

    vm.hook_add(UC_HOOK_CODE, hook)
    before = snapshot(vm, fields)
    invoke(vm, 0x40c2e0, 1)
    assert len(observed['loads']) == 2, observed
    return {'name': name, 'seed': seed, 'fields': before, 'nextNode': observed['loads'][1],
            'rngDraws': observed['rngDraws'], 'rngState': read_i32(vm, 0x4a3f84) & 0xffffffff,
            'skillTriggers': observed['skillTriggers'],
            'after': snapshot(vm, [0x4b64ca, 0x4b64c2])}


base_fields = {0x4b64a2: -1, 0x4b64c2: 0, 0x4b64ca: 1500, 0x4b64c6: 0,
               0x4b649e: 2, 0x4b6498: 0, 0x4b64a6: 0, MONEY: 10000}
scenarios = [
    ('intro', {}), ('story1501', {0x4b64ca: 1501}),
    ('story_reject', {0x4b64ca: 1502}), ('story_accept', {0x4b64ca: 1506, 0x4b64c6: 1}),
    ('story_poor', {0x4b64ca: 1510, MONEY: 500000}),
    ('story_rich', {0x4b64ca: 1510, MONEY: 500001}),
    ('complete_student', {0x4b64c2: 1}),
    ('complete_career', {0x4b64c2: 1, 0x4b64a2: 3, 0x4b64a6: 2, 0x4b649e: 6}),
    ('career24', {0x4b64a2: 24, 0x4b64a6: 5, 0x4b649e: 6}),
    ('complete_female', {0x4b64c2: 1, 0x4b6498: 1, 0x4b649e: 4}),
]
selectors = [selector_case(name, seed, base_fields | changed)
             for name, changed in scenarios for seed in [0, 1, 2, 4, 42, 100, 1000, 12345]]


def reward_case(name, words, multiplier, fields, inventory=None):
    vm = create_vm()
    for address, value in fields.items():
        write(vm, address, value, 4 if address == MONEY else 2)
    write(vm, 0x4a4f50, 1, 4)
    write(vm, 0x4a4d88, multiplier, 4)
    inventory_fields = list(range(0x4b9ef8, 0x4b9f12, 2)) + list(range(0x4b9f14, 0x4b9f4a, 2))
    for address in inventory_fields:
        write(vm, address, (inventory or {}).get(address, 0))

    def hook(vm, address, size, _):
        if address == 0x409c20:
            stack = vm.reg_read(UC_X86_REG_ESP)
            dest = read_i32(vm, stack + 8)
            vm.mem_write(dest, struct.pack('<59H', *words))
            return_call(vm)

    vm.hook_add(UC_HOOK_CODE, hook)
    observed_fields = PLAYER_FIELDS + inventory_fields
    before = snapshot(vm, observed_fields)
    amplified = invoke(vm, 0x40bc30, words[0]) & 0xff
    after = snapshot(vm, observed_fields)
    return {'name': name, 'words': words, 'multiplier': multiplier, 'fields': before,
            'after': after, 'amplified': bool(amplified), 'lostLevel': read_i32(vm, 0x4a4f5c) == 1}


def result_words(**changes):
    words = [0xf000] * 59
    words[0] = 1
    words[8] = words[32] = 0xffff
    words[15] = 0
    for index, value in changes.items():
        words[int(index)] = value & 0xffff
    return words


rewards = [
    reward_case('positive_multiplier', result_words(**{'7': 100, '8': 0, '9': 5, '10': -2}), 3, {}),
    reward_case('level_loss', result_words(**{'5': 0}), 1, {0x4b64a4: 3, 0x4b6488: 450}),
    reward_case('normal_inventory_cap', result_words(**{'14': 3, '15': 2}), 2, {}, {0x4b9efa: 7}),
    reward_case('special_inventory_cap', result_words(**{'14': 3, '15': 21}), 2, {}, {0x4b9f14: 7}),
    reward_case('unlimited_item999', result_words(**{'14': 3, '15': 1}), 2, {}),
    reward_case('short_wrap', result_words(**{'9': 100}), 1, {0x4b6492: 32700}),
    reward_case('multiplier_wrap', result_words(**{'9': 20000, '31': 20000, '32': 0}), 3, {}),
]
output = {'sourceSha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
          'method': 'Unicorn executes original N2Game.exe x86; only record loading and presentation are hooked',
          'functions': {'selector': '0x0040c2e0 opcode 28', 'missionResult': '0x0040bc30'},
          'selectors': selectors, 'missionResults': rewards}
target = ROOT / 'tests/fixtures/original-vm.json'
target.write_text(json.dumps(output, ensure_ascii=False, indent=2) + '\n', encoding='utf8')
print(f'Generated {len(selectors)} ordinary-event and {len(rewards)} mission-result fixtures')
