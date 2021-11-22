#!/bin/sh
echo $TRAIN_CONFIG
/binaries/train $TRAIN_CONFIG /proc/1/fd/1 2>/proc/1/fd/2
