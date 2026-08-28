#!/usr/bin/env bash
set -euo pipefail
LIB=${1:?usage: native_abi.sh libbleradar_core.so}
readelf -Ws "$LIB" | awk '$7!="UND" && ($4=="FUNC" || $4=="OBJECT") {print $8}' | sed '/^$/d' | sort -u
