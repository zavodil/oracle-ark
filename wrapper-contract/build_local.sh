#!/bin/bash

cargo near build non-reproducible-wasm
cp target/near/oracle_wrapper_contract.wasm ./res/
