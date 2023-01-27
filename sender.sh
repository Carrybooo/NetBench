#!/bin/bash
# sender.sh
#
# Ce script bash lance l'exécutable sender
cargo build
sudo ./target/debug/sender $@
