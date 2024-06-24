#!/usr/bin/bash
config=release
exe=/tmp/control-frontend/aarch64-unknown-linux-gnu/$config/launch-control
SDKTARGETSYSROOT=/opt/nova-view-sdk/sysroots/cortexa72-poky-linux PKG_CONFIG_SYSROOT_DIR=/opt/nova-view-sdk/sysroots/cortexa72-poky-linux PKG_CONFIG_PATH=/opt/nova-view-sdk/sysroots/cortexa72-poky-linux/usr/lib/pkgconfig:$SDKTARGETSYSROOT/usr/share/pkgconfig CFLAGS="--sysroot=/opt/nova-view-sdk/sysroots/cortexa72-poky-linux" CXXFLAGS="--sysroot=/opt/nova-view-sdk/sysroots/cortexa72-poky-linux" cargo build --target aarch64-unknown-linux-gnu --features novaview --$config

if [ -f $exe ]
then
   cp  $exe /home/deets/projects/private/nova-view/layers/meta-nova-view/recipes-far/launch-control/launch-control/
else
   echo "no target executable found at $exe"
   exit 1
fi
