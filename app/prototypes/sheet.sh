#!/bin/sh
# Contact sheet: sheet.sh <out.jpg> <cols> <width-per-tile> <png>...
out=$1; cols=$2; w=$3; shift 3
n=$#; inputs=""; filt=""; i=0
for f in "$@"; do inputs="$inputs -i $f"; filt="$filt[$i:v]scale=$w:-1,pad=$w+8:ih+8:4:4:0x101010[v$i];"; i=$((i+1)); done
layout=""; for j in $(seq 0 $((n-1))); do c=$((j % cols)); r=$((j / cols)); x=""; y="";
  [ $c -eq 0 ] && x=0 || x=$(for k in $(seq 1 $c); do printf "w0+"; done | sed 's/+$//')
  [ $r -eq 0 ] && y=0 || y=$(for k in $(seq 1 $r); do printf "h0+"; done | sed 's/+$//')
  layout="$layout${x}_${y}|"; done
layout=${layout%|}; streams=$(for j in $(seq 0 $((n-1))); do printf "[v$j]"; done)
ffmpeg -loglevel error -y $inputs -filter_complex "$filt${streams}xstack=inputs=$n:layout=$layout:fill=0x101010" -q:v 3 "$out"
