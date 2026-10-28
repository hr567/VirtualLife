"""Execute original card combat routines with only graphics/audio/input stubs."""
import hashlib,json,struct,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
sys.path.insert(0,str(ROOT/'research/tools/python-packages'))
import pefile
from unicorn import Uc,UC_ARCH_X86,UC_MODE_32,UC_HOOK_CODE
from unicorn.x86_const import *
SOURCE=ROOT/'research/original/N2Game.exe';PE=pefile.PE(str(SOURCE));STOP=0x200ffff0
RULES=json.loads((ROOT/'assets/catalog/rules.json').read_text(encoding='utf8'))['battle']
def put(vm,a,v,size=4):vm.mem_write(a,int(v&((1<<(size*8))-1)).to_bytes(size,'little'))
def get(vm,a,size=4,signed=True):return int.from_bytes(vm.mem_read(a,size),'little',signed=signed)
INPUT={'hp':0x6486,'maxHp':0x6484,'experience':0x6488,'money':0x648a,'sex':0x6498,'stage':0x649e,'career':0x64a2,'careerLevel':0x64a6,'lifeLevel':0x64a4,'stamina':0x648e,'intelligence':0x6492,'morality':0x6490,'charm':0x6494,'missionKind':0x64ce,'missionTarget':0x64cc,'god':0x64d6,'traveling':0x64ac}
FIELDS={'enemy':(0x4ba7b8,2),'background':(0x4ba754,1),'playerClass':(0x4ba7b6,2),'hp':(0x4ba798,2),'maxHp':(0x4ba79a,2),'enemyHp':(0x4ba44c,2),'enemyMaxHp':(0x4ba44e,2),'money':(0x4ba79c,4),'experience':(0x4ba7a0,4),'cardCapacity':(0x4ba7b4,1),'cardCount':(0x4ba7c6,2),'playerCooldown':(0x4ba77c,2),'enemyCooldown':(0x4ba61a,2),'playerTenths':(0x4ba3e4,2),'enemyTenths':(0x4ba3e6,2),'playerSeconds':(0x4ba448,2),'enemySeconds':(0x4ba6d6,2),'remaining':(0x4ba7b2,2),'attackPower':(0x4ba5d8,2),'defensePower':(0x4ba5da,2),'enemyAttack':(0x4ba5d0,2),'enemyDefense':(0x4ba5d2,2),'bankPlayerAttack':(0x4ba5dc,2),'bankPlayerDefense':(0x4ba5de,2),'bankEnemyAttack':(0x4ba5d4,2),'bankEnemyDefense':(0x4ba5d6,2),'reflection':(0x4ba42c,1),'attackRepeats':(0x4ba4d8,1),'playerDamage':(0x4ba3dc,2),'enemyDamage':(0x4ba662,2)}
def before(**kw):
    x={'hp':100,'maxHp':100,'experience':0,'money':10000,'sex':0,'stage':2,'career':-1,'careerLevel':0,'lifeLevel':1,'stamina':100,'intelligence':100,'morality':100,'charm':100,'missionKind':-1,'missionTarget':-1,'god':-1,'traveling':False,'invulnerable':False,'equipped':[-1]*6,'carried':[{'id':-1,'level':0}for _ in range(8)]};x.update(kw);return x
