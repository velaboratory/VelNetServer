#!/bin/bash

cd ..
sudo -u "$1" bash -c "~/.cargo/bin/cargo build --release"
