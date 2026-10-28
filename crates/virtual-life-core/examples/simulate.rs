//! Exercise recovered gameplay using real disc catalogs. Stops at unresolved
//! native calls; it never skips a blocker or substitutes made-up mechanics.
use std::{env,fs,path::PathBuf};
use serde_json::{Value,json};
use virtual_life_core::engine::Runtime;

fn main(){
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let read=|name:&str|->Value{serde_json::from_str(&fs::read_to_string(root.join(format!("assets/catalog/{name}.json"))).unwrap()).unwrap()};
    let mut catalog=read("maps");catalog["events"]=read("events");catalog["rules"]=read("rules");catalog["questionnaires"]=read("questionnaires");
    let count=env::args().nth(1).and_then(|s|s.parse::<u64>().ok()).unwrap_or(1);
    let mut reports=Vec::new();
    for seed in 1..=count{
        let mut runtime=Runtime::new(&catalog.to_string()).unwrap();
        let mut snapshot:Value=serde_json::from_str::<Value>(&runtime.dispatch(&json!({"seq":1,"type":"newGame","profile":{"name":"原版回归","sex":"male","birthMonth":1,"birthDay":1,"bloodType":"A"},"seed":seed}).to_string())).unwrap()["snapshot"].clone();
        for action in 0..300{
            let seq=snapshot["lastSeq"].as_u64().unwrap()+1;
            let command=match snapshot["phase"].as_str().unwrap(){
                "playing"=>json!({"seq":seq,"type":"rollDice"}),
                "event"=>match snapshot["pending"]["kind"].as_str(){
                    Some("dialogue")=>json!({"seq":seq,"type":"advanceEvent"}),
                    Some("choice")=>json!({"seq":seq,"type":"chooseOption","index":0}),
                    Some("allocation")=>json!({"seq":seq,"type":"allocateStats","allocations":snapshot["pending"]["defaults"]}),
                    _=>break,
                },
                _=>break,
            };
            let reply:Value=serde_json::from_str(&runtime.dispatch(&command.to_string())).unwrap();
            if reply["ok"]!=true{panic!("seed {seed} action {action}: {reply}");}
            snapshot=reply["snapshot"].clone();
            let save=runtime.serialize();let response:Value=serde_json::from_str(&runtime.restore(&save)).unwrap();
            assert_eq!(response["ok"],true,"save round trip at {seed}:{action}: {response}");
        }
        reports.push(json!({"seed":seed,"turn":snapshot["turn"],"tileId":snapshot["tileId"],"phase":snapshot["phase"],"pending":snapshot["pending"]}));
    }
    println!("{}",serde_json::to_string_pretty(&reports).unwrap());
}
