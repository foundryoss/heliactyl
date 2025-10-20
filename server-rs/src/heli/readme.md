Heli-rs uses the .heli language

A better version of json + toml

How data works

file
```
{
    node = {Heli-dev},
    key = {auto-helia[uuid]},
    rate-limits = {
        per-ip-sec = 10,
        per-ip-min = 120
    }
}
```

let heli = heli-parse("config.heli")
heli.node // this should send Heli-dev

now special cases when parsing if it finds 
auto-helia, types
number, uuid, text 

it will randomly generate and make for those, replacing the values in the config.heli file

heli.ratelimits.per-ip-sec // this should give 10