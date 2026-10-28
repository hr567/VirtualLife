"""Execute isolated original x86 functions in Unicorn, without an OS or I/O.

The executable stays local. The output contains only observable test vectors.
Requires tools/research-requirements.txt in research/tools/python-packages.
"""
import hashlib
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
from unicorn import Uc, UC_ARCH_X86, UC_MODE_32
from unicorn.x86_const import UC_X86_REG_ESP, UC_X86_REG_EAX

source = ROOT / 'research/original/N2Game.exe'
pe = pefile.PE(str(source))
vm = Uc(UC_ARCH_X86, UC_MODE_32)
base = pe.OPTIONAL_HEADER.ImageBase
vm.mem_map(base, (pe.OPTIONAL_HEADER.SizeOfImage + 0xfff) & ~0xfff)
vm.mem_write(base, pe.get_memory_mapped_image())
vm.mem_map(0x10000000, 0x10000)

def invoke(address):
    vm.reg_write(UC_X86_REG_ESP, 0x1000fff0)
    vm.mem_write(0x1000fff0, struct.pack('<I', 0x1000))
    vm.emu_start(address, 0x1000, count=100000)
    return vm.reg_read(UC_X86_REG_EAX)

vm.mem_write(0x4b6478, b'Test\0')
invoke(0x4144c0)
initial = bytes(vm.mem_read(0x4b6478, 0x7c))
cases = []
for seed in [0, 1, 42, 0x7fffffff, 0xffffffff]:
    vm.mem_write(0x4a3f84, struct.pack('<I', seed))
    values = [invoke(0x45dec1) for _ in range(16)]
    cases.append({'seed': seed, 'values': values,
                  'finalState': struct.unpack('<I', vm.mem_read(0x4a3f84, 4))[0]})
result = {
    'sourceSha256': hashlib.sha256(source.read_bytes()).hexdigest(),
    'method': 'Unicorn executes original x86 machine code; no reimplemented formulas used to generate expected values',
    'functions': {'initialize': '0x004144c0', 'rand': '0x0045dec1'},
    'initialPlayer': {'baseAddress': '0x004b6478', 'bytesHex': initial.hex(),
                      'words': {f'0x{0x4b6484 + i * 2:08x}': struct.unpack_from('<h', initial, 12 + i * 2)[0]
                                for i in range((len(initial) - 12) // 2)}},
    'random': cases,
}
out = ROOT / 'tests/fixtures/original-core.json'
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf8')
print(f'Generated {out}: {len(cases)} random sequences and original initial player bytes')