def persistent(rank=0,hp=None):return {'enemies':[{'hp':row[0]if hp is None else hp,'respawnLevel':rank,'rank':rank,'encounters':rank*2,'wins':rank}for row in RULES['enemyHp']],'cards':[],'visits':1,'equippedIds':[-1]*6}
class Machine:
    def __init__(self,seed,inp,per):
        self.vm=vm=Uc(UC_ARCH_X86,UC_MODE_32);vm.mem_map(0,0x1000);vm.mem_map(0x400000,(PE.OPTIONAL_HEADER.SizeOfImage+4095)&~4095);vm.mem_write(0x400000,PE.get_memory_mapped_image());vm.mem_map(0x10000000,0x20000);vm.mem_map(0x20000000,0x100000);vm.mem_map(0x30000000,0x1000)
        vm.reg_write(UC_X86_REG_FPCW,0x023f);put(vm,0x4a3f84,seed);put(vm,0x46b110,0x30000000);self.clock=0;self.draws=0
        for name,offset in INPUT.items():put(vm,0x4b0000+offset,inp[name],4 if name in ['money','traveling']else 2)
        put(vm,0x49b89c,2 if inp['invulnerable']else 0)
        for i,item in enumerate(inp['carried']):put(vm,0x47ad64+i*4,item['id'],2);put(vm,0x47ad66+i*4,item['level'],2)
        for i,item in enumerate(inp['equipped']):put(vm,0x47ad94+i,item,1)
        for i,enemy in enumerate(per['enemies']):
            for key,address in [('hp',0x4b6a84+i*4),('respawnLevel',0x4b6a86+i*4),('encounters',0x4b9f4c+i*2),('wins',0x4b9f6c+i*2),('rank',0x4b9f8c+i*2)]:put(vm,address,enemy[key],2)
        put(vm,0x4b9ecc,per['visits'],1);put(vm,0x4ba7c6,len(per['cards']),2)
        self.write_cards(per['cards']);self.hook_handle=vm.hook_add(UC_HOOK_CODE,self.hook)
    def ret(self,args=0,value=0):
        vm=self.vm;sp=vm.reg_read(UC_X86_REG_ESP);vm.reg_write(UC_X86_REG_EIP,get(vm,sp,signed=False));vm.reg_write(UC_X86_REG_ESP,sp+4+args);vm.reg_write(UC_X86_REG_EAX,value&0xffffffff)
    def hook(self,vm,address,size,_):
        if address==0x30000000:self.ret(value=self.clock)
        elif address==0x45dec1:self.draws+=1
    def call(self,addr,args=[],until=STOP):
        vm=self.vm;sp=0x1001f000;put(vm,sp,STOP)
        for i,arg in enumerate(args):put(vm,sp+4+i*4,arg)
        vm.reg_write(UC_X86_REG_ESP,sp);vm.emu_start(addr,until,count=1000000)
        assert vm.reg_read(UC_X86_REG_EIP)==until,hex(vm.reg_read(UC_X86_REG_EIP))
    def write_cards(self,cards):
        for i,card in enumerate(cards):
            for name,off,size in [('kind',0,1),('attack',4,4),('defense',8,4)]:put(self.vm,0x4ba3f0+i*12+off,card[name],size)
    def snap(self,fields=FIELDS):
        x={name:get(self.vm,a,size)for name,(a,size)in fields.items()};x['rngState']=get(self.vm,0x4a3f84,signed=False);x['rngDraws']=self.draws;return x
    def cards(self):return [{'kind':get(self.vm,0x4ba3f0+i*12,1),'attack':get(self.vm,0x4ba3f4+i*12),'defense':get(self.vm,0x4ba3f8+i*12)}for i in range(get(self.vm,0x4ba7c6,2))]
def initialize(seed,inp,per):
    m=Machine(seed,inp,per);m.call(0x4330f0);m.call(0x431540)
    fields={k:v for k,v in FIELDS.items()if k not in ['reflection','playerDamage','enemyDamage']};initial=m.snap(fields)
    m.call(0x432580,until=0x432878);return {'seed':seed,'input':inp,'persistent':per,'initial':initial,'cards':m.cards(),'rngState':get(m.vm,0x4a3f84,signed=False),'rngDraws':m.draws}
def combo(kind,count,mod,level,seed=42):
    inp=before(stage=5,lifeLevel=25,career=7,careerLevel=3,hp=500,maxHp=600)
    if level>=0:inp['equipped']=[5,12,13,14,15,17];inp['carried']=[{'id':id,'level':level}for id in inp['equipped']]+[{'id':-1,'level':0}]*2
    m=Machine(seed,inp,persistent());m.call(0x4330f0);m.call(0x431540)
    cards=[{'kind':kind,'attack':37+i*2,'defense':19+i*3}for i in range(count)]
    if mod>=0:cards.append({'kind':mod,'attack':0,'defense':0})
    cards+= [{'kind':0,'attack':0,'defense':0}]*(5-len(cards));m.write_cards(cards)
    selected=[i<count+int(mod>=0)for i in range(5)]
    for i,value in enumerate(selected):put(m.vm,0x4ba668+i,value,1)
    m.call(0x4319e0,[2]);preview=m.snap();m.call(0x4319e0,[0]);committed=m.snap();m.call(0x4319e0,[1]);enemy=m.snap()
    return {'seed':seed,'input':inp,'cards':cards,'selected':selected,'preview':preview,'committed':committed,'enemy':enemy}
