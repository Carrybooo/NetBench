#!/bin/bash
# receiver.sh
#
# Ce script bash lance l'exécutable receiver
cargo build
sudo ./target/debug/receiver $@
