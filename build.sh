#!/bin/bash
export SDKTARGETSYSROOT=/opt/nova-view-sdk/sysroots/cortexa72-poky-linux
export PKG_CONFIG_SYSROOT_DIR=$SDKTARGETSYSROOT
export PKG_CONFIG_PATH=$SDKTARGETSYSROOT/usr/lib/pkgconfig:$SDKTARGETSYSROOT/usr/share/pkgconfig
export CFLAGS="--sysroot=/opt/nova-view-sdk/sysroots/cortexa72-poky-linux"
export CXXFLAGS="--sysroot=/opt/nova-view-sdk/sysroots/cortexa72-poky-linux"
cargo build --features novaview --target aarch64-unknown-linux-gnu
