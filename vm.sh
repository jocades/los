#!/usr/bin/env bash

qemu-system-x86_64 -drive format=raw,file=target/x86_64-los/debug/bootimage-los.bin
