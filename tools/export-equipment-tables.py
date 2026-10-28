"""Export opcode 27's equipment prices, original descriptions and icons."""
import json
import struct
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile
pe = pefile.PE(str(ROOT / 'research/original/N2Game.exe'))
base = pe.OPTIONAL_HEADER.ImageBase
def read32(address): return struct.unpack('<I', pe.get_data(address-base, 4))[0]
def text(address): return pe.get_string_at_rva(address-base).decode('gbk')
count = read32(0x49438c)
assert count == 18
destination = ROOT / 'assets/catalog/rules.json'
catalog = json.loads(destination.read_text(encoding='utf8'))
catalog['equipment'] = [{'id': index, 'name': text(0x493eb6+index*70),
                         'price': read32(0x493cd8+index*4),
                         'icon': text(read32(0x493d60+index*4)).replace('\\', '/').lower(),
                         'description': text(read32(0x493c88+index*4))} for index in range(count)]
catalog['evidence']['equipmentWorkshop'] = '0x0043d130; tables 0x00493cd8, 0x00493eb6'
destination.write_text(json.dumps(catalog, ensure_ascii=False, indent=2) + '\n', encoding='utf8')
print(f'Exported {count} original equipment workshop records')
