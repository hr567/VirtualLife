"""Differential fortune vectors: original outcome branches and stat clipping.

Questionnaire records are decoded from the original data catalog. Native sum,
outcome selection, signed arithmetic, and clamping run as unmodified x86.
Only the digit/FLIC presentation after clamping is omitted.
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
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EAX, UC_X86_REG_EDX, UC_X86_REG_EBP, UC_X86_REG_EDI, UC_X86_REG_ESI, UC_X86_REG_EIP

SOURCE = ROOT / 'research/original/N2Game.exe'
PE = pefile.PE(str(SOURCE))
STATS = [0x4b6490, 0x4b6492, 0x4b648e, 0x4b6494]
KINDS = {2: 0, 3: 1, 1: 2, 0: 3}

def create_vm():
    vm = Uc(UC_ARCH_X86, UC_MODE_32)
    vm.mem_map(0, 4096)
    vm.mem_map(0x400000, (PE.OPTIONAL_HEADER.SizeOfImage + 4095) & ~4095)
    vm.mem_write(0x400000, PE.get_memory_mapped_image())
    vm.mem_map(0x10000000, 0x20000)
    vm.reg_write(UC_X86_REG_ESP, 0x1001ff00)
    return vm

def i16(value): return struct.unpack('<h', struct.pack('<H', value & 0xffff))[0]
def read32(vm, address): return struct.unpack('<i', vm.mem_read(address, 4))[0]
def write16(vm, address, value): vm.mem_write(address, struct.pack('<h', i16(value)))
def write32(vm, address, value): vm.mem_write(address, struct.pack('<I', value & 0xffffffff))

def clipped_delta(current, delta, kind):
    vm = create_vm()
    for address in STATS: write16(vm, address, current)
    stack = vm.reg_read(UC_X86_REG_ESP)
    vm.mem_write(stack, struct.pack('<5I', 0x1000, delta & 0xffffffff, 0, 0, kind))
    vm.emu_start(0x437360, 0x43744b, count=1000)
    assert vm.reg_read(UC_X86_REG_EIP) == 0x43744b
    return i16(vm.reg_read(UC_X86_REG_EAX))

def fortune(record, choice, current):
    vm = create_vm()
    answers = [min(choice, len(q['options']) - 1) for q in record['questions']]
    count = len(answers)
    write16(vm, 0x4ba7e4, count)
    for address, value in zip(STATS, current): write16(vm, address, value)
    for index, answer in enumerate(answers): write16(vm, 0x4e5af4 + index * 2, answer)
    score = 0
    if record['format'] == 1:
        for q, question in enumerate(record['questions']):
            for answer, row in enumerate(question['percentages']):
                vm.mem_write(0x4e5288 + (q * 12 + answer) * 8, struct.pack('<4h', *row))
        entry, stop = 0x436f19, 0x437055
    else:
        score = sum(q['scores'][answer] for q, answer in zip(record['questions'], answers))
        for index, row in enumerate(record['percentages']):
            vm.mem_write(0x4e5ad4 + index * 8, struct.pack('<4h', *row))
        write16(vm, 0x4e5aec, len(record['thresholds']))
        vm.mem_write(0x4e5aee, struct.pack('<3h', *(record['thresholds'] + [0] * (3-len(record['thresholds'])))))
        write32(vm, 0x4e5b1c, score)
        write32(vm, 0x4e5b3c, len(record['questions'][-1]['options']) - 1)
        entry, stop = 0x437094, 0x43718b
    observed = {'percentages': None, 'outcome': None, 'after': list(current)}
    def hook(vm, address, size, _):
        if address == 0x436f90:
            observed['percentages'] = [i16(vm.reg_read(r)) for r in [UC_X86_REG_EDX, UC_X86_REG_EBP, UC_X86_REG_EDI, UC_X86_REG_ESI]]
        elif address == 0x4370c4:
            observed['outcome'] = vm.reg_read(UC_X86_REG_EAX) & 0xffff
            observed['percentages'] = list(struct.unpack('<4h', vm.mem_read(0x4e5ad4 + observed['outcome'] * 8, 8)))
        elif address == 0x437360:
            stack = vm.reg_read(UC_X86_REG_ESP)
            delta, kind = read32(vm, stack + 4), read32(vm, stack + 16)
            index = KINDS[kind]
            observed['after'][index] = current[index] + clipped_delta(current[index], delta, kind)
            vm.reg_write(UC_X86_REG_EIP, read32(vm, stack))
            vm.reg_write(UC_X86_REG_ESP, stack + 4)
    vm.hook_add(UC_HOOK_CODE, hook)
    vm.emu_start(entry, stop, count=10000)
    assert vm.reg_read(UC_X86_REG_EIP) == stop
    return {'recordId': record['id'], 'choice': choice, 'answers': answers,
            'score': score, 'before': current, **observed}

catalog = json.loads((ROOT / 'assets/catalog/questionnaires.json').read_text(encoding='utf8'))['fortune']['records']
cases = [fortune(record, choice, current) for record in catalog if record['runtimeCompatible']
         for choice, current in [(0, [1000, 1234, 5678, 9999]), (1, [1, 99, 5555, 9800]), (4, [9990, 9000, 8000, 7000])]]
out = {'sourceSha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
       'method': 'Original x86 scoring and clipping execute in Unicorn with decoded original records; animation omitted',
       'cases': cases}
destination = ROOT / 'tests/fixtures/original-fortune.json'
destination.write_text(json.dumps(out, indent=2) + '\n', encoding='utf8')
print(f'Generated {len(cases)} original questionnaire score/effect vectors')
