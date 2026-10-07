#!/usr/bin/env bash
# Regenerates tests/golden/charts.json from cases/charts.json through the web's ChartView on the Linux host.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
K="-i $USERPROFILE/.ssh/id_ed25519_kubuno -o UserKnownHostsFile=$USERPROFILE/.ssh/known_hosts_kubuno"
H=martinien@192.168.1.220
sed "s#'../../../../web/src/#'/home/martinien/projects/kubuno/office/web/src/#" "$here/charts-harness.tsx" > /tmp/ch.tsx
ssh $K $H 'mkdir -p /tmp/chartgold'
scp -q $K /tmp/ch.tsx "$here/cases/charts.json" $H:/tmp/chartgold/
ssh $K $H 'cd ~/projects/kubuno/office/web && TZ=UTC LANG=en_US.UTF-8 NODE_PATH=$PWD/node_modules JITI_JSX=1 node node_modules/jiti/lib/jiti-cli.mjs /tmp/chartgold/ch.tsx /tmp/chartgold/charts.json > /tmp/chartgold/out.json'
scp -q $K $H:/tmp/chartgold/out.json "$here/../../tests/golden/charts.json"
grep -o '"tag": "[a-z]*"' "$here/../../tests/golden/charts.json" | sort | uniq -c
