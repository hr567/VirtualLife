"""Export native gameplay records from the local reference image.

Run after export-rule-tables.py; only decoded data, never executable bytes,
is added to the public catalog. Addresses are pinned to the source SHA there.
"""
import json
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'research/tools/python-packages'))
import pefile

pe = pefile.PE(str(ROOT / 'research/original/N2Game.exe'))
base = pe.OPTIONAL_HEADER.ImageBase
def read(address, size): return pe.get_data(address-base, size)
def i16(address): return struct.unpack('<h', read(address, 2))[0]
def i32(address): return struct.unpack('<i', read(address, 4))[0]
def u32(address): return struct.unpack('<I', read(address, 4))[0]
def i8(address): return struct.unpack('<b', read(address, 1))[0]
def text(address, limit=None):
    data = read(address, limit) if limit else pe.get_string_at_rva(address-base)
    return data.split(b'\0', 1)[0].decode('gbk')

def shop_group(address, count):
    return [{'id': index, 'resource': text(address+index*48+28, 20).replace('\\', '/').lower(),
             'price': i32(address+index*48+20), 'amount': i32(address+index*48+24),
             'name': text(address+index*48, 20)} for index in range(count)]

catalog_path=ROOT / 'assets/catalog/rules.json'
catalog=json.loads(catalog_path.read_text(encoding='utf8'))
catalog['statShops']={
    'young':[shop_group(address,count) for address,count in [(0x47c218,3),(0x47c2a8,4),(0x47c368,5),(0x47c458,3)]],
    'adult':[shop_group(address,count) for address,count in [(0x47c4e8,14),(0x47c788,5),(0x47c878,10),(0x47ca58,5),(0x47cb48,10)]],
    'evidenceFunctions':['0x0042b930','0x0042c620','0x0042ce40'],
}
catalog['careers']=[{
    'id':index,'name':text(u32(0x476f34+index*4)),
    'morality':i16(0x47f0b0+index*16),'intelligence':i16(0x47f0b2+index*16),
    'stamina':i16(0x47f0b4+index*16),'charm':i16(0x47f0b6+index*16),'money':i32(0x47f0b8+index*16),
    'degreeRequired':i8(0x47f05c+index),
    'prerequisites':[{'id':i8(0x47ee10+index*6+entry*2),'level':i8(0x47ee11+index*6+entry*2)} for entry in range(3)],
    'promotionIncrement':i16(0x47a57e+i16(0x47a54c+index*2)*2),
    'promotionItem':i8(0x47f0bc+index*16),'baseSalary':i16(0x47f078+index*2),
    'evidenceFunctions':['0x0042d4b0','0x00423780','0x00424210'],
} for index in range(24)]
catalog['salaryMultipliers']=[i32(0x477a4c+level*4) for level in range(6)]
catalog['baseSalaries']=[i16(0x47f078+index*2) for index in range(25)]
catalog['spouseSalary']=[i16(0x477a64+index*2) for index in range(16)]
catalog['missions']=[{'id':index,'words':[i16(0x476a58+index*22+word*2) for word in range(11)]} for index in range(50)]
catalog['travelMaps']=[{'mapPath':text(u32(0x4771c4+index*4)).replace('\\','/'),
                        'musicPath':text(u32(0x4771d4+index*4)).replace('\\','/')} for index in range(4)]
catalog['evidence'].update({'missions':'0x00476a58','careerEligibility':'0x0042d4b0','statShop':'0x0042b930','travelMapPaths':'0x004771c4'})
catalog_path.write_text(json.dumps(catalog,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'youngProducts':sum(map(len,catalog['statShops']['young'])),
                  'adultProducts':sum(map(len,catalog['statShops']['adult'])),
                  'careers':len(catalog['careers']),'missions':len(catalog['missions']),
                  'travelMaps':catalog['travelMaps']},ensure_ascii=False))
