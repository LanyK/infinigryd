#!/bin/sh

## ~/.ssh/config must contain something like:
#
# Host agakauitai
#   Hostname 141.84.94.111
#   Port     54321
#   User     ammann
#
# Host haruku
#   Hostname 141.84.94.207
#   Port     54321
#   User     ammann
#

cargo build --release

if [ $? -eq 0 ] ; then
    strip -s target/release/infinigryd
    for HOST in agakauitai haruku
    do
        scp cfg/machines.cfg target/release/infinigryd ${HOST}:
    done
fi

# target/debug/actlib
