"""Export the original relationship requirements, questions and scene records."""
import json
import struct
import sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
PE=pefile.PE(str(ROOT/'research/original/N2Game.exe'));BASE=PE.OPTIONAL_HEADER.ImageBase
def data(address,count):return PE.get_data(address-BASE,count)
def i16(address):return struct.unpack('<h',data(address,2))[0]
def i32(address):return struct.unpack('<i',data(address,4))[0]
def ptr(address):return struct.unpack('<I',data(address,4))[0]
def text(address):return PE.get_string_at_rva(address-BASE).decode('gbk')
def indirect(address):return text(ptr(address))
def resource(address):return indirect(address).replace('\\','/').lower()
people=[]
for sex in range(2):
    for index in range(8):
        row=sex*8+index
        people.append({'id':index,'playerSex':sex,'cost':i16(0x48bdb4+row*6),
                       'charmRequired':i16(0x48bdb6+row*6),'romanceRequired':i16(0x48bdb8+row*6),
                       'selectedPortrait':resource(0x48bd34+row*8),'portrait':resource(0x48bd38+row*8),
                       'introduction':indirect(0x48be7c+row*36),
                       'questions':[{'prompt':indirect(0x48be80+row*36+question*16),
                                     'answers':[indirect(0x48be84+row*36+question*16+answer*4) for answer in range(3)]}
                                    for question in range(2)]})
rules_path=ROOT/'assets/catalog/rules.json'
rules=json.loads(rules_path.read_text(encoding='utf8'))
rules['relationships']={'people':people,
    'playerPortraits':[[resource(0x48bc4c+(sex*29+index)*4) for index in range(29)] for sex in range(2)],
    'answerPrefixes':[text(address) for address in [0x48dd90,0x48dd88,0x48dd80]],
    'eventBases':[[i32(0x4738dc+sex*16+kind*4) for kind in range(4)] for sex in range(2)],
    'messages':{key:indirect(address) for key,address in [
        ('child',0x48be54),('junior',0x48be58),('welcome',0x48be5c),('meet',0x48be60),
        ('success',0x48be64),('rejection',0x48be68),('full',0x48be6c),('date',0x48be70),
        ('marriage',0x48be74),('confirmMarriage',0x48be78),('mission',0x49b34c)]},
    'requirementsFailed':text(0x48dd68),
    'evidenceFunctions':['0x00437a50','0x004379f0','0x00438480','0x004389d0','0x00439310','0x00439650','0x0040c2e0']}
rules_path.write_text(json.dumps(rules,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'people':len(people),'eventBases':rules['relationships']['eventBases']}))
