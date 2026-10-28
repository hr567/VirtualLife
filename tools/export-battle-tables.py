"""Export native 15's card combat tables directly from the original PE image."""
import json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
PE=pefile.PE(str(ROOT/'research/original/N2Game.exe'));BASE=PE.OPTIONAL_HEADER.ImageBase
def unpack(fmt,a):return list(struct.unpack(fmt,PE.get_data(a-BASE,struct.calcsize(fmt))))
def rows(fmt,a,n):return [unpack(fmt,a+i*struct.calcsize(fmt))for i in range(n)]
def text(a):return PE.get_string_at_rva(unpack('<I',a)[0]-BASE).decode('gbk')
path=ROOT/'assets/catalog/rules.json';catalog=json.loads(path.read_text(encoding='utf8'))
battle={
 'playerCooldown':unpack('<5h',0x47ff70),'careerPower':unpack('<25h',0x47ff7c),
 'baseCards':rows('<6h',0x480000,4),'cardPositions':rows('<2h',0x47ffb0,5),
 'enemyCooldown':rows('<5h',0x481df8,15),'enemyHp':rows('<5h',0x482070,15),
 'modifiers':rows('<2h',0x481ee0,15),'combos':[[unpack('<2d',0x481f30+(kind*4+count)*16)for count in range(4)]for kind in range(4)],
 'rewards':rows('<5h',0x48210a,15),'enemyPower':rows('<2h',0x4821a0,15),
 'equipment':rows('<18h',0x4821e0,18),'enemyNames':[text(0x482e38+i*4)for i in range(15)],
 'enemyDescriptions':[text(0x482f6c+i*4)for i in range(15)],
 'introText':[[text(0x482468+(enemy*10+i)*4)for i in range(10)]for enemy in range(15)],
 'timeoutText':[text(0x4826c0+i*4)for i in range(15)],
 'winText':text(0x4826fc),'masteredWinText':text(0x482700),'masteredIntroText':text(0x482704),'lossText':text(0x482708),
}
if 'rendering'in catalog.get('battle',{}):battle['rendering']=catalog['battle']['rendering']
catalog['battle']=battle
catalog['evidence']['battle']='Native 15: 0x433680; initialization 0x4330f0; cards 0x432580; damage 0x4319e0/0x431d00/0x431d90; equipment 0x431540/0x431670/0x4317d0'
path.write_text(json.dumps(catalog,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('Exported original fifteen-enemy battle tables')
