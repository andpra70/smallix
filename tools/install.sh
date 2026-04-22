#!/bin/bash

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain none -y
sudo apt install qemu xorriso qemu-system

source $HOME/.cargo/env
rustup toolchain install stable
rustup component add clippy
rustup component add rustfmt
rustup component add rust-analyzer
rustup component add rust-src
rustup component add llvm-tools-preview
rustup component add rust-docs
rustup component add rustc-dev
rustup component add rustc-docs

rustup toolchain install nightly --allow-downgrade --profile minimal --component clippy