def damage(hp,enemy_hp,bank,invulnerable):
    inp=before(invulnerable=invulnerable);m=Machine(42,inp,persistent());m.call(0x4330f0);put(m.vm,0x4ba798,hp,2);put(m.vm,0x4ba44c,enemy_hp,2)
    for key,val in zip(['bankPlayerAttack','bankPlayerDefense','bankEnemyAttack','bankEnemyDefense'],bank):a,size=FIELDS[key];put(m.vm,a,val,size)
    m.call(0x431d00);m.call(0x431d90)
    return {'hp':hp,'enemyHp':enemy_hp,'bank':bank,'invulnerable':invulnerable,'after':m.snap(),'won':bool(get(m.vm,0x4ba64c,2)),'lost':bool(get(m.vm,0x4ba6c4,2))}

FRAME_FIELDS=dict(FIELDS,**{'attackPhase':(0x4ba6f2,2),'playerAttacking':(0x4ba778,4),'enemyAttacking':(0x4ba750,4),'playerActor':(0x4ba7c0,2),'enemyActor':(0x4ba7c2,2),'enemyEffect':(0x4ba7c4,2),'comboCount':(0x4ba7a4,1),'attackKind':(0x4ba6f0,2),'repeatCount':(0x4ba554,1),'strongAttack':(0x4ba44a,1),'strongEnemy':(0x4ba758,1),'godEffect':(0x4ba664,1),'won':(0x4ba64c,4),'lost':(0x4ba6c4,4),'displayedHp':(0x4ba444,4),'displayedEnemyHp':(0x4ba784,4),'playerDamageFrames':(0x4ba6b0,2),'enemyDamageFrames':(0x4ba74e,2),'dealing':(0x4ba59c,4)})
class BattleMachine(Machine):
    def __init__(self,seed,inp,per,step,mode,max_frames):
        self.active=False;self.rows=[];self.layers=[];self.sounds=[];self.sound_stops=[];self.sound_objects={};self.bonuses=[];self.step=step;self.mode=mode;self.max_frames=max_frames;self.choice=None;self.attack=False;self.tracks={};self.profile=None;self.finished=False
        super().__init__(seed,inp,per);self.call(0x4330f0);self.call(0x431540);vm=self.vm
        sex=inp['sex'];actor=get(vm,0x4ba7b6,2);enemy=get(vm,0x4ba7b8,2);r=RULES['rendering'];rows=[]
        for i in range(5):rows += [(f'player{i}',0x4ba624+i*4,r['actors'][sex][actor][i]),(f'enemy{i}',0x4ba738+i*4,r['enemyActors'][enemy][i]),(f'element{i}',0x4ba580+i*4,r['elementEffects'][i]),(f'combo{i+1}',0x4ba6fc+i*4,r['cardCombo'][i]),(f'hover{i}',0x4ba710+i*4,r['playerCardFlips'][i]),(f'selected{i}',0x4ba724+i*4,r['enemyCardFlips'][i])]
        rows += [('strong',0x4ba4e8,r['playerStrongEffects'][sex][actor])]
        for i in range(2):rows += [(f'enemyEffect{i}',0x4ba5a8+i*4,r['enemyStrongEffects'][enemy][i]),(f'shield{i}',0x4ba6b4+i*4,r['shields'][i]),(f'ready{i}',0x4ba438+i*4,r['healthFlares'][i])]
        for i in range(3):rows += [(f'god{i}',0x4ba788+i*4,r['godEffects'][i])]
        for index,(name,addr,track)in enumerate(rows):
            obj=0x20001000+index*0x1000;put(vm,addr,obj);self.tracks[name]=(obj,track)
            for off,value in [(0,0x20000000),(0xc,track['x']),(0x10,track['y']),(0x1c,0),(0x24,track['frameCount']),(0x28,0),(0x30,1),(0x34,track['interval'])]:put(vm,obj+off,value)
        put(vm,0x20000004,0x30000010);put(vm,0x20000008,0x30000010);put(vm,0x2000001c,0x30000020)
        for addr in [0x4ba578,0x4ba57c,0x4ba434,0x4ba5e0,*range(0x4ba558,0x4ba578,4),*range(0x4ba450,0x4ba4d8,4),*range(0x4ba4f0,0x4ba518,4),*range(0x4ba670,0x4ba6b0,4),*range(0x4ba6dc,0x4ba6ec,4)]:put(vm,addr,0x200f0000)
        put(vm,0x200f0000,0x20000000);put(vm,0x4ba6d0,0x200f1000);put(vm,0x4b6454,0x200f2000);put(vm,0x200f2004,0x200f3000);put(vm,0x200f2008,0x200f4000);put(vm,0x4ba610,0x200f5000)
        for index,(name,address)in enumerate([('cooldownReady',0x4ba5bc),('card',0x4ba5c0),('showCards',0x4ba594),('shield',0x4ba4e4),('shieldBounce0',0x4ba5b4),('shieldBounce1',0x4ba5b8),('playerWeak',0x4ba3cc),('playerStrong',0x4ba3d0),('playerHit',0x4ba3d4),('playerLoss',0x4ba3d8),('enemyStrong',0x4ba4e0),('enemyDefence',0x4ba614),('enemyDamage',0x4ba6c8),*[(f'combo{i+1}',0x4ba760+i*4)for i in range(5)]]):
            obj=0x200e0000+index*0x100;put(vm,address,obj);self.sound_objects[obj]=name
        self.stack=0x1001e000;put(vm,self.stack+0x1c,0x200f5000)
        vm.reg_write(UC_X86_REG_ESP,self.stack);vm.reg_write(UC_X86_REG_EBX,0);self.active=True
        vm.hook_del(self.hook_handle)
        for address in [0x430f37,0x433c02,0x4345c8,0x4345f3,0x4347c9,0x433550,0x30000000,0x30000010,0x30000020,0x456c20,0x4597a0,0x42fc30,0x458610,0x458c50,0x458bf0,0x458c70,0x45a3e0,0x44c8c0,0x45bd90,0x458ef0,0x45ba70,0x45a370,0x45bae0,0x458ba0,0x458c90,0x458ce0,0x4596e0,0x458e40,0x456eb0,0x457100,0x457050,0x458550,0x458570,0x45a090,0x459a20,0x4599b0,0x4599e0,0x459b60,0x45dec1]:vm.hook_add(UC_HOOK_CODE,self.hook,begin=address,end=address)
    def hook(self,vm,address,size,_):
        if not self.active:return super().hook(vm,address,size,_)
        if address==0x430f37:
            # This span contains only repeated one-pixel bitmap device blits.
            # All HP arithmetic, text counters and cooldown updates follow it.
            vm.reg_write(UC_X86_REG_EIP,0x431137)
        elif address==0x433c02:
            self.clock+=self.step;self.layers=[];self.sounds=[];self.sound_stops=[];self.choice=None;self.attack=False
            if self.mode!='none' and not get(vm,0x4ba59c)and not get(vm,0x4ba778)and not get(vm,0x4ba750):
                selected=list(vm.mem_read(0x4ba668,5));cards=self.cards();want=[False]*len(cards)
                base=next((card['kind']for card in cards if card['kind']<4),None)
                if self.mode=='one':want[0:1]=[True]
                else:want=[card['kind']==base or card['kind']>=4 for card in cards]
                self.choice=next((i for i in range(len(cards))if bool(selected[i])!=want[i]),None)
                if self.choice is None:self.attack=True
        elif address in [0x4345c8,0x4345f3]:
            row=self.snap(FRAME_FIELDS);row.update(cards=self.cards(),selected=[bool(v)for v in vm.mem_read(0x4ba668,5)],cardX=[get(vm,0x4ba658+i*2,2)for i in range(5)],tracks={name:[get(vm,obj+off)for off in [0x1c,0x28,0x30,0xc,0x10]]for name,(obj,_)in self.tracks.items()},layers=self.layers[:],sounds=self.sounds[:],soundStops=self.sound_stops[:],resultStarted=bool(vm.reg_read(UC_X86_REG_ESI)))
            self.rows.append({'time':self.clock,'select':self.choice,'attack':self.attack,'after':row})
            if len(self.rows)>=self.max_frames:vm.emu_stop()
        elif address==0x4347c9:self.finished=True;vm.emu_stop()
        elif address==0x433550:
            sp=vm.reg_read(UC_X86_REG_ESP);self.bonuses.append([get(vm,sp+16),get(vm,sp+4)]);self.ret()
        elif address==0x30000000:self.ret(value=self.clock)
        elif address in [0x30000010,0x30000020]:self.ret(12 if address==0x30000010 else 16)
        elif address in [0x45a370,0x45a3e0]:
            name=self.sound_objects.get(vm.reg_read(UC_X86_REG_ECX))
            if name:(self.sounds if address==0x45a370 else self.sound_stops).append(name)
            self.ret(4 if address==0x45a370 else 0)
        elif address in [0x456c20,0x4597a0,0x42fc30,0x458610,0x458c50,0x458bf0,0x458c70,0x44c8c0,0x45bd90]:self.ret()
        elif address in [0x458ef0,0x45ba70,0x45bae0]:self.ret(4)
        elif address in [0x458ba0,0x458c90,0x458ce0,0x4596e0]:self.ret(16)
        elif address==0x458e40:self.ret(20)
        elif address==0x456eb0:self.ret(12)
        elif address==0x457100:self.ret(16)
        elif address==0x457050:self.ret(4)
        elif address in [0x458550,0x458570]:
            obj=vm.reg_read(UC_X86_REG_ECX)
            for name,(pointer,_)in self.tracks.items():
                if pointer==obj:self.layers.append([name,get(vm,obj+0x1c),get(vm,obj+0xc),get(vm,obj+0x10)])
            self.ret(4 if address==0x458550 else 8)
        elif address==0x45a090:put(vm,0x200f101c,int(self.attack));self.ret(4)
        elif address==0x459a20:
            sp=vm.reg_read(UC_X86_REG_ESP);x=get(vm,sp+4);y=get(vm,sp+8);clicked=self.choice is not None and[x,y]==RULES['cardPositions'][self.choice];self.ret(16,int(clicked))
        elif address in [0x4599b0,0x4599e0]:self.ret(16)
        elif address==0x459b60:self.ret(4)
        elif address==0x45dec1:self.draws+=1
    def run(self):
        try:self.vm.emu_start(0x433bf4,STOP,count=300000000)
        except Exception as error:raise RuntimeError(f'{error} at {self.vm.reg_read(UC_X86_REG_EIP):x} frame {len(self.rows)} ECX={self.vm.reg_read(UC_X86_REG_ECX):x} combo={get(self.vm,0x4ba7a4,1)} phase={get(self.vm,0x4ba6f2,2)} player={get(self.vm,0x4ba7c0,2)} enemy={get(self.vm,0x4ba7c2,2)}')from error
        return {'frames':self.rows,'finished':self.finished,'bonuses':self.bonuses,'settled':{'hp':get(self.vm,0x4b6486,2),'money':get(self.vm,0x4b648a),'experience':get(self.vm,0x4b6488,2),'enemyHp':get(self.vm,0x4b6a84+get(self.vm,0x4ba7b8,2)*4,2)}}
