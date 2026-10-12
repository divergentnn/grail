#!/bin/bash
# Lab tool: the rest of the whole-set check (PORT_SPEC.md section 12), low priority, one shard after another:
# offsets 0, 2, 3 of every 4th showing (offset 1 = results/whole_subset.json, done 2026-10-12 16:40).
# ETA ~70 min on an otherwise busy RTX 3090; stuck if results/whole_rest_*.log gets no new "case" line for 5 min.
set -e
cd "$(dirname "$0")/.."
LAB=${LAB:-/media/tmp/hunt/grain-lab/grainscore}
FG=${GRAIL_FULLGRAIN_LIB:-/media/tmp/hunt/tools/dav1d-grain/build/src}
[ -f results/whole_set.json ] || nice -n 19 /media/tmp/hunt/.venv/bin/python tools/make_whole_set.py "$LAB" results/whole_set.json
for off in 0 2 3; do
  echo "$(date +%H:%M) whole set: offset $off start" >> progress.log
  nice -n 19 ./target/release/grail check-reference results/whole_set.json --every 4 --offset $off --threads 1 \
    --fullgrain-lib "$FG" --out results/whole_rest_$off.json > results/whole_rest_$off.log 2>&1
  echo "$(date +%H:%M) whole set: offset $off done: $(grep Spearman results/whole_rest_$off.log)" >> progress.log
done
python3 tools/readme_tables.py tools/README.template.md tools/README.correlation.md results README.md
