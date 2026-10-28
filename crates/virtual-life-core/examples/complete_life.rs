//! Whole-life public-command probe. Never edits state or skips a rule blocker.
use std::{collections::{BTreeMap,BTreeSet},env,fs,path::PathBuf};
use serde_json::{Value,json};
use virtual_life_core::engine::Runtime;
fn action(kind:&str,name:&str,index:usize)->Value{json!({"type":format!("{kind}Action"),"action":name,"index":index})}
fn tick(kind:&str)->Value{json!({"type":format!("{kind}Tick"),"elapsedMs":71})}
fn command(s:&Value,seed:usize,iteration:usize)->Result<Value,String>{
    if s["phase"]=="playing"{return Ok(json!({"type":"rollDice"}));}
    let p=&s["pending"];let g=&p["state"];let phase=g["phase"].as_str().unwrap_or("");
    Ok(match p["kind"].as_str().unwrap_or(""){
        "dialogue"=>json!({"type":"advanceEvent"}),
        "allocation"=>json!({"type":"allocateStats","allocations":p["defaults"]}),
        "choice"=>{
            let eligible:Vec<&Value>=p["options"].as_array().ok_or("Choice without options")?.iter().filter(|o|o["disabled"]!=true).collect();
            let selected=eligible.iter().find(|o|o["text"].as_str().is_some_and(|s|s.contains("进入社会"))).copied()
                .or_else(||eligible.iter().find(|o|o["text"].as_str().is_some_and(|s|s.contains("离开"))).copied())
                .or_else(||eligible.first().copied()).ok_or("No enabled choice")?;
            json!({"type":"chooseOption","index":selected["index"]})
        },
        "relationship"=>action("relationship","close",0),
        "minigame"=>match(p["game"].as_str().unwrap_or(""),phase){
            ("tracking","memorize")=>action("minigame","startShuffle",0),
            ("tracking","choose")=>action("minigame","choose",(seed+iteration)%5),
            ("tracking","roundResult")=>action("minigame","nextRound",0),
            ("spiritSlots","spinning")=>action("minigame","stop",0),
            (_,"ready"|"help")=>action("minigame","start",0),
            ("gas","playing")=>action("minigame","throw",0),
            _=>tick("minigame"),
        },
        "competition"=>match phase{
            "menu"=>action("competition","choose",seed%3),
            "intro"|"quizIntro"|"quizResult"=>action("competition","continue",0),
            "quizQuestion"=>action("competition","answer",(iteration+seed)%4),
            _=>tick("competition"),
        },
        "battle"=>if p["game"]=="ordinary"{
            if g["help"]==true||phase=="intro"{action("battle","continue",0)}else if phase=="reward"{action("battle","finishRewardAnimation",0)}else{tick("battle")}
        }else{match phase{
            "intro"|"shop"|"opening"=>action("battle","continue",0),
            "playing" if g["combatState"]==0&&g["cardsMoving"]==false=>{
                let cards=g["cards"].as_array().ok_or("Final battle without cards")?;
                // Original cards form a same-kind combination: selecting another
                // kind clears the previous selection, so selecting all kinds
                // would be a driver loop rather than a game defect.
                let kind=cards.iter().find(|c|c["selected"]==true).or_else(||cards.first()).map(|c|&c["kind"]);
                if let Some((i,_))=cards.iter().enumerate().find(|(_,c)|Some(&c["kind"])==kind&&c["selected"]!=true){action("battle","selectCard",i)}
                else if !cards.is_empty(){action("battle","attack",0)}else{tick("battle")}
            },_=>tick("battle"),
        }},
        "ending"=>match phase{
            "summary"|"movie"=>action("ending","escape",0),
            "credits" if g["credits"]["clockMs"].as_u64().unwrap_or(0)>3000=>action("ending","escape",0),
            "endImage"=>action("ending","continue",0),
            "reincarnation"=>action("ending","choose",0),
            _=>tick("ending"),
        },
        "blocked"=>return Err(format!("Original-rule blocker: {}",p["text"])),
        kind=>return Err(format!("No probe driver for {kind}: {p}")),
    })
}
fn send(runtime:&mut Runtime,snapshot:&mut Value,mut command:Value)->Result<Vec<Value>,String>{
    command["seq"]=json!(snapshot["lastSeq"].as_u64().unwrap()+1);
    let response:Value=serde_json::from_str(&runtime.dispatch(&command.to_string())).unwrap();*snapshot=response["snapshot"].clone();
    if response["ok"]!=true{return Err(format!("Command {command}: {}",response["error"]));}
    Ok(response["events"].as_array().cloned().unwrap_or_default())
}
fn main(){
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read=|name:&str|->Value{serde_json::from_str(&fs::read_to_string(root.join(format!("assets/catalog/{name}.json"))).unwrap()).unwrap()};
    let mut catalog=read("maps");for name in ["events","rules","questionnaires"]{catalog[name]=read(name);}
    let args:Vec<String>=env::args().collect();let count=args.get(1).and_then(|s|s.parse::<usize>().ok()).unwrap_or(1);
    let limit=args.get(2).and_then(|s|s.parse::<usize>().ok()).unwrap_or(300_000);let first_seed=args.get(3).and_then(|s|s.parse::<usize>().ok()).unwrap_or(1);
    let resume=args.get(4).is_some_and(|s|s=="resume");
    let directory=root.join("output/life-probes");fs::create_dir_all(&directory).unwrap();let mut reports=Vec::new();let mut all_completed=true;
    for seed in first_seed..first_seed+count{
        let mut runtime=Runtime::new(&catalog.to_string()).unwrap();
        let mut s:Value=serde_json::from_str::<Value>(&runtime.dispatch(&json!({"seq":1,"type":"newGame","profile":{"name":"流程回归","sex":if seed%2==1{"male"}else{"female"},"birthMonth":1,"birthDay":1,"bloodType":"A"},"seed":seed}).to_string())).unwrap()["snapshot"].clone();
        let mut stages=BTreeSet::new();let mut activities=BTreeSet::new();let mut events:BTreeMap<String,usize>=BTreeMap::new();let mut error=None;
        let mut completed=false;let mut restored=0usize;let mut steps=0usize;let mut previous_steps=0u64;
        if resume{
            let save=fs::read_to_string(directory.join(format!("seed-{seed}.state.json"))).unwrap();
            let restored_state:Value=serde_json::from_str(&runtime.restore(&save)).unwrap();assert_eq!(restored_state["ok"],true,"{}",restored_state["error"]);s=restored_state["snapshot"].clone();
            fs::write(directory.join(format!("seed-{seed}.resumed.state.json")),save).unwrap();
            let old:Value=serde_json::from_str(&fs::read_to_string(directory.join(format!("seed-{seed}.json"))).unwrap()).unwrap();
            stages=serde_json::from_value(old["stages"].clone()).unwrap();activities=serde_json::from_value(old["activities"].clone()).unwrap();events=serde_json::from_value(old["events"].clone()).unwrap();restored=old["saveRoundTrips"].as_u64().unwrap()as usize;previous_steps=old["driverSteps"].as_u64().unwrap();
        }
        for i in 0..limit{
            steps=i+1;stages.insert(s["life"]["stage"].as_i64().unwrap_or(-1));
            if s["phase"]=="completed"||(events.contains_key("lifeCompleted")&&events.contains_key("reincarnated")){completed=true;break;}
            activities.insert(format!("{}:{}",s["pending"]["kind"].as_str().unwrap_or("map"),s["pending"]["game"].as_str().unwrap_or("")));
            let input=match command(&s,seed,i){Ok(v)=>v,Err(e)=>{error=Some(e);break;}};let timed=input["type"].as_str().unwrap().ends_with("Tick");
            let mut replies=match send(&mut runtime,&mut s,input){Ok(v)=>v,Err(e)=>{error=Some(e);break;}};
            if !timed{
                let kind=s["pending"]["kind"].as_str().unwrap_or("");
                if matches!(kind,"minigame"|"competition"|"battle"|"ending"|"relationship"){
                    let input=tick(kind);match send(&mut runtime,&mut s,input){Ok(v)=>replies.extend(v),Err(e)=>{error=Some(e);break;}}
                }
            }
            for event in replies{if let Some(kind)=event["type"].as_str(){*events.entry(kind.into()).or_default()+=1;}}
            if !timed||i%1000==0{
                let save=runtime.serialize();let result:Value=serde_json::from_str(&runtime.restore(&save)).unwrap();
                if result["ok"]!=true{error=Some(format!("Save restore failed: {}",result["error"]));break;}
                assert_eq!(result["snapshot"],s,"Save round trip seed {seed}:{i}");restored+=1;
            }
            if i%20000==0{eprintln!("seed {seed}: step {i}, {} turns, stage {}, {}",s["turn"],s["life"]["stage"],s["pending"]["game"]);fs::write(directory.join(format!("seed-{seed}.progress.state.json")),runtime.serialize()).unwrap();}
        }
        if !completed&&error.is_none(){error=Some("Probe driver reached its step limit".to_string());}
        let report=json!({"seed":seed,"completed":completed,"resumedFromSave":resume,"commands":s["lastSeq"],"driverSteps":previous_steps+steps as u64,"turns":s["turn"],"life":s["life"],"stages":stages,"activities":activities,"events":events,"saveRoundTrips":restored,"error":error});
        fs::write(directory.join(format!("seed-{seed}.json")),serde_json::to_string_pretty(&report).unwrap()).unwrap();
        if !completed{fs::write(directory.join(format!("seed-{seed}.state.json")),runtime.serialize()).unwrap();}
        eprintln!("{report}");all_completed&=completed;reports.push(report);
    }
    fs::write(directory.join("summary.json"),serde_json::to_string_pretty(&reports).unwrap()).unwrap();if !all_completed{std::process::exit(1);}
}