if __name__=='__main__':
    initial=[]
    for stage in range(8):
        for rank in [0,3,5,9]:initial.append(initialize(42,before(stage=stage,lifeLevel=1+rank*5),persistent(rank,0 if rank in [3,9]else None)))
    for id in range(18):
        for level in [0,2,4]:initial.append(initialize(1999,before(stage=6,career=13,careerLevel=5,lifeLevel=45,stamina=9999,intelligence=9999,charm=9999,morality=9999,equipped=[id]*6,carried=[{'id':id,'level':level}]+[{'id':-1,'level':0}]*7),persistent()))
    initial.append(initialize(42,before(stage=4,missionKind=2,missionTarget=16),persistent(4)))
    combos=[combo(kind,count,mod,level)for kind in range(4)for count in range(1,6)for mod in ([-1,4,7,10,14]if count<5 else[-1])for level in [-1,4]]
    damages=[damage(hp,ep,bank,immune)for hp,ep in [(100,200),(1,1),(32000,32000)]for bank in [[30,20,50,10],[0,100,100,100],[32000,-20000,32000,-20000]]for immune in [False,True]]
    sequence_inputs=[(42,before(),persistent(),100,'combo'),(42,before(),persistent(),16,'one'),(42,before(),persistent(),150,'none'),(1999,before(stage=6,invulnerable=True),persistent(4),100,'none'),(1,before(stage=6,hp=1),persistent(4),71,'none')]
    for sex in range(2):
        for stage in [0,1,3,4,5,6]:sequence_inputs.append((1999,before(sex=sex,stage=stage,hp=900,maxHp=900,lifeLevel=30,career=-1 if stage<5 else 7,careerLevel=5,god=1),persistent(),71,'combo'))
        sequence_inputs.append((1999,before(sex=sex,stage=6,career=18,careerLevel=5,hp=900,maxHp=900,lifeLevel=50),persistent(4),16,'combo'))
    for kind in range(4):
        per=persistent();per['cards']=[{'kind':kind,'attack':47,'defense':38}for _ in range(4)]+[{'kind':14,'attack':0,'defense':0}]
        sequence_inputs.append((42,before(stage=6,lifeLevel=40,hp=800,maxHp=800,god=1),per,100,'combo'))
    for id in [0,3,5,6,9,12,15]:
        sequence_inputs.append((42,before(stage=6,lifeLevel=40,hp=800,maxHp=800,equipped=[id]+[-1]*5,carried=[{'id':id,'level':4}]+[{'id':-1,'level':0}]*7),persistent(3),100,'combo'))
    sequences=[]
    for seed,inp,per,step,mode in sequence_inputs:
        result=BattleMachine(seed,inp,per,step,mode,6000).run();result.update(seed=seed,input=inp,persistent=per,step=step,mode=mode);sequences.append(result)
        print(f'Battle sequence {len(sequences)}: {len(result["frames"])} frames',flush=True)
    out={'sourceSha256':hashlib.sha256(SOURCE.read_bytes()).hexdigest(),'method':'Original x86 0x4330f0/0x431540/0x432580/0x4319e0/0x431d00/0x431d90 and complete 0x433bf4 combat loop through 0x4347c9. Native FLC clocks and CRT RNG run unchanged; device calls and separately verified BonusAnimation are hooked. x87 control word 0x023f.','initial':initial,'combos':combos,'damage':damages,'sequences':sequences}
    out['frameFields']=list(sequences[0]['frames'][0]['after'])
    out['trackNames']=list(sequences[0]['frames'][0]['after']['tracks'])
    for case in sequences:
        previous={}
        for frame in case['frames']:
            current=frame['after']['tracks'];frame['after']['tracks']=[[i,values]for i,name in enumerate(out['trackNames'])if (values:=current[name])!=previous.get(name)];previous=current
            frame['after']['layers']=[[out['trackNames'].index(layer[0]),*layer[1:]]for layer in frame['after']['layers']]
            frame['after']=[frame['after'][name]for name in out['frameFields']]
    (ROOT/'tests/fixtures/original-battle.json').write_text(json.dumps(out,separators=(',',':'))+'\n',encoding='utf8')
    print(f'{len(initial)} initialization/card, {len(combos)} combination, {len(damages)} damage cases')
