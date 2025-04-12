#!/usr/bin/env nu


def main [strategy: string, out_dir: path, ...durations: string] {
    for $duration in $durations {
        let out = $out_dir | path join $duration

        pueue add cargo run --release -- configs/wamr-wasmtime.yaml --silent --strategy $strategy $out -c 1 --time-limit $duration
    }
}
