"""Export item-shop and housing tables from the local reference image."""
import json
import struct
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
PE=pefile.PE(str(ROOT/'research/original/N2Game.exe'));BASE=PE.OPTIONAL_HEADER.ImageBase
def read(address,size):return PE.get_data(address-BASE,size)
def i16(address):return struct.unpack('<h',read(address,2))[0]
def i32(address):return struct.unpack('<i',read(address,4))[0]
def u32(address):return struct.unpack('<I',read(address,4))[0]
def i8(address):return struct.unpack('<b',read(address,1))[0]
def text(address):return PE.get_string_at_rva(address-BASE).decode('gbk')
def items(count,prices,descriptions,resources,mapping=None):
    return [{'index':index,'inventoryId':i8(mapping+index) if mapping else index,
             'price':i32(prices+index*4),'description':text(u32(descriptions+index*4)),
             'resource':text(resources+index*70).replace('\\','/').lower()} for index in range(count)]
path=ROOT/'assets/catalog/rules.json'
rules=json.loads(path.read_text(encoding='utf8'))
rules['unemployedBaseSalary']=i16(0x47f076)
rules['itemShop']={
    'ordinaryYoung':items(6,0x48e1fc,0x490554,0x48dd98,0x49054c),
    'ordinaryAdult':items(10,0x48e22c,0x490298,0x48df40),
    'specialYoung':items(7,0x48e27c,0x490574,0x48f908,0x49056c),
    'specialAdult':items(27,0x48e298,0x490308,0x48faf8),
    'furniture':[{'index':index,'price':i32(0x48e304+index*4)//2,
                  'description':text(u32(0x490374+index*4)),
                  'resource':text(0x48e8e0+index*70).replace('\\','/').lower(),
                  'bonusAmount':i16(0x490460+(index+1)*4),'bonusAttribute':i16(0x490462+(index+1)*4)} for index in range(59)],
    'evidenceFunctions':['0x004399b0','0x0043a7b0','0x0043bf80'],
}
rules['housing']={
    'styles':[{'id':index,'description':text(u32(0x47add0+index*4)),
               'price':i32(0x47ae10+index*4),'bonuses':[i16(0x47adf0+index*8+stat*2) for stat in range(4)]} for index in range(4)],
    'investmentRows':[[i32(0x47ae20+row*20+col*4) for col in range(5)] for row in range(4)],
    'stageDescriptions':[text(u32(0x47ae6c+stage*4)) for stage in range(1,14)],
    'evidenceFunctions':['0x0042a550','0x0042a120','0x0042b2e0'],
}
path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'ordinaryYoung':len(rules['itemShop']['ordinaryYoung']),'ordinaryAdult':len(rules['itemShop']['ordinaryAdult']),
                  'specialYoung':len(rules['itemShop']['specialYoung']),'specialAdult':len(rules['itemShop']['specialAdult']),
                  'furniture':len(rules['itemShop']['furniture']),'houseStyles':len(rules['housing']['styles'])}))
